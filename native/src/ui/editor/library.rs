// The library: media pool, sample footage, music, stills, overlays, text styles, fonts, effects and
// transitions. Cards drag onto the timeline, the preview or a clip.

use super::{DragPayload, DropTarget, Editor, PayloadKind, Tab};
use crate::fx::registry::{effects, EFFECT_GROUPS};
use crate::fx::transitions::{TRANSITIONS, TRANSITION_GROUPS};
use crate::model::assets::{asset, font_def, images as stills, music, overlays, scenes, shaders, text_presets, Asset, FADES, FONTS};
use crate::model::time::short_time;
use crate::model::types::AssetType;
use crate::ui::images::{cached, put, to_image};
use crate::ui::prefs;
use crate::ui::theme::*;
use futures::channel::mpsc::UnboundedSender;
use gpui::prelude::*;
use gpui::{div, img, px, AnyElement, App, Context, FontWeight, ObjectFit, Pixels, Point, RenderImage, SharedString, WeakEntity, Window};
use gpui_component::input::Input;
use std::sync::Arc;

const TABS: [(Tab, &str, &str); 7] = [
    (Tab::Media, "Media", "film"),
    (Tab::Music, "Music", "music-2"),
    (Tab::Images, "Images", "image"),
    (Tab::Overlays, "Overlays", "layers"),
    (Tab::Text, "Text", "type"),
    (Tab::Effects, "Effects", "sparkles"),
    (Tab::Transitions, "Transitions", "blend"),
];

pub fn saved_tab() -> Tab {
    let s = prefs::get_str("tab").unwrap_or_default();
    TABS.iter().find(|(_, l, _)| l.to_lowercase() == s).map(|(t, _, _)| *t).unwrap_or(Tab::Media)
}

/// Render every built-in thumbnail off the UI thread, then wake the editor.
pub fn warm_thumbs(wake: UnboundedSender<()>) {
    if cached("fx:glow").is_some() {
        return;
    }
    std::thread::spawn(move || {
        for a in scenes().iter().chain(stills().iter()).chain(overlays().iter()).chain(text_presets().iter()).chain(shaders().iter()) {
            let key = format!("asset:{}", a.id);
            if cached(&key).is_none() {
                if let Some(p) = crate::render::thumbs::asset_thumb(a) {
                    put(&key, to_image(&p));
                }
            }
        }
        let _ = wake.unbounded_send(());
        for t in TRANSITIONS.iter() {
            put(&format!("tr:{}", t.id), to_image(&crate::render::thumbs::transition_thumb(t.id)));
        }
        let _ = wake.unbounded_send(());
        for (i, e) in effects().iter().enumerate() {
            put(&format!("fx:{}", e.id), to_image(&crate::render::thumbs::effect_thumb(e.id)));
            if i % 6 == 5 {
                let _ = wake.unbounded_send(());
            }
        }
        let _ = wake.unbounded_send(());
    });
}

/// A library card's picture (built-in or imported).
pub fn asset_image(a: &Asset) -> Option<Arc<RenderImage>> {
    let key = format!("asset:{}", a.id);
    if let Some(i) = cached(&key) {
        return Some(i);
    }
    let pm = a.thumb.as_ref()?;
    let i = to_image(pm);
    put(&key, i.clone());
    Some(i)
}

