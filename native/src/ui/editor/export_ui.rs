// The export dialog: what to export, its settings, then progress and the result. The export runs
// on its own thread; the file goes to Downloads, like a browser download.

use super::inspector::{item, MenuEntry};
use super::{Drag, Editor};
use crate::export::{self, gif_size, output_size, Container, Framing, Job, Kind, Outcome, Quality, RangeMode, Settings, Support, ASPECTS, RESOLUTIONS};
use crate::model::time::{tc, FPS};
use crate::ui::prefs;
use crate::ui::theme::*;
use gpui::prelude::*;
use gpui::{div, px, relative, Context, FontWeight, MouseButton, MouseDownEvent, Window};
use gpui_component::input::Input;
use gpui_component::Sizable;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub enum Phase {
    Setup,
    Running,
    Done(Outcome),
    Error(String),
}

pub struct ExportUi {
    pub s: Settings,
    pub name: String,
    pub custom: [f64; 2],
    pub phase: Phase,
    support: Support,
    progress: Arc<Mutex<(f64, String)>>,
    cancel: Arc<AtomicBool>,
}
impl ExportUi {
    pub fn running(&self) -> bool {
        matches!(self.phase, Phase::Running)
    }
}

fn safe_name(s: &str) -> String {
    let cleaned: String = s.trim().chars().map(|c| if "\\/:*?\"<>|".contains(c) { '-' } else { c }).take(80).collect();
    if cleaned.is_empty() { "Liemo export".into() } else { cleaned }
}
fn mb(bytes: u64) -> String {
    if bytes < 1048576 { format!("{} KB", (bytes / 1024).max(1)) } else { format!("{:.1} MB", bytes as f64 / 1048576.0) }
}
fn downloads() -> PathBuf {
    dirs::download_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join("Downloads"))
}

impl Editor {
    pub fn open_export(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pause();
        let mut s: Settings = prefs::get("export").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        // Export in the project's own shape unless changed in the dialog.
        s.aspect = self.project.aspect_key();
        s.resolution = self.project.resolution;
        s.fps = self.project.fps;
        // In and out points set on the ruler are what to export, unless changed here.
        if self.store.in_out_range().is_some() {
            s.range_mode = RangeMode::InOut;
        } else if s.range_mode == RangeMode::InOut {
            s.range_mode = RangeMode::All;
        }
        let name = safe_name(&self.store.name);
        self.export_name.update(cx, |st, cx| st.set_value(name.clone(), window, cx));
        let support = export::probe(s.container);
        self.export = Some(ExportUi {
            s,
            name,
            custom: [0.0, self.store.end()],
            phase: Phase::Setup,
            support,
            progress: Arc::new(Mutex::new((0.0, "Preparing".into()))),
            cancel: Arc::new(AtomicBool::new(false)),
        });
        cx.notify();
    }

    fn set_export(&mut self, f: impl FnOnce(&mut Settings)) {
        if let Some(e) = &mut self.export {
            f(&mut e.s);
            e.support = export::probe(e.s.container);
            if let Ok(v) = serde_json::to_value(&e.s) {
                prefs::set("export", v);
            }
        }
    }

    fn export_range(&self) -> (f64, f64) {
        let dur = self.store.end();
        let Some(e) = &self.export else { return (0.0, dur) };
        match e.s.range_mode {
            RangeMode::Clip => match self.store.selected_clip() {
                Some(c) => (c.start, dur.min(c.start + c.dur)),
                None => (0.0, dur),
            },
            RangeMode::Custom => (e.custom[0].min(e.custom[1]), e.custom[0].max(e.custom[1])),
            RangeMode::All => (0.0, dur),
            RangeMode::InOut => self.store.in_out_range().unwrap_or((0.0, dur)),
        }
    }

