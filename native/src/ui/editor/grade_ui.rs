// Inspector pieces for colour and sound work: colour wheels, LUT files, blend modes and masks,
// loudness, ducking, EQ and beats.

use super::{Apply, Drag, Editor};
use crate::fx::registry::effect_def;
use crate::media::audio::{beats, buffer};
use crate::model::types::*;
use super::inspector::{item, MenuEntry};
use crate::ui::theme::*;
use gpui::prelude::*;
use gpui::{div, img, px, AnyElement, Context, Div, Pixels, RenderImage, SharedString};
use std::rc::Rc;
use std::sync::Arc;

const WHEEL: f32 = 92.0;

/// The hue disc behind each colour wheel, drawn once.
fn wheel_image() -> Arc<RenderImage> {
    if let Some(i) = crate::ui::images::cached("ui:wheel") {
        return i;
    }
    let n = 192u32;
    let mut pm = tiny_skia::Pixmap::new(n, n).unwrap();
    let r = n as f32 / 2.0;
    for (i, px) in pm.data_mut().chunks_exact_mut(4).enumerate() {
        let (x, y) = ((i as u32 % n) as f32 + 0.5 - r, (i as u32 / n) as f32 + 0.5 - r);
        let d = (x * x + y * y).sqrt() / r;
        let a = ((1.0 - d) * r).clamp(0.0, 1.0);
        if a <= 0.0 {
            continue;
        }
        let hue = (-y).atan2(x).to_degrees().rem_euclid(360.0);
        let [cr, cg, cb] = hsv(hue, d.min(1.0) * 0.9, 0.82);
        let a8 = (a * 255.0) as u32;
        px.copy_from_slice(&[(cr * a8 as f32) as u8, (cg * a8 as f32) as u8, (cb * a8 as f32) as u8, a8 as u8]);
    }
    let img = crate::ui::images::to_image(&pm);
    crate::ui::images::put("ui:wheel", img.clone());
    img
}
fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