/// What follows the pointer while something is dragged out of the library, and says where it
/// will land.
pub struct DragGhost {
    payload: DragPayload,
    offset: Point<Pixels>,
    editor: WeakEntity<Editor>,
}
impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (mut hint, mut bad, mut on) = (
            if self.payload.kind == PayloadKind::Asset { "Drop on a track or the preview".to_string() } else { "Drop on a clip".to_string() },
            false,
            false,
        );
        if let Some(ed) = self.editor.upgrade() {
            let ed = ed.read(cx);
            match &ed.drop_target {
                Some(DropTarget::Lane { track, time, ok }) => {
                    let t = ed.store.track(track);
                    on = true;
                    if t.locked {
                        hint = format!("{} is locked", t.name);
                        bad = true;
                    } else if !ok {
                        hint = format!("{} doesn't take this. Drops on its own track.", t.name);
                        bad = true;
                    } else {
                        hint = format!("{} at {}", t.name, crate::model::time::tc(*time));
                    }
                }
                Some(DropTarget::Stage) => {
                    hint = "Add at the playhead".into();
                    on = true;
                }
                Some(DropTarget::Clip(_)) => {
                    hint = if self.payload.kind == PayloadKind::Effect { "Apply to this clip".into() } else { "Add to this clip".into() };
                    on = true;
                }
                _ => {}
            }
        }
        div().pl(self.offset.x + px(14.)).pt(self.offset.y + px(14.)).child(
            div()
                .w(px(208.))
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded(px(10.))
                .bg(surface2())
                .border_1()
                .border_color(white_a(0.1))
                .shadow_lg()
                .when_some(self.payload.thumb.clone(), |d, t| d.child(img(t).w_full().aspect_ratio(16. / 9.).rounded_t(px(9.)).object_fit(ObjectFit::Cover)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .px(px(10.))
                        .py_2()
                        .child(type_micro(div()).font_weight(FontWeight::MEDIUM).truncate().child(self.payload.label.clone()))
                        .child(type_micro(div()).text_size(px(11.)).truncate().text_color(if bad { coral() } else if on { signal() } else { ink_muted() }).child(hint)),
                ),
        )
    }
}