    fn start_export(&mut self, cx: &mut Context<Self>) {
        let (start, end) = self.export_range();
        let dur = self.store.end();
        let Some(e) = &mut self.export else { return };
        let ext = export::extension(&e.s);
        let path = export::unique_path(&downloads(), &safe_name(&e.name), ext);
        let job = Job {
            settings: e.s.clone(),
            start,
            end,
            image_time: self.store.time.min((dur - 1.0 / FPS()).max(0.0)),
            clips: self.store.clips.clone(),
            tracks: self.store.tracks.clone(),
            path,
        };
        e.cancel.store(false, Ordering::Relaxed);
        *e.progress.lock() = (0.0, "Preparing".into());
        e.phase = Phase::Running;
        let (progress, cancel) = (e.progress.clone(), e.cancel.clone());
        let result: Arc<Mutex<Option<Result<Outcome, String>>>> = Arc::new(Mutex::new(None));
        let out = result.clone();
        let env = self.env();
        std::thread::spawn(move || {
            crate::model::time::set_env(env);
            let report = |f: f64, l: String| *progress.lock() = (f, l);
            let p = export::Progress { cancel: &cancel, report: &report };
            let r = export::run(&job, &p).map_err(|e| if e.is::<export::CancelledError>() { String::new() } else { e.to_string() });
            *out.lock() = Some(r);
        });
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(100)).await;
            let done = result.lock().take();
            let alive = this.update(cx, |this, cx| {
                if let (Some(r), Some(e)) = (done.as_ref(), this.export.as_mut()) {
                    e.phase = match r {
                        Ok(_) => Phase::Setup,
                        Err(m) if m.is_empty() => Phase::Setup,
                        Err(m) => Phase::Error(m.clone()),
                    };
                }
                cx.notify();
            });
            if let Some(Ok(o)) = done {
                let _ = this.update(cx, |this, cx| {
                    if let Some(e) = this.export.as_mut() {
                        e.phase = Phase::Done(o);
                    }
                    cx.notify();
                });
                break;
            }
            if done.is_some() || alive.is_err() {
                break;
            }
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_export(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (start, end) = self.export_range();
        let length = (end - start).max(0.0);
        let duration = self.store.end();
        let time = self.store.time;
        let clip_count = self.store.clips.len();
        let selected = self.store.selected_clip().cloned();
        let project_name = if self.store.name.is_empty() { "Untitled project".to_string() } else { self.store.name.clone() };
        let e = self.export.as_ref().unwrap();
        let s = e.s.clone();
        let (w, h) = if s.kind == Kind::Gif { gif_size(s.gif_width, &s.aspect) } else { output_size(s.resolution, &s.aspect) };
        let running = e.running();
        let video_blocked = s.kind == Kind::Video && e.support.video.is_none();
        let can_start = clip_count > 0 && (s.kind == Kind::Image || length >= 0.05) && !video_blocked;
        let ed = cx.entity();

        let set = |f: fn(&mut Settings, u8), v: u8| {
            let ed = ed.clone();
            move |_: &mut Window, cx: &mut gpui::App| ed.update(cx, |this, cx| {
                this.set_export(|s| f(s, v));
                cx.notify();
            })
        };
        let _ = set;
        let seg = |id: &str, opts: Vec<(String, String)>, value: String, disabled: Vec<String>, apply: fn(&mut Settings, &str)| {
            let ed = ed.clone();
            let refs: Vec<(String, &str)> = opts.iter().map(|(v, l)| (v.clone(), l.as_str())).collect();
            segmented(id, &refs, &value, |v| disabled.contains(v), move |v: String, _, cx| {
                ed.update(cx, |this, cx| {
                    this.set_export(|s| apply(s, &v));
                    cx.notify();
                })
            })
        };
        let o = |pairs: &[(&str, &str)]| pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        let sel = |this: &Editor, id: &str, cur: String, opts: Vec<(String, String)>, apply: fn(&mut Settings, &str), cx: &mut Context<Editor>| {
            let label_now = opts.iter().find(|(v, _)| *v == cur).map(|(_, l)| l.clone()).unwrap_or_default();
            let entries: Vec<MenuEntry> = opts
                .into_iter()
                .map(|(v, l)| {
                    let checked = v == cur;
                    item(l, checked, move |e: &mut Editor| e.set_export(|s| apply(s, &v)))
                })
                .collect();
            this.select(id.to_string(), label_now, entries, cx).into_any_element()
        };

        let framing = |this: &Editor, cx: &mut Context<Editor>| {
            let mut v = vec![field_row(
                "Shape",
                sel(this, "ex-aspect", s.aspect.clone(), ASPECTS.iter().map(|(k, l, _, _)| (k.to_string(), l.to_string())).collect(), |s, v| s.aspect = v.into(), cx),
                None,
            )
            .into_any_element()];
            if s.aspect != "16:9" {
                v.push(
                    field_row(
                        "Framing",
                        seg("ex-framing", o(&[("crop", "Crop to fill"), ("blur", "Blurred fill"), ("bars", "Black bars")]), format!("{:?}", s.framing).to_lowercase(), vec![], |s, v| {
                            s.framing = match v {
                                "crop" => Framing::Crop,
                                "bars" => Framing::Bars,
                                _ => Framing::Blur,
                            }
                        }),
                        None,
                    )
                    .into_any_element(),
                );
            }
            v
        };
        let quality = |id: &str| {
            seg(id, o(&[("low", "Low"), ("medium", "Medium"), ("high", "High"), ("best", "Best")]), format!("{:?}", s.quality).to_lowercase(), vec![], |s, v| {
                s.quality = match v {
                    "low" => Quality::Low,
                    "medium" => Quality::Medium,
                    "best" => Quality::Best,
                    _ => Quality::High,
                }
            })
        };

        let mut fields: Vec<gpui::AnyElement> = vec![field_row("File name", div().relative().child(self.track_bounds("input:export")).child(Input::new(&self.export_name).small()), None).into_any_element()];
        match s.kind {
            Kind::Video => {
                fields.push(field_row("Format", seg("ex-fmt", o(&[("mp4", "MP4"), ("webm", "WebM")]), if s.container == Container::Mp4 { "mp4".into() } else { "webm".into() }, vec![], |s, v| s.container = if v == "mp4" { Container::Mp4 } else { Container::Webm }), None).into_any_element());
                fields.push(field_row("Resolution", sel(self, "ex-res", s.resolution.to_string(), RESOLUTIONS.iter().map(|(r, l)| (r.to_string(), l.to_string())).collect(), |s, v| s.resolution = v.parse().unwrap_or(1080), cx), None).into_any_element());
                fields.extend(framing(self, cx));
                fields.push(field_row("Frame rate", seg("ex-fps", o(&[("24", "24"), ("25", "25"), ("30", "30"), ("60", "60")]), s.fps.to_string(), vec![], |s, v| s.fps = v.parse().unwrap_or(30)), None).into_any_element());
                fields.push(field_row("Quality", quality("ex-q"), None).into_any_element());
                fields.push(field_row("Sound", seg("ex-sound", o(&[("on", "Include"), ("off", "Silent")]), if s.audio { "on".into() } else { "off".into() }, vec![], |s, v| s.audio = v == "on"), None).into_any_element());
            }
            Kind::Gif => {
                fields.push(field_row("Width", sel(self, "ex-gw", s.gif_width.to_string(), o(&[("320", "320 px · small"), ("480", "480 px"), ("640", "640 px"), ("800", "800 px · large")]), |s, v| s.gif_width = v.parse().unwrap_or(480), cx), None).into_any_element());
                fields.extend(framing(self, cx));
                fields.push(field_row("Frame rate", seg("ex-gfps", o(&[("10", "10"), ("15", "15"), ("20", "20"), ("24", "24")]), s.gif_fps.to_string(), vec![], |s, v| s.gif_fps = v.parse().unwrap_or(15)), None).into_any_element());
            }
            Kind::Image => {
                fields.push(field_row("Format", seg("ex-ifmt", o(&[("png", "PNG"), ("jpeg", "JPEG")]), if s.image_png { "png".into() } else { "jpeg".into() }, vec![], |s, v| s.image_png = v == "png"), None).into_any_element());
                fields.push(field_row("Resolution", sel(self, "ex-ires", s.resolution.to_string(), RESOLUTIONS.iter().map(|(r, l)| (r.to_string(), l.to_string())).collect(), |s, v| s.resolution = v.parse().unwrap_or(1080), cx), None).into_any_element());
                fields.extend(framing(self, cx));
                fields.push(micro(format!("Saves the frame under the playhead, at {}. Move the playhead to pick another.", tc(time))).into_any_element());
            }
            Kind::Audio => {
                fields.push(field_row("Format", seg("ex-afmt", o(&[("wav", "WAV · lossless"), ("m4a", "M4A · smaller")]), if s.audio_wav { "wav".into() } else { "m4a".into() }, vec![], |s, v| s.audio_wav = v == "wav"), None).into_any_element());
                if !s.audio_wav {
                    fields.push(field_row("Quality", quality("ex-aq"), None).into_any_element());
                }
            }
        }
        if s.kind != Kind::Image {
            let mut disabled = if selected.is_none() { vec!["clip".to_string()] } else { vec![] };
            if self.store.in_out_range().is_none() {
                disabled.push("inout".to_string());
            }
            fields.push(
                field_row(
                    "Range",
                    seg("ex-range", o(&[("all", "Whole video"), ("inout", "In to out"), ("clip", "Selected clip"), ("custom", "Custom")]), format!("{:?}", s.range_mode).to_lowercase(), disabled, |s, v| {
                        s.range_mode = match v {
                            "clip" => RangeMode::Clip,
                            "custom" => RangeMode::Custom,
                            "inout" => RangeMode::InOut,
                            _ => RangeMode::All,
                        }
                    }),
                    None,
                )
                .into_any_element(),
            );
            if s.range_mode == RangeMode::Custom {
                fields.push(self.range_slider(duration, cx).into_any_element());
            }
            if s.range_mode == RangeMode::Clip {
                if let Some(c) = &selected {
                    fields.push(micro(format!("“{}”, {} to {}", c.name, tc(start), tc(end))).pl(px(84.)).into_any_element());
                }
            }
        }

        let summary = match s.kind {
            Kind::Audio => format!("{} · {}", if s.audio_wav { "WAV · 48 kHz · 16-bit" } else { "M4A · AAC" }, tc(length)),
            Kind::Image => format!("{w} × {h} · frame at {}", tc(time)),
            _ => {
                let fps = if s.kind == Kind::Gif { s.gif_fps } else { s.fps };
                format!("{w} × {h} · {fps} fps · {} · {} frames", tc(length), ((length * fps as f64).round() as i64).max(1))
            }
        };
        let mut warnings = vec![];
        if s.kind == Kind::Gif && length > 15.0 {
            warnings.push("GIFs get large quickly. Keep them under about 15 seconds, or lower the width and frame rate.".to_string());
        }
        if s.kind == Kind::Video && s.resolution == 2160 {
            warnings.push("4K renders effects at full size, so heavy effects (ASCII, dots, edges) will export slowly.".into());
        }
        if s.kind != Kind::Audio && s.aspect != "16:9" && s.framing == Framing::Crop {
            warnings.push("Cropping cuts off the sides of the frame. Check that titles still fit.".into());
        }
        if s.kind == Kind::Video && e.support.video.is_some() && s.audio && e.support.audio.is_none() {
            warnings.push(format!("This build can't encode audio for {}, so the file will be silent.", if s.container == Container::Mp4 { "MP4" } else { "WebM" }));
        }
        let codec = if s.kind == Kind::Video {
            Some(match e.support.video {
                Some(_) => div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .flex_none()
                    .child(icon("check", 12., success()))
                    .child(micro(format!("{}{}", e.support.video_label, if s.audio && e.support.audio.is_some() { format!(" + {}", e.support.audio_label) } else { String::new() }))),
                None => micro("Not supported here").text_color(coral()),
            })
        } else {
            None
        };

        let kinds = [(Kind::Video, "Video", "film"), (Kind::Gif, "GIF", "sparkles"), (Kind::Image, "Still", "image"), (Kind::Audio, "Audio", "audio-lines")];
        let tabs = div().grid().grid_cols(4).gap(px(6.)).children(kinds.iter().map(|(k, l, ic)| {
            let active = s.kind == *k;
            let k = *k;
            div()
                .id(SharedStr(format!("ex-kind-{l}")))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .rounded(px(10.))
                .border_1()
                .border_color(if active { alpha(ink(), 0.0) } else { hairline() })
                .bg(if active { surface3() } else { alpha(ink(), 0.0) })
                .px_2()
                .py_3()
                .cursor_pointer()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if active { ink() } else { ink_muted() })
                .hover(|s| s.bg(surface2()).text_color(ink()))
                .child(icon(ic, 16., if active { ink() } else { ink_muted() }))
                .child(l.to_string())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_export(|s| s.kind = k);
                    cx.notify();
                }))
        }));

        let body = match &e.phase {
            Phase::Setup | Phase::Error(_) => div()
                .flex()
                .flex_col()
                .gap_4()
                .child(tabs)
                .child(div().flex().flex_col().gap_3().children(fields))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .rounded(px(10.))
                        .bg(surface2())
                        .p_3()
                        .child(div().flex().items_center().justify_between().gap_3().child(timecode(summary, 12.).text_color(ink())).children(codec))
                        .children(warnings.into_iter().map(micro)),
                )
                .when_some(if let Phase::Error(m) = &e.phase { Some(m.clone()) } else { None }, |d, m| {
                    d.child(div().flex().items_start().gap_2().rounded(px(10.)).bg(alpha(coral(), 0.1)).p_3().child(icon("alert-triangle", 14., coral())).child(type_micro(div()).text_color(ink()).child(m)))
                }),
            Phase::Running => {
                let (f, l) = e.progress.lock().clone();
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().h(px(6.)).rounded_full().bg(surface3()).overflow_hidden().child(div().h_full().rounded_full().bg(ink()).w(relative(f.clamp(0.0, 1.0) as f32))))
                    .child(div().flex().justify_between().child(micro(l)).child(timecode(format!("{}%", (f * 100.0).round()), 12.).text_color(ink_muted())))
            }
            Phase::Done(o) => {
                let fname = o.path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string();
                let mut detail = mb(o.bytes);
                if let Some((w, h)) = o.size {
                    detail += &format!(" · {w} × {h}");
                }
                if o.seconds > 0.0 {
                    detail += &format!(" · {}", tc(o.seconds));
                }
                if s.kind == Kind::Video && !o.with_audio {
                    detail += " · no sound";
                }
                detail += ". Saved to your Downloads folder.";
                div()
                    .flex()
                    .items_start()
                    .gap_3()
                    .rounded(px(10.))
                    .bg(surface2())
                    .p_3()
                    .child(div().size(px(32.)).flex_none().rounded_full().bg(alpha(success(), 0.15)).flex().items_center().justify_center().child(icon("check", 16., success())))
                    .child(div().flex().flex_col().gap(px(2.)).min_w_0().child(type_caption(div()).truncate().child(fname)).child(micro(detail)))
            }
        };

        let footer = match &e.phase {
            Phase::Running => div().flex().justify_end().gap_2().child(button("ex-cancel", Variant::Secondary, 36.).child("Cancel export").on_click(cx.listener(|this, _, _, _| {
                if let Some(e) = &this.export {
                    e.cancel.store(true, Ordering::Relaxed);
                }
            }))),
            Phase::Done(o) => {
                let path = o.path.clone();
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(button("ex-again", Variant::Secondary, 36.).child("Export another").on_click(cx.listener(|this, _, _, cx| {
                        if let Some(e) = this.export.as_mut() {
                            e.phase = Phase::Setup;
                        }
                        cx.notify();
                    })))
                    .child(button("ex-reveal", Variant::Secondary, 36.).child(icon("search", 14., ink())).child("Show in Finder").on_click(move |_, _, _| {
                        crate::platform::reveal(&path);
                    }))
                    .child(button("ex-done", Variant::Primary, 36.).child("Done").on_click(cx.listener(|this, _, _, cx| {
                        this.export = None;
                        cx.notify();
                    })))
            }
            _ => {
                let label = if matches!(e.phase, Phase::Error(_)) {
                    "Try again".to_string()
                } else {
                    match s.kind {
                        Kind::Video => format!("Export {}", if s.container == Container::Mp4 { "MP4" } else { "WEBM" }),
                        Kind::Gif => "Export GIF".into(),
                        Kind::Image => format!("Save {}", if s.image_png { "PNG" } else { "JPEG" }),
                        Kind::Audio => format!("Export {}", if s.audio_wav { "WAV" } else { "M4A" }),
                    }
                };
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(button("ex-close", Variant::Secondary, 36.).child("Close").on_click(cx.listener(|this, _, _, cx| {
                        this.export = None;
                        cx.notify();
                    })))
                    .child(button("ex-start", Variant::Primary, 36.).child(label).when(!can_start, |d| d.opacity(0.4)).on_click(cx.listener(move |this, _, _, cx| {
                        if can_start {
                            this.start_export(cx);
                        }
                    })))
            }
        };

        div()
            .id("export-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(black_overlay(0.6))
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if !this.export.as_ref().map(|e| e.running()).unwrap_or(false) {
                    this.export = None;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("export-dialog")
                    .w(px(512.))
                    .max_h(relative(0.92))
                    .overflow_y_scroll()
                    .rounded(px(20.))
                    .bg(surface1())
                    .border_1()
                    .border_color(hairline())
                    .shadow_lg()
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(type_headline(div()).child("Export"))
                            .child(div().flex().gap_1().text_size(px(13.)).text_color(ink_muted()).child(project_name).child("·").child(timecode(tc(duration), 13.).text_color(ink())).child(format!("· {clip_count} clips"))),
                    )
                    .child(body)
                    .child(footer)
                    .when(running, |d| d),
            )
    }

    fn range_slider(&self, duration: f64, cx: &mut Context<Self>) -> impl IntoElement {
        let e = self.export.as_ref().unwrap();
        let d = duration.max(1.0 / FPS());
        let (a, b) = (e.custom[0].min(e.custom[1]), e.custom[0].max(e.custom[1]));
        let (fa, fb) = ((a / d).clamp(0.0, 1.0) as f32, (b / d).clamp(0.0, 1.0) as f32);
        let thumb = |i: usize, f: f32, cx: &mut Context<Self>| {
            div()
                .id(("ex-thumb", i))
                .absolute()
                .top(px(2.))
                .left(relative(f))
                .ml(px(-7.))
                .size(px(14.))
                .rounded_full()
                .bg(ink())
                .border_2()
                .border_color(surface1())
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    // Thumbs are stored unordered; drag whichever value this thumb shows.
                    let idx = if let Some(e) = &this.export {
                        let lo = if e.custom[0] <= e.custom[1] { 0 } else { 1 };
                        if i == 0 { lo } else { 1 - lo }
                    } else {
                        i
                    };
                    this.begin_drag(Drag::Range { thumb: idx }, cx);
                    this.drag_move(ev.position, gpui::Modifiers::default(), cx);
                }))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .pl(px(84.))
            .child(
                div()
                    .relative()
                    .h(px(18.))
                    .child(self.track_bounds("ex-range"))
                    .child(div().absolute().left_0().right_0().top(px(7.)).h(px(4.)).rounded_full().bg(surface3()))
                    .child(div().absolute().top(px(7.)).h(px(4.)).bg(ink()).left(relative(fa)).w(relative(fb - fa)))
                    .child(thumb(0, fa, cx))
                    .child(thumb(1, fb, cx)),
            )
            .child(div().flex().justify_between().child(timecode(tc(a), 11.).text_color(ink_muted())).child(timecode(tc(b), 11.).text_color(ink_muted())))
    }
}

#[allow(non_snake_case)]
fn SharedStr(s: String) -> gpui::SharedString {
    gpui::SharedString::from(s)
}