impl Editor {
    /// One colour wheel: drag the puck towards a hue to tint that range; the slider under it
    /// brightens or darkens it.
    fn wheel(&self, c: &Clip, uid: &str, which: &'static str, label_text: &'static str, cx: &mut Context<Self>) -> Div {
        let lt = (self.store.time - c.start).clamp(0.0, c.dur);
        let hue = c.value(&format!("fx.{uid}.{which}_hue"), lt).unwrap_or(0.0);
        let amt = c.value(&format!("fx.{uid}.{which}_amt"), lt).unwrap_or(0.0);
        let key = format!("wheel-{uid}-{which}");
        let (px_, py_) = ((hue.to_radians().cos() * amt) as f32 * WHEEL / 2.0, (-(hue.to_radians().sin()) * amt) as f32 * WHEEL / 2.0);
        let (cid, uid2, key2) = (c.id.clone(), uid.to_string(), key.clone());
        let lum = self.keyed_slider(c, &format!("fx.{uid}.{which}_lum"), "", -1.0, 1.0, 0.01, |v| format!("{:+.0}", v * 100.0), None, cx);
        let (rid, ruid) = (c.id.clone(), uid.to_string());
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_1()
            .flex_1()
            .min_w_0()
            .child(type_micro(div()).text_color(ink_muted()).child(label_text))
            .child(
                div()
                    .id(SharedString::from(key.clone()))
                    .relative()
                    .size(px(WHEEL))
                    .cursor_crosshair()
                    .child(self.track_bounds(key))
                    .child(img(wheel_image()).absolute().size_full())
                    .child(div().absolute().left(px(WHEEL / 2.0 - 0.5)).top(px(WHEEL / 2.0 - 4.0)).w(px(1.)).h(px(8.)).bg(white_a(0.35)))
                    .child(div().absolute().left(px(WHEEL / 2.0 - 4.0)).top(px(WHEEL / 2.0 - 0.5)).w(px(8.)).h(px(1.)).bg(white_a(0.35)))
                    .child(div().absolute().left(px(WHEEL / 2.0 + px_ - 6.0)).top(px(WHEEL / 2.0 + py_ - 6.0)).size(px(12.)).rounded_full().border_2().border_color(white_a(0.95)).bg(black_overlay(0.3)))
                    .on_mouse_down(gpui::MouseButton::Left, cx.listener(move |this, ev: &gpui::MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if ev.click_count >= 2 {
                            // Double-click: back to neutral.
                            this.store.commit();
                            this.set_props_now(&rid, &[(&format!("fx.{ruid}.{which}_amt"), 0.0)]);
                            cx.notify();
                            return;
                        }
                        this.store.begin_edit();
                        this.begin_drag(Drag::Wheel { key: key2.clone(), clip: cid.clone(), uid: uid2.clone(), which }, cx);
                        this.wheel_drag(&key2, &cid, &uid2, which, ev.position);
                    }))
                    .tooltip(|w, cx| gpui_component::tooltip::Tooltip::new("Drag towards a colour to tint. Double-click to reset.").build(w, cx)),
            )
            .child(div().w_full().child(lum))
    }

    /// Set a wheel from a pointer position.
    pub(super) fn wheel_drag(&mut self, key: &str, clip: &str, uid: &str, which: &str, pos: gpui::Point<Pixels>) {
        let Some(b) = self.bounds(key) else { return };
        let (cx, cy) = (f32::from(b.origin.x) + f32::from(b.size.width) / 2.0, f32::from(b.origin.y) + f32::from(b.size.height) / 2.0);
        let (dx, dy) = (f32::from(pos.x) - cx, f32::from(pos.y) - cy);
        let r = f32::from(b.size.width) / 2.0;
        let amt = ((dx * dx + dy * dy).sqrt() / r).min(1.0) as f64;
        let hue = ((-dy).atan2(dx).to_degrees() as f64).rem_euclid(360.0).round();
        let (h, a) = (format!("fx.{uid}.{which}_hue"), format!("fx.{uid}.{which}_amt"));
        self.set_props_now(clip, &[(h.as_str(), hue), (a.as_str(), (amt * 100.0).round() / 100.0)]);
    }

    /// The colour wheels card body: shadows, midtones, highlights, then saturation.
    pub(super) fn grade_controls(&self, c: &Clip, fx: &ClipEffect, cx: &mut Context<Self>) -> Vec<AnyElement> {
        vec![
            div()
                .flex()
                .gap_2()
                .child(self.wheel(c, &fx.uid, "lift", "Shadows", cx))
                .child(self.wheel(c, &fx.uid, "gamma", "Midtones", cx))
                .child(self.wheel(c, &fx.uid, "gain", "Highlights", cx))
                .into_any_element(),
            self.keyed_slider(c, &format!("fx.{}.saturation", fx.uid), "Saturation", -100.0, 100.0, 1.0, |v| format!("{v:+.0}"), None, cx).into_any_element(),
        ]
    }

    /// A file parameter (a LUT): the file's name and a button to pick another.
    pub(super) fn file_param(&self, c: &Clip, fx: &ClipEffect, key: &'static str, label_text: &str, ext: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let cur = match fx.params.get(key) {
            Some(ParamValue::Str(s)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };
        let status = cur.as_ref().map(|p| match crate::fx::grade::load_lut(p) {
            Ok(l) => format!("{}{}", crate::media::import::file_name(std::path::Path::new(p)), if l.title.is_empty() { String::new() } else { format!(" · {}", l.title) }),
            Err(e) => e,
        });
        let (cid, uid) = (c.id.clone(), fx.uid.clone());
        field_row(
            label_text,
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    button(SharedString::from(format!("file-{}", fx.uid)), Variant::Secondary, 30.)
                        .text_size(px(12.))
                        .child(if cur.is_some() { format!("Change .{ext}…") } else { format!("Choose .{ext} file…") })
                        .on_click(cx.listener(move |this, _, window, cx| this.pick_lut(cid.clone(), Some(uid.clone()), window, cx))),
                )
                .when_some(status, |d, s| d.child(micro(s).truncate())),
            None,
        )
        .into_any_element()
    }

    /// Choose a .cube file for a clip: into its LUT effect, or a new one.
    pub fn pick_lut(&mut self, clip: String, uid: Option<String>, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Use LUT".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(p) = paths.into_iter().next() {
                    let _ = this.update(cx, |this, cx| {
                        this.apply_lut(&clip, uid.as_deref(), &p);
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }
    pub fn apply_lut(&mut self, clip: &str, uid: Option<&str>, path: &std::path::Path) {
        let ps = path.to_string_lossy().to_string();
        if let Err(e) = crate::fx::grade::load_lut(&ps) {
            self.store.toast(e);
            return;
        }
        let uid = match uid {
            Some(u) => u.to_string(),
            None => {
                self.store.apply_effect("lut", Some(clip));
                match self.store.clip(clip).and_then(|c| c.effects.iter().rev().find(|e| e.id == "lut").map(|e| e.uid.clone())) {
                    Some(u) => u,
                    None => return,
                }
            }
        };
        self.store.commit();
        self.store.update_effect(clip, &uid, |e| {
            e.params.insert("file".into(), ParamValue::Str(ps.clone()));
        });
        self.store.toast(format!("LUT {} applied", crate::media::import::file_name(path)));
    }

    /// Blend mode and mask for a picture clip.
    pub(super) fn blend_mask_group(&self, c: &Clip, cx: &mut Context<Self>) -> Div {
        let cur = c.blend.clone().unwrap_or_else(|| "normal".into());
        let label_now = BLEND_MODES.iter().find(|(k, _)| *k == cur).map(|(_, l)| *l).unwrap_or("Normal");
        let entries: Vec<MenuEntry> = BLEND_MODES
            .iter()
            .map(|(k, l)| {
                let id = c.id.clone();
                let k = *k;
                item(*l, k == cur, move |e: &mut Editor| {
                    e.store.commit();
                    e.store.update_clip(&id, |c| c.blend = (k != "normal").then(|| k.to_string()));
                })
            })
            .collect();
        let ed = cx.entity();
        let id = c.id.clone();
        let shape = c.mask.as_ref().map(|m| m.shape.clone()).unwrap_or_default();
        let seg = segmented(&format!("mask-{}", c.id), &[(String::new(), "None"), ("ellipse".to_string(), "Ellipse"), ("rect".to_string(), "Rectangle")], &shape, |_| false, move |v, _, cx| {
            let id = id.clone();
            ed.update(cx, |e, cx| {
                e.store.commit();
                e.store.update_clip(&id, |c| {
                    if v.is_empty() {
                        c.mask = None;
                        c.keys.retain(|k, _| !k.starts_with("mask."));
                    } else {
                        let mut m = c.mask.clone().unwrap_or_default();
                        m.shape = v.clone();
                        c.mask = Some(m);
                    }
                });
                cx.notify();
            })
        });
        let mut g = group(Some("Blend & mask"), c.mask.as_ref().map(|_| "Masked".to_string()), false)
            .child(field_row("Blend", self.select("f-blend", label_now, entries, cx), None))
            .child(field_row("Mask", seg, None));
        if let Some(m) = &c.mask {
            let (w, h) = (crate::model::time::W() as f64, crate::model::time::H() as f64);
            let ed = cx.entity();
            let id = c.id.clone();
            let inv = segmented(&format!("maskinv-{}", c.id), &[(false, "Inside"), (true, "Outside")], &m.invert, |_| false, move |v, _, cx| {
                let id = id.clone();
                ed.update(cx, |e, cx| {
                    e.store.commit();
                    e.store.update_clip(&id, |c| {
                        if let Some(m) = c.mask.as_mut() {
                            m.invert = v;
                        }
                    });
                    cx.notify();
                })
            });
            g = g
                .child(field_row("Show", inv, None))
                .child(self.keyed_slider(c, "mask.x", "Centre X", -w / 2.0, w / 2.0, 1.0, |v| format!("{}", v.round()), None, cx))
                .child(self.keyed_slider(c, "mask.y", "Centre Y", -h / 2.0, h / 2.0, 1.0, |v| format!("{}", v.round()), None, cx))
                .child(self.keyed_slider(c, "mask.w", "Width", 8.0, w * 1.5, 1.0, |v| format!("{}", v.round()), None, cx))
                .child(self.keyed_slider(c, "mask.h", "Height", 8.0, h * 1.5, 1.0, |v| format!("{}", v.round()), None, cx))
                .child(self.keyed_slider(c, "mask.rot", "Rotation", -180.0, 180.0, 1.0, |v| format!("{}°", v.round()), None, cx))
                .child(self.keyed_slider(c, "mask.feather", "Feather", 0.0, 200.0, 1.0, |v| format!("{}", v.round()), None, cx))
                .when(m.shape == "rect", |d| d.child(self.keyed_slider(c, "mask.radius", "Corners", 0.0, 300.0, 1.0, |v| format!("{}", v.round()), None, cx)))
                .child(micro("Keyframe the mask to follow something as it moves. The mask moves with the clip's transform."));
        }
        g
    }

    /// Loudness, ducking and beats for a clip with sound.
    pub(super) fn sound_tools(&self, c: &Clip, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = vec![];
        let id = c.id.clone();
        let duck: Apply = Rc::new(move |e: &mut Editor, v| e.store.update_clip(&id, |c| c.duck = v));
        out.push(self.slider_field("Duck", format!("duck-{}", c.id), c.duck, 0.0, 1.0, 0.01, |v| if v <= 0.0 { "Off".into() } else { format!("{}%", (v * 100.0).round()) }, duck, cx).into_any_element());
        if c.duck > 0.0 {
            out.push(micro("Dips while voice plays on other tracks (imported audio, recordings, video sound).").into_any_element());
        }
        let mut row = div().flex().flex_wrap().gap(px(6.));
        if matches!(c.kind, AssetType::Audio | AssetType::Video) && buffer(&c.asset).is_some() {
            let id = c.id.clone();
            row = row.child(button(SharedString::from(format!("norm-{}", c.id)), Variant::Secondary, 28.).text_size(px(12.)).child("Normalize loudness").on_click(cx.listener(move |this, _, _, cx| {
                this.normalize(&id);
                this.restart_audio();
                cx.notify();
            })));
        }
        if c.kind == AssetType::Video && asset_has_sound(c) {
            let id = c.id.clone();
            row = row.child(button(SharedString::from(format!("detach-{}", c.id)), Variant::Secondary, 28.).text_size(px(12.)).child("Detach audio").on_click(cx.listener(move |this, _, _, cx| {
                this.store.detach_audio(&id);
                this.restart_audio();
                cx.notify();
            })));
        }
        if let Some(bpm) = self.clip_bpm(c) {
            let id = c.id.clone();
            out.push(micro(format!("Beat: about {bpm} BPM. With snapping on, clips snap to its beats.")).into_any_element());
            row = row.child(button(SharedString::from(format!("beatm-{}", c.id)), Variant::Secondary, 28.).text_size(px(12.)).child("Markers on beats").on_click(cx.listener(move |this, _, _, cx| {
                this.beat_markers(&id);
                cx.notify();
            })));
        }
        out.push(row.into_any_element());
        out
    }

    /// The tempo of a clip's sound: built-in music's own, or what was detected in an import.
    pub fn clip_bpm(&self, c: &Clip) -> Option<f64> {
        match c.kind {
            AssetType::Music => crate::model::assets::asset(&c.asset).map(|a| a.bpm).filter(|b| *b > 0.0),
            AssetType::Audio => beats(&c.asset).map(|b| b.bpm * c.speed()),
            _ => None,
        }
    }
    /// Where a clip's beats fall on the timeline, inside the clip.
    pub fn beat_times(&self, c: &Clip) -> Vec<f64> {
        let (first, period, speed) = match c.kind {
            AssetType::Music => match crate::model::assets::asset(&c.asset).filter(|a| a.bpm > 0.0) {
                Some(a) => (0.0, 60.0 / a.bpm, 1.0),
                None => return vec![],
            },
            AssetType::Audio => match beats(&c.asset) {
                Some(b) => (b.first, b.period(), c.speed()),
                None => return vec![],
            },
            _ => return vec![],
        };
        let k0 = ((c.in_ - first) / period).ceil().max(0.0) as i64;
        let mut v = vec![];
        let mut k = k0;
        loop {
            let src = first + k as f64 * period;
            let t = c.start + (src - c.in_) / speed;
            if t > c.start + c.dur + 1e-6 || v.len() > 4000 {
                break;
            }
            v.push(t);
            k += 1;
        }
        v
    }
    fn beat_markers(&mut self, id: &str) {
        let Some(c) = self.store.clip(id).cloned() else { return };
        let list: Vec<Marker> = self.beat_times(&c).into_iter().enumerate().map(|(i, t)| Marker { t: crate::model::time::snap_frame(t), name: format!("Beat {}", i + 1), color: "#0099ff".into() }).collect();
        let n = list.len();
        self.store.add_markers(list);
        self.store.toast(format!("{n} beat markers added. Clips snap to markers."));
    }

    /// Set a clip's volume so its sound averages about −16 dB (speech and music loudness for
    /// online video), never letting peaks go over −1 dB.
    pub fn normalize(&mut self, id: &str) {
        let Some(c) = self.store.clip(id).cloned() else { return };
        let Some(v) = crate::audio::mix::normalize_volume(&c) else {
            self.store.toast(if buffer(&c.asset).is_none() { "This clip's sound isn't loaded yet" } else { "This clip is silent" });
            return;
        };
        self.store.commit();
        self.store.update_clip(id, |c| {
            c.volume = v;
            c.keys.remove("volume");
        });
        self.store.toast(format!("Normalized: volume {}%{}", (v * 100.0).round(), if v >= MAX_VOLUME { " (as loud as it goes; the recording is very quiet)" } else { "" }));
    }
}

fn asset_has_sound(c: &Clip) -> bool {
    crate::model::assets::asset(&c.asset).and_then(|a| a.media.as_ref().map(|m| m.has_audio)).unwrap_or(false)
}

/// For the effect card: whether it gets the colour wheels instead of plain sliders.
pub fn is_grade(fx: &ClipEffect) -> bool {
    effect_def(&fx.id).map(|d| d.engine == crate::fx::registry::Engine::Grade).unwrap_or(false)
}