impl Editor {
    fn drag_source<E: InteractiveElement + StatefulInteractiveElement + 'static>(&self, el: E, payload: DragPayload, cx: &mut Context<Self>) -> E {
        let editor = cx.entity().downgrade();
        el.on_drag(payload, move |p: &DragPayload, offset, _w, cx: &mut App| {
            let editor = editor.clone();
            let p = p.clone();
            cx.new(|cx| {
                if let Some(e) = editor.upgrade() {
                    cx.observe(&e, |_, _, cx| cx.notify()).detach();
                }
                DragGhost { payload: p, offset, editor }
            })
        })
    }

    pub(super) fn render_library(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = div()
            .id("lib-tabs")
            .w(px(72.))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(6.))
            .py_2()
            .border_r_1()
            .border_color(hairline_soft())
            .overflow_y_scroll()
            .children(TABS.iter().map(|(t, l, ic)| {
                let active = self.tab == *t;
                let t = *t;
                div()
                    .id(SharedString::from(format!("tab-{l}")))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(2.))
                    .py(px(6.))
                    .rounded(px(10.))
                    .cursor_pointer()
                    .text_size(px(10.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if active { ink() } else { ink_muted() })
                    .bg(if active { surface3() } else { alpha(ink(), 0.0) })
                    .hover(|s| s.bg(surface2()).text_color(ink()))
                    .child(icon(ic, 16., if active { ink() } else { ink_muted() }))
                    .child(l.to_string())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.tab = t;
                        prefs::set("tab", serde_json::Value::String(format!("{t:?}").to_lowercase()));
                        cx.notify();
                    }))
            }));

        let q = self.query.trim().to_lowercase();
        let matches = |name: &str, sub: &str| q.is_empty() || format!("{name} {sub}").to_lowercase().contains(&q);
        let mine: Vec<Arc<Asset>> = self.store.pool.iter().filter_map(|id| asset(id)).filter(|a| matches(&a.name, &a.sub)).collect();

        let body: Vec<AnyElement> = match self.tab {
            Tab::Media => {
                let vids: Vec<&Arc<Asset>> = mine.iter().filter(|a| a.kind != AssetType::Audio).collect();
                vec![
                    section(
                        "Media pool",
                        None,
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(self.drop_zone("pool", "video, audio or images", cx))
                            .child(if vids.is_empty() {
                                micro("Imported files are used from where they are on your Mac. Nothing is uploaded.").into_any_element()
                            } else {
                                grid(vids.iter().map(|a| self.asset_card(a, a.kind == AssetType::Video, cx).into_any_element())).into_any_element()
                            }),
                    ),
                    section("Sample footage", None, grid(scenes().iter().filter(|a| matches(&a.name, &a.sub)).map(|a| self.asset_card(a, true, cx).into_any_element()))),
                    section("Shader backgrounds", None, grid(shaders().iter().filter(|a| matches(&a.name, &a.sub)).map(|a| self.asset_card(a, true, cx).into_any_element()))),
                ]
            }
            Tab::Music => {
                let imported: Vec<&Arc<Asset>> = mine.iter().filter(|a| a.kind == AssetType::Audio).collect();
                vec![
                    section("Music library", None, rows(music().iter().filter(|a| matches(&a.name, &a.sub)).map(|a| self.music_row(a, false, cx)))),
                    section(
                        "Imported audio",
                        None,
                        if imported.is_empty() { self.drop_zone("audio", "audio files", cx).into_any_element() } else { rows(imported.iter().map(|a| self.music_row(a, true, cx))).into_any_element() },
                    ),
                ]
            }
            Tab::Images => {
                let photos: Vec<&Arc<Asset>> = mine.iter().filter(|a| a.kind == AssetType::Photo).collect();
                let mut v = vec![];
                if !photos.is_empty() {
                    v.push(section("Your images", None, grid(photos.iter().map(|a| self.asset_card(a, false, cx).into_any_element()))));
                }
                v.push(section("Stills & backgrounds", None, grid(stills().iter().filter(|a| matches(&a.name, &a.sub)).map(|a| self.asset_card(a, false, cx).into_any_element()))));
                if photos.is_empty() {
                    v.push(section("Import images", None, self.drop_zone("images", "PNG, JPG or WebP files", cx)));
                }
                v
            }
            Tab::Overlays => vec![
                section("Overlays", None, grid(overlays().iter().filter(|a| matches(&a.name, &a.sub)).map(|a| self.asset_card(a, false, cx).into_any_element()))),
                micro("Overlays go on the Overlays track above your footage and blend with it. Lower the opacity in the inspector to soften them.").px_3().pt_3().into_any_element(),
            ],
            Tab::Text => vec![
                section("Text styles", None, grid(text_presets().iter().filter(|a| matches(&a.name, &a.sub)).map(|a| self.asset_card(a, true, cx).into_any_element()))),
                section("Fonts", Some("Click to apply"), rows(FONTS.iter().filter(|f| matches(f.family, f.role)).map(|f| self.font_card(f.family, cx)))),
                {
                    // Every font on this Mac, previewed in itself. Without a search only the first
                    // few show, so the panel stays quick.
                    let all = crate::render::text::system_families();
                    let hits: Vec<&'static String> = all.iter().filter(|f| matches(f, "installed")).collect();
                    let cap = if q.is_empty() { 24 } else { 120 };
                    let more = hits.len().saturating_sub(cap);
                    let list = rows(hits.iter().take(cap).map(|f| self.font_card(f.as_str(), cx)));
                    section(
                        "Installed fonts",
                        Some(if all.is_empty() { "None found" } else { "On this Mac" }),
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(list)
                            .when(more > 0, |d| d.child(micro(format!("{more} more. Search to find a font by name.")).px_1())),
                    )
                },
            ],
            Tab::Effects => {
                let mut v = vec![micro("Click an effect to add it to the selected clip, or drag it onto any clip. Effects stack, and every setting can be changed in the inspector.")
                    .px_3()
                    .pt_3()
                    .into_any_element()];
                for (g, title) in EFFECT_GROUPS {
                    let list: Vec<_> = effects().iter().filter(|e| e.group == g && matches(e.name, "")).collect();
                    if !list.is_empty() {
                        v.push(section(title, None, grid(list.iter().map(|e| self.fx_card(PayloadKind::Effect, e.id, e.name, cx)))));
                    }
                }
                v
            }
            Tab::Transitions => {
                let mut v = vec![micro("A transition goes on the start of a clip. Between two touching clips it blends across the cut. On a clip with nothing before it, the clip transitions in. Click to add to the selected clip, or drag onto one.")
                    .px_3()
                    .pt_3()
                    .into_any_element()];
                for (g, title) in TRANSITION_GROUPS {
                    let list: Vec<_> = TRANSITIONS.iter().filter(|t| t.group == g && matches(t.name, "")).collect();
                    if !list.is_empty() {
                        v.push(section(title, None, grid(list.iter().map(|t| self.fx_card(PayloadKind::Transition, t.id, t.name, cx)))));
                    }
                }
                v.push(section("Fades", None, rows(FADES.iter().filter(|(_, n, s)| matches(n, s)).map(|(id, n, s)| self.fade_row(id, n, s, cx)))));
                v
            }
        };

        panel()
            .w(px(self.layout.lib_w))
            .flex_none()
            .flex_row()
            .child(tabs)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(
                        panel_header()
                            .px_3()
                            .child(div().relative().flex_1().child(self.track_bounds("input:search")).child(Input::new(&self.search_input).prefix(icon("search", 16., ink_muted())).aria_label("Search library")))
                            .child(icon_button("lib-close", "x", "Close library", Some("⌘B"), 28., Variant::Ghost, cx.listener(|this, _, _, cx| this.toggle_panel(super::layout::Edge::Library, cx)))),
                    )
                    .child(div().id("lib-scroll").flex_1().min_h_0().overflow_y_scroll().pb_3().children(body)),
            )
    }

    fn asset_card(&self, a: &Asset, show_dur: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let thumb = asset_image(a);
        let id = a.id.clone();
        let id2 = a.id.clone();
        let payload = DragPayload { kind: PayloadKind::Asset, id: a.id.clone(), label: a.name.clone(), thumb: thumb.clone() };
        let group_name = SharedString::from(format!("card-{}", a.id));
        let card = div()
            .id(SharedString::from(format!("asset-{}", a.id)))
            .group(group_name.clone())
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(10.))
            .bg(surface2())
            .cursor_grab()
            .border_1()
            .border_color(alpha(ink(), 0.0))
            .hover(|s| s.border_color(hairline()))
            .tooltip(|w, cx| gpui_component::tooltip::Tooltip::new("Drag to the timeline, or press + to add at the playhead").build(w, cx))
            .child(
                div()
                    .relative()
                    .w_full()
                    .aspect_ratio(16. / 9.)
                    .rounded_t(px(9.))
                    .bg(black())
                    .when_some(thumb, |d, t| d.child(img(t).absolute().size_full().rounded_t(px(9.)).object_fit(ObjectFit::Cover)))
                    .when(show_dur, |d| {
                        d.child(type_timecode(div(), 10.).absolute().right(px(6.)).bottom(px(6.)).rounded(px(4.)).bg(black_overlay(0.65)).px(px(6.)).py(px(2.)).child(short_time(a.dur)))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .px_2()
                    .pt(px(6.))
                    .pb_2()
                    .child(type_micro(div()).font_weight(FontWeight::MEDIUM).truncate().child(a.name.clone()))
                    .child(type_micro(div()).text_size(px(11.)).text_color(ink_muted()).truncate().child(a.sub.clone())),
            )
            .child(
                div()
                    .id(SharedString::from(format!("add-{id}")))
                    .absolute()
                    .top(px(6.))
                    .right(px(6.))
                    .size(px(24.))
                    .rounded_full()
                    .bg(ink())
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(0.)
                    .group_hover(group_name, |s| s.opacity(1.))
                    .cursor_pointer()
                    .child(icon("plus", 14., black()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.add_at_playhead(&id2);
                        cx.notify();
                    })),
            );
        let _ = id;
        self.drag_source(card, payload, cx)
    }

    fn music_row(&self, a: &Asset, imported: bool, cx: &mut Context<Self>) -> AnyElement {
        let playing = self.audio.previewing().as_deref() == Some(a.id.as_str());
        let hue = hex(a.hue);
        let id = a.id.clone();
        let id2 = a.id.clone();
        let payload = DragPayload { kind: PayloadKind::Asset, id: a.id.clone(), label: a.name.clone(), thumb: None };
        let row = div()
            .id(SharedString::from(format!("music-{}", a.id)))
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(10.))
            .bg(surface2())
            .p_2()
            .cursor_grab()
            .hover(|s| s.bg(surface3()))
            .child(
                div()
                    .size(px(40.))
                    .flex_none()
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(if imported { surface3() } else { mix_color(hue, surface3(), 0.18) })
                    .child(icon(if imported { "audio-lines" } else { "music-2" }, 16., if imported { ink() } else { hue })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .child(type_caption(div()).truncate().child(a.name.clone()))
                    .child(type_micro(div()).text_size(px(11.)).text_color(ink_muted()).truncate().child(a.sub.clone()))
                    .child(type_timecode(div(), 10.).text_color(ink_muted()).child(if imported { short_time(a.dur) } else { format!("{} BPM · {}", a.bpm, short_time(a.dur)) })),
            )
            .when(!imported, |d| {
                d.child(icon_button(SharedString::from(format!("pv-{id}")), if playing { "pause" } else { "play" }, if playing { "Stop preview" } else { "Preview" }, None, 28., Variant::Ghost, cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    if this.audio.previewing().as_deref() == Some(id.as_str()) {
                        this.audio.preview(None);
                    } else if let Some(a) = asset(&id) {
                        if !this.audio.available() {
                            this.store.toast("Audio playback isn't available: no output device");
                        }
                        this.audio.preview(Some(a));
                        // Re-render when the 16-beat preview ends, so the button resets.
                        let beat = 60.0 / this.audio_bpm(&id);
                        cx.spawn(async move |this, cx| {
                            cx.background_executor().timer(std::time::Duration::from_secs_f64(beat * 16.0 + 0.6)).await;
                            let _ = this.update(cx, |_, cx| cx.notify());
                        })
                        .detach();
                    }
                    cx.notify();
                })))
            })
            .child(icon_button(SharedString::from(format!("addm-{id2}")), "plus", "Add at playhead", None, 28., Variant::Ghost, cx.listener({
                let id = a.id.clone();
                move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.add_at_playhead(&id);
                    cx.notify();
                }
            })));
        self.drag_source(row, payload, cx).into_any_element()
    }
    fn audio_bpm(&self, id: &str) -> f64 {
        asset(id).map(|a| a.bpm).filter(|b| *b > 0.0).unwrap_or(90.0)
    }

    fn font_card(&self, family: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let f = font_def(family);
        div()
            .id(SharedString::from(format!("font-{family}")))
            .flex()
            .flex_col()
            .gap_1()
            .rounded(px(10.))
            .bg(surface2())
            .p_3()
            .cursor_pointer()
            .hover(|s| s.bg(surface3()))
            .child(div().font_family(f.family).text_size(px(26.)).line_height(px(28.)).font_weight(FontWeight(f.weight as f32)).truncate().child("Golden light"))
            .child(type_micro(div()).text_size(px(11.)).text_color(ink_muted()).child(format!("{} · {}", f.family, f.role)))
            .on_click(cx.listener(move |this, _, _, cx| {
                let f = font_def(family);
                let sel = this.store.selected_clip().filter(|c| c.text.is_some()).map(|c| c.id.clone());
                if let Some(id) = sel {
                    this.store.commit();
                    this.store.update_text(&id, |t| {
                        t.font = f.family.into();
                        t.weight = f.weight;
                    });
                    this.store.toast(format!("Font set to {}", f.family));
                } else if let Some(n) = this.store.add_clip("tx-title", Some("T1"), None) {
                    this.store.update_text(&n.id, |t| {
                        t.font = f.family.into();
                        t.weight = f.weight;
                        t.content = "Golden light".into();
                    });
                    this.store.toast(format!("Added a title in {}", f.family));
                }
                cx.notify();
            }))
            .into_any_element()
    }

    fn fx_card(&self, kind: PayloadKind, id: &'static str, name: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let key = if kind == PayloadKind::Effect { format!("fx:{id}") } else { format!("tr:{id}") };
        let thumb = cached(&key);
        let payload = DragPayload { kind, id: id.into(), label: name.into(), thumb: thumb.clone() };
        let card = div()
            .id(SharedString::from(format!("{key}-card")))
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(10.))
            .bg(surface2())
            .cursor_pointer()
            .border_1()
            .border_color(alpha(ink(), 0.0))
            .hover(|s| s.border_color(hairline()))
            .child(div().relative().w_full().aspect_ratio(16. / 10.).rounded_t(px(9.)).bg(black()).when_some(thumb, |d, t| d.child(img(t).absolute().size_full().rounded_t(px(9.)).object_fit(ObjectFit::Cover))))
            .child(type_micro(div()).font_weight(FontWeight::MEDIUM).truncate().px_2().pt(px(6.)).pb_2().child(name))
            .on_click(cx.listener(move |this, _, _, cx| {
                if kind == PayloadKind::Effect {
                    this.store.apply_effect(id, None);
                } else {
                    this.store.apply_transition(id, None);
                }
                cx.notify();
            }));
        self.drag_source(card, payload, cx).into_any_element()
    }

    fn fade_row(&self, id: &'static str, name: &'static str, sub: &'static str, cx: &mut Context<Self>) -> AnyElement {
        let payload = DragPayload { kind: PayloadKind::Transition, id: id.into(), label: name.into(), thumb: None };
        let row = div()
            .id(SharedString::from(format!("fade-{id}")))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(10.))
            .bg(surface2())
            .p_2()
            .cursor_pointer()
            .hover(|s| s.bg(surface3()))
            .child(div().size(px(40.)).flex_none().rounded(px(6.)).bg(surface3()).flex().items_center().justify_center().child(icon("blend", 16., ink_muted())))
            .child(div().flex().flex_col().flex_1().min_w_0().child(type_caption(div()).child(name)).child(type_micro(div()).text_size(px(11.)).text_color(ink_muted()).child(sub)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.store.apply_transition(id, None);
                cx.notify();
            }));
        self.drag_source(row, payload, cx).into_any_element()
    }

    fn drop_zone(&self, key: &str, what: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let over = matches!(self.drop_target, Some(DropTarget::Pool)) && self.lib_drag.is_none();
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .rounded(px(10.))
            .border_1()
            .border_dashed()
            .border_color(if over { signal() } else { hairline() })
            .when(over, |d| d.bg(alpha(signal(), 0.1)))
            .p_4()
            .text_color(if over { ink() } else { ink_muted() })
            .child(icon("upload", 16., ink_muted()))
            .child(type_micro(div()).child(format!("Drop {what} here")))
            .child(
                button(SharedString::from(format!("choose-{key}")), Variant::Primary, 32.)
                    .child("Choose files")
                    .on_click(cx.listener(|this, _, window, cx| this.pick_files(None, window, cx))),
            )
    }
}

fn section(title: &str, action: Option<&str>, body: impl IntoElement) -> AnyElement {
    div().flex().flex_col().gap_2().px_3().pt_3().child(section_label(title, action)).child(body).into_any_element()
}
fn grid(items: impl Iterator<Item = AnyElement>) -> gpui::Div {
    div().grid().grid_cols(2).gap_2().children(items)
}
fn rows(items: impl Iterator<Item = AnyElement>) -> gpui::Div {
    div().flex().flex_col().gap_1().children(items)
}
/// `a` mixed into `b` by `t` (like color-mix in oklch, close enough for a tint).
fn mix_color(a: gpui::Hsla, b: gpui::Hsla, t: f32) -> gpui::Hsla {
    let (ar, br) = (a.to_rgb(), b.to_rgb());
    gpui::Rgba { r: br.r + (ar.r - br.r) * t, g: br.g + (ar.g - br.g) * t, b: br.b + (ar.b - br.b) * t, a: 1.0 }.into()
}
