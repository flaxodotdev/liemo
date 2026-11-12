// The timeline: toolbar, ruler, track headers and lanes with clips, transitions, the drop preview,
// snapping guide and playhead. Scrolling is our own, so the header column and ruler stay put.

use super::{menus, ClipMode, Drag, DropTarget, Editor};
use crate::fx::registry::effect_def;
use crate::media::audio::{peaks, PEAK_RATE};
use crate::model::assets::{asset, Asset};
use crate::model::keys;
use crate::model::store::{stack_rows, track_height, EditMode};
use crate::model::time::snap_frame;
use crate::model::types::*;
use crate::render::compositor::transition_windows;
use crate::ui::editor::library::asset_image;
use crate::ui::theme::*;
use gpui::prelude::*;
use gpui::{canvas, div, fill, img, point, px, AnyElement, Bounds, Context, FontWeight, Hsla, MouseButton, MouseDownEvent, ObjectFit, PathBuilder, Pixels, ScrollEvent, SharedString, Window};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::menu::{ContextMenuExt, DropdownMenu};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

const RULER_H: f32 = 28.0;
const HEAD_W: f32 = 164.0;

fn clip_bg(k: AssetType) -> Hsla {
    match k {
        AssetType::Scene | AssetType::Video | AssetType::Image | AssetType::Photo => hex("#2a2a2e"),
        AssetType::Overlay => hex("#3a2a44"),
        AssetType::Text => hex("#2c2a48"),
        AssetType::Music | AssetType::Audio => hex("#1f3328"),
    }
}

impl Editor {
    fn content_w(&self) -> f32 {
        ((self.store.end() + 20.0).max(60.0) * self.store.zoom) as f32
    }
    fn clamp_scroll(&mut self) {
        let vw = self.bounds("tl-view").map(|b| f32::from(b.size.width)).unwrap_or(800.0);
        let vh = self.bounds("tl-lanes-view").map(|b| f32::from(b.size.height)).unwrap_or(200.0);
        let ch: f32 = self.store.tracks.iter().map(|t| self.lane_height(t)).sum();
        self.scroll_x = self.scroll_x.clamp(0.0, (self.content_w() - vw).max(0.0));
        self.scroll_y = self.scroll_y.clamp(0.0, (ch - vh).max(0.0));
    }

    pub(super) fn render_timeline(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Follow the playhead while playing.
        if self.store.playing {
            let x = (self.store.time * self.store.zoom) as f32;
            let vw = self.bounds("tl-view").map(|b| f32::from(b.size.width)).unwrap_or(800.0);
            if x > self.scroll_x + vw - 60.0 || x < self.scroll_x {
                self.scroll_x = (x - 80.0).max(0.0);
            }
        }
        self.clamp_scroll();
        let s = &self.store;
        let zoom = s.zoom;
        let snap = s.snap;
        let edit_mode = s.edit_mode;

        let toolbar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.))
            .border_b_1()
            .border_color(hairline_soft())
            .px_3()
            .py_2()
            .child(
                button("split", Variant::Secondary, 32.)
                    .child(icon("scissors", 14., ink()))
                    .child("Split")
                    .tooltip(|w, cx| gpui_component::tooltip::Tooltip::new("Split at playhead  S").build(w, cx))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.store.split();
                        cx.notify();
                    })),
            )
            .child(icon_button("dup", "copy", "Duplicate clip", Some("⌘D"), 32., Variant::Secondary, cx.listener(|this, _, _, cx| {
                this.store.duplicate();
                cx.notify();
            })))
            .child(icon_button("del", "trash-2", "Delete clip", Some("⌫"), 32., Variant::Secondary, cx.listener(|this, _, _, cx| {
                this.store.remove();
                cx.notify();
            })))
            .child(div().w(px(1.)).h(px(18.)).mx_1().bg(hairline()))
            .child(div().w(px(170.)).child(segmented(
                "editmode",
                &[(EditMode::Insert, "Insert"), (EditMode::Overwrite, "Overwrite")],
                &edit_mode,
                |_| false,
                {
                    let ed = cx.entity();
                    move |v, _, cx| {
                        ed.update(cx, |this, cx| {
                            if this.store.edit_mode != v {
                                this.store.edit_mode = v;
                                this.store.toast(if v == EditMode::Insert { "Insert: clips move along to make room" } else { "Overwrite: new clips cut into what's underneath" });
                            }
                            cx.notify();
                        })
                    }
                },
            )))
            .child(
                button("snap", if snap { Variant::Secondary } else { Variant::Ghost }, 32.)
                    .when(snap, |d| d.bg(surface3()).border_1().border_color(hairline()))
                    .child(icon("magnet", 14., if snap { ink() } else { ink_muted() }))
                    .child("Snap")
                    .tooltip(move |w, cx| gpui_component::tooltip::Tooltip::new(if snap { "Turn snapping off  N" } else { "Turn snapping on  N" }).build(w, cx))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.store.snap = !this.store.snap;
                        cx.notify();
                    })),
            )
            .child(div().w(px(1.)).h(px(18.)).mx_1().bg(hairline()))
            .child({
                let ed = cx.entity();
                Button::new("captions")
                    .label(if self.captioning { "Captioning…" } else { "Captions" })
                    .icon(gpui_component::Icon::empty().path("rayna/icons/captions.svg"))
                    .ghost()
                    .dropdown_caret(true)
                    .h(px(32.))
                    .text_size(px(13.))
                    .dropdown_menu(move |menu, window, cx| menus::captions_menu(menu.min_w(px(220.)), &ed, window, cx))
            })
            .child(spacer())
            .child(
                div()
                    .w(px(176.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(icon("zoom-out", 16., ink_muted()))
                    .child(self.slider("tl-zoom", zoom, 12.0, 240.0, 1.0, false, Rc::new(|e: &mut Editor, v| e.zoom_to(v)), cx))
                    .child(icon("zoom-in", 16., ink_muted())),
            )
            .child(icon_button("tl-close", "x", "Close timeline", Some("⌘J"), 28., Variant::Ghost, cx.listener(|this, _, _, cx| this.toggle_panel(super::layout::Edge::Timeline, cx))));

        // Header column: corner, then the track heads, scrolled with the lanes.
        let mut heads = div().absolute().left_0().right_0().top(px(-self.scroll_y)).flex().flex_col();
        let last = self.store.tracks.last().map(|t| t.id.clone());
        for t in self.store.tracks.clone() {
            let end = Some(&t.id) == last.as_ref();
            heads = heads.child(self.track_head(&t, end, window, cx));
        }
        heads = heads.child({
            let ed = cx.entity();
            div().px_2().py_2().child(
                Button::new("add-track")
                    .label("Add track")
                    .icon(gpui_component::Icon::empty().path("rayna/icons/plus.svg"))
                    .ghost()
                    .h(px(28.))
                    .text_size(px(12.))
                    .dropdown_menu(move |menu, _, _| menus::add_track_menu(menu, &ed)),
            )
        });
        let header_col = div()
            .w(px(HEAD_W))
            .flex_none()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(hairline_soft())
            .child(div().h(px(RULER_H)).flex_none().border_b_1().border_color(hairline_soft()).child(self.meters()))
            .child(div().relative().flex_1().overflow_hidden().child(heads));

        // Lanes.
        let content_w = self.content_w();
        let mut lanes = div().absolute().left(px(-self.scroll_x)).top(px(-self.scroll_y)).w(px(content_w)).flex().flex_col();
        for t in self.store.tracks.clone() {
            let end = Some(&t.id) == last.as_ref();
            lanes = lanes.child(self.lane(&t, end, cx));
        }
        let lanes_view = div()
            .id("tl-lanes")
            .absolute()
            .top(px(RULER_H))
            .left_0()
            .right_0()
            .bottom_0()
            .overflow_hidden()
            .child(self.track_bounds("tl-lanes-view"))
            .child(lanes);

        let view = div()
            .id("tl-view")
            .relative()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .child(self.track_bounds("tl-view"))
            .child(lanes_view)
            .child(self.ruler(content_w, cx))
            .when_some(self.store.in_out_range(), |d, (a, b)| {
                // Outside in and out is shaded.
                let (xa, xb) = ((a * zoom) as f32 - self.scroll_x, (b * zoom) as f32 - self.scroll_x);
                d.child(div().absolute().top(px(RULER_H)).bottom_0().left_0().w(px(xa.max(0.0))).bg(black_overlay(0.35)))
                    .child(div().absolute().top(px(RULER_H)).bottom_0().left(px(xb)).right_0().bg(black_overlay(0.35)))
            })
            .children(self.store.markers.iter().map(|m| div().absolute().top(px(RULER_H)).bottom_0().left(px((m.t * zoom) as f32 - self.scroll_x)).w(px(1.)).bg(alpha(hex(&m.color), 0.45))))
            .when_some(self.snap_at, |d, t| d.child(div().absolute().top(px(RULER_H)).bottom_0().left(px((t * zoom) as f32 - self.scroll_x)).w(px(1.)).bg(signal())))
            .when_some(self.marquee.zip(self.bounds("tl-view")), |d, (m, v)| {
                d.child(
                    div()
                        .absolute()
                        .left(m.origin.x - v.origin.x)
                        .top(m.origin.y - v.origin.y)
                        .w(m.size.width)
                        .h(m.size.height)
                        .border_1()
                        .border_color(signal())
                        .bg(alpha(signal(), 0.08)),
                )
            })
            .child(self.playhead());

        panel().flex_1().child(toolbar).child(
            div()
                .id("tl-body")
                .flex()
                .flex_1()
                .min_h_0()
                .on_scroll(cx.listener(|this, ev: &ScrollEvent, _, cx| {
                    let d = ev.delta.pixel_delta(px(20.));
                    if ev.modifiers.platform || ev.modifiers.control {
                        let ax = this.bounds("tl-view").map(|b| f32::from(ev.position.x) - f32::from(b.origin.x));
                        this.zoom_by(if f32::from(d.y) > 0.0 { 1.12 } else { 0.89 }, ax);
                    } else {
                        this.scroll_x -= f32::from(d.x);
                        this.scroll_y -= f32::from(d.y);
                    }
                    this.clamp_scroll();
                    cx.notify();
                }))
                .child(header_col)
                .child(view),
        )
    }

    fn ruler(&self, content_w: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom = self.store.zoom;
        let steps = [0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0];
        let major = steps.iter().copied().find(|s| s * zoom >= 90.0).unwrap_or(120.0);
        let minor = major / if major >= 5.0 { 5.0 } else if major >= 1.0 { 4.0 } else { 5.0 };
        let vw = self.bounds("tl-view").map(|b| f32::from(b.size.width)).unwrap_or(1600.0);
        let t0 = (self.scroll_x as f64 / zoom / major).floor() * major;
        let t1 = (self.scroll_x + vw) as f64 / zoom;
        let mut ticks: Vec<AnyElement> = vec![];
        let mut t = t0.max(0.0);
        while t <= t1 + major {
            let label = format!("{}:{:02}{}", (t / 60.0).floor() as i64, (t % 60.0).floor() as i64, if t % 1.0 != 0.0 { ".5" } else { "" });
            ticks.push(
                div()
                    .absolute()
                    .bottom_0()
                    .left(px((t * zoom) as f32))
                    .h(px(12.))
                    .border_l_1()
                    .border_color(hex("#525252"))
                    .child(type_timecode(div(), 10.).absolute().top(px(-14.)).left(px(6.)).whitespace_nowrap().text_color(ink_muted()).child(label))
                    .into_any_element(),
            );
            t += major;
        }
        let sx = self.scroll_x;
        let minor_px = (minor * zoom) as f32;
        div()
            .id("ruler")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(RULER_H))
            .overflow_hidden()
            .bg(surface1())
            .border_b_1()
            .border_color(hairline_soft())
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .child(
                canvas(|_, _, _| {}, move |b: Bounds<Pixels>, _, window, _| {
                    let mut x = -(sx % minor_px);
                    while x < f32::from(b.size.width) {
                        window.paint_quad(fill(Bounds::new(point(b.origin.x + px(x), b.origin.y + b.size.height - px(6.)), gpui::size(px(1.), px(6.))), surface3()));
                        x += minor_px;
                    }
                })
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .child(div().absolute().top_0().bottom_0().left(px(-self.scroll_x)).w(px(content_w)).children(ticks).children(self.ruler_marks(cx)))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.start_scrub(ev.position, cx);
            }))
    }

    /// In/out bracket and markers on the ruler, in content coordinates.
    fn ruler_marks(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let zoom = self.store.zoom;
        let mut out = vec![];
        if let Some((a, b)) = self.store.in_out_range() {
            let tip = format!("In {} · Out {}. I and O set them, ⌥X clears.", crate::model::time::tc(a), crate::model::time::tc(b));
            out.push(
                div()
                    .id("in-out")
                    .absolute()
                    .top(px(RULER_H - 6.0))
                    .h(px(6.))
                    .left(px((a * zoom) as f32))
                    .w(px(((b - a) * zoom) as f32))
                    .bg(alpha(signal(), 0.75))
                    .rounded(px(2.))
                    .tooltip(move |w, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(w, cx))
                    .into_any_element(),
            );
        }
        let ed = cx.entity();
        for (i, m) in self.store.markers.iter().enumerate() {
            let x = (m.t * zoom) as f32;
            let renaming = self.renaming == Some(super::Rename::Marker(i));
            let t = m.t;
            let name = m.name.clone();
            let ed = ed.clone();
            let mut flag = div()
                .id(SharedString::from(format!("marker-{i}")))
                .absolute()
                .top(px(2.))
                .left(px(x))
                .h(px(14.))
                .flex()
                .items_center()
                .gap_1()
                .pr_1()
                .rounded_r(px(3.))
                .bg(hex(&m.color))
                .cursor_pointer()
                .text_size(px(9.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(black())
                .whitespace_nowrap()
                .child(div().w(px(2.)).h_full().bg(black_overlay(0.4)))
                .tooltip(move |w, cx| gpui_component::tooltip::Tooltip::new(format!("{name} · click to go there, right-click to rename")).build(w, cx))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if ev.click_count >= 2 {
                        this.start_rename(super::Rename::Marker(i), window, cx);
                        return;
                    }
                    if this.store.playing {
                        this.pause();
                    }
                    this.seek(t);
                    cx.notify();
                }));
            if renaming {
                flag = flag.child(div().w(px(120.)).child(gpui_component::input::Input::new(&self.rename_input).appearance(false).text_size(px(10.)).h(px(14.))));
            } else if !m.name.is_empty() {
                flag = flag.child(m.name.clone());
            }
            out.push(flag.context_menu(move |menu, _, _| menus::marker_menu(menu, &ed, i)).into_any_element());
        }
        out
    }

    /// Stereo output meters in the timeline's corner, live while playing.
    fn meters(&self) -> impl IntoElement {
        let lv = if self.store.playing { self.audio.levels() } else { [0.0; 2] };
        let bar = |v: f32| {
            // −48 dB to 0 dB across the bar.
            let db = 20.0 * v.max(1e-5).log10();
            let f = ((db + 48.0) / 48.0).clamp(0.0, 1.0);
            let col = if db > -1.0 { coral() } else if db > -9.0 { hex("#ffd166") } else { hex("#78e6a0") };
            div().relative().h(px(4.)).w_full().rounded_full().bg(surface3()).child(div().absolute().left_0().top_0().bottom_0().rounded_full().w(gpui::relative(f)).bg(col))
        };
        div()
            .id("meters")
            .size_full()
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(3.))
            .px_3()
            .child(bar(lv[0]))
            .child(bar(lv[1]))
            .tooltip(|w, cx| gpui_component::tooltip::Tooltip::new("Output level, left and right. Red means it's clipping.").build(w, cx))
    }

    pub fn start_scrub(&mut self, pos: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let was_playing = self.store.playing;
        if was_playing {
            self.pause();
        }
        let t = snap_frame(self.time_at(pos.x));
        self.seek(t);
        self.begin_drag(Drag::Scrub { was_playing }, cx);
    }

    fn playhead(&self) -> impl IntoElement {
        let x = (self.store.time * self.store.zoom) as f32 - self.scroll_x;
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(px(x))
            .w(px(0.))
            .child(div().absolute().top_0().bottom_0().left(px(-0.5)).w(px(1.)).bg(ink()))
            .child(
                canvas(|_, _, _| {}, |b: Bounds<Pixels>, _, window, _| {
                    let o = b.origin;
                    let mut pb = PathBuilder::fill();
                    pb.move_to(point(o.x, o.y));
                    pb.line_to(point(o.x + px(12.), o.y));
                    pb.line_to(point(o.x + px(12.), o.y + px(8.4)));
                    pb.line_to(point(o.x + px(6.), o.y + px(14.)));
                    pb.line_to(point(o.x, o.y + px(8.4)));
                    pb.close();
                    if let Ok(p) = pb.build() {
                        window.paint_path(p, ink());
                    }
                })
                .absolute()
                .top(px(2.))
                .left(px(-6.))
                .w(px(12.))
                .h(px(14.)),
            )
    }

    /// `last`: the bottom track draws no divider under itself (it would sit on the panel's edge).
    fn track_head(&self, t: &Track, last: bool, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = t.id.clone();
        let small = |name: &str, active: bool, tip: String, key: &'static str, cx: &mut Context<Self>| {
            let id = id.clone();
            div()
                .id(SharedString::from(format!("th-{id}-{key}")))
                .size(px(24.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|s| s.bg(surface2()))
                .child(icon(name, 14., if active { coral() } else { ink_muted() }))
                .tooltip(move |w, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(w, cx))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.store.toggle_track(&id, key);
                    if key == "muted" {
                        this.restart_audio();
                    }
                    cx.notify();
                }))
        };
        let toggle = if t.kind == TrackKind::Audio {
            small(if t.muted { "volume-x" } else { "volume-2" }, t.muted, if t.muted { "Unmute".into() } else { "Mute".into() }, "muted", cx)
        } else {
            small(if t.hidden { "eye-off" } else { "eye" }, t.hidden, if t.hidden { "Show".into() } else { "Hide".into() }, "hidden", cx)
        };
        let lock = small(if t.locked { "lock" } else { "lock-open" }, t.locked, if t.locked { "Unlock".into() } else { "Lock".into() }, "locked", cx);
        let (ed, track) = (cx.entity(), t.clone());
        div()
            .id(SharedString::from(format!("head-{}", t.id)))
            .h(px(self.lane_height(t)))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .when(!last, |d| d.border_b_1())
            .border_color(hairline_soft())
            .pl_3()
            .pr(px(6.))
            .child(type_timecode(div(), 11.).w(px(22.)).text_color(ink_muted()).child(t.id.clone()))
            .child(if self.renaming == Some(super::Rename::Track(t.id.clone())) {
                div().flex_1().min_w_0().child(gpui_component::input::Input::new(&self.rename_input).appearance(false).text_size(px(12.)).h(px(22.))).into_any_element()
            } else {
                let tid = t.id.clone();
                type_micro(div())
                    .id(SharedString::from(format!("tname-{}", t.id)))
                    .flex_1()
                    .font_weight(FontWeight::MEDIUM)
                    .truncate()
                    .child(t.name.clone())
                    .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| {
                        if ev.click_count() >= 2 {
                            this.start_rename(super::Rename::Track(tid.clone()), window, cx);
                        }
                    }))
                    .into_any_element()
            })
            .child(toggle)
            .child(lock)
            .context_menu(move |menu, window, cx| {
                // The track as it is when the menu opens.
                let t = ed.read(cx).store.tracks.iter().find(|x| x.id == track.id).cloned().unwrap_or(track.clone());
                menus::track_menu(menu, &ed, &t, window, cx)
            })
    }

    fn lane(&self, t: &Track, last: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom = self.store.zoom;
        let target = match &self.drop_target {
            Some(DropTarget::Lane { track, time, ok }) if *track == t.id => Some((*time, *ok)),
            _ => None,
        };
        let mut mine: Vec<&Clip> = self.store.clips.iter().filter(|c| c.track == t.id).collect();
        mine.sort_by(|a, b| a.start.total_cmp(&b.start));
        let ghost_dur = self.lib_drag.as_ref().and_then(|p| asset(&p.id)).map(|a| a.dur).unwrap_or(0.0);
        let dim = t.hidden || t.muted;
        let row_h = track_height(t.kind);
        let (rows, n) = stack_rows(&self.store.clips, &t.id);
        let h = row_h * n as f32;
        let mut lane = div()
            .id(SharedString::from(format!("lane-{}", t.id)))
            .relative()
            .h(px(h))
            .flex_none()
            .when(!last, |d| d.border_b_1())
            .border_color(hairline_soft())
            .when(target.map(|t| t.1).unwrap_or(false), |d| d.bg(alpha(signal(), 0.06)))
            .when(target.map(|t| !t.1).unwrap_or(false), |d| d.bg(alpha(coral(), 0.05)))
            .child(self.track_bounds(format!("lane:{}", t.id)))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                if this.store.playing {
                    this.pause();
                }
                this.start_marquee(ev, cx);
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, ev: &MouseDownEvent, _, _| {
                this.ctx_time = snap_frame(this.time_at(ev.position.x));
            }));
        for c in &mine {
            let row = rows.get(&c.id).copied().unwrap_or(0);
            lane = lane.child(self.clip_view(c, row_h, row as f32 * row_h, dim, cx));
        }
        for w in transition_windows(&mine) {
            let bid = w.b.id.clone();
            let wpx = ((w.to - w.from) * zoom) as f32;
            let name = w.def.name;
            let tip = format!("{} · {:.2}s. Click to edit.", w.def.name, w.to - w.from);
            lane = lane.child(
                div()
                    .id(SharedString::from(format!("tr-{bid}")))
                    .absolute()
                    .top(px(row_h / 2.0 - 10.0))
                    .left(px((w.from * zoom) as f32))
                    .w(px(wpx.max(20.0)))
                    .h(px(20.))
                    .rounded_full()
                    .bg(ink())
                    .hover(|s| s.bg(hex("#ffffff")))
                    .shadow_md()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .px(px(6.))
                    .overflow_hidden()
                    .text_size(px(10.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(black())
                    .child(icon("blend", 12., black()))
                    .when(wpx > 64.0, |d| d.child(div().truncate().child(name)))
                    .tooltip(move |win, cx| gpui_component::tooltip::Tooltip::new(tip.clone()).build(win, cx))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.store.select(Some(bid.clone()));
                        cx.notify();
                    })),
            );
        }
        if let Some((time, true)) = target {
            lane = lane.child(
                div()
                    .absolute()
                    .top(px(4.))
                    .bottom(px(4.))
                    .left(px((time * zoom) as f32))
                    .w(px(((ghost_dur.min(24.0)) * zoom).max(6.0) as f32))
                    .rounded(px(6.))
                    .border_1()
                    .border_dashed()
                    .border_color(signal())
                    .bg(alpha(signal(), 0.1)),
            );
        }
        let (ed, track) = (cx.entity(), t.id.clone());
        lane.w_full().context_menu(move |menu, _, cx| {
            let time = ed.read(cx).ctx_time;
            menus::lane_menu(menu, &ed, track.clone(), time)
        })
    }

    /// A track's height: one row, or a row per stack of overlapping titles.
    fn lane_height(&self, t: &Track) -> f32 {
        track_height(t.kind) * stack_rows(&self.store.clips, &t.id).1 as f32
    }

    fn clip_view(&self, c: &Clip, lane_h: f32, top: f32, dim: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let zoom = self.store.zoom;
        let selected = self.store.is_selected(&c.id);
        let fx_target = matches!(&self.drop_target, Some(DropTarget::Clip(id)) if *id == c.id);
        let dragging = self.dragging_clip.as_deref() == Some(&c.id);
        let w = ((c.dur * zoom) as f32).max(6.0);
        let h = lane_h - 8.0;
        let on: Vec<&ClipEffect> = c.effects.iter().filter(|e| e.enabled).collect();
        let fx = match on.len() {
            0 => None,
            1 => effect_def(&on[0].id).map(|d| d.name.to_string()),
            n => Some(format!("{n} effects")),
        };
        let group_name = SharedString::from(format!("clip-{}", c.id));
        let id = c.id.clone();
        let (idl, idr) = (c.id.clone(), c.id.clone());

        let mut el = div()
            .id(SharedString::from(format!("clip-{}", c.id)))
            .group(group_name.clone())
            .absolute()
            .left(px((c.start * zoom) as f32))
            .top(px(top + 4.))
            .w(px(w))
            .h(px(h))
            .overflow_hidden()
            .rounded(px(6.))
            .bg(clip_bg(c.kind))
            .cursor_grab()
            .when(dim, |d| d.opacity(0.4))
            .when(dragging, |d| d.opacity(0.9).shadow_lg())
            .child(self.track_bounds(format!("clip:{}", c.id)));

        // Picture strip, waveform or text preview under the name bar.
        let body_h = (h - 18.0).max(1.0);
        if c.visual() && c.kind != AssetType::Text {
            if let Some(a) = asset(&c.asset) {
                if let Some(thumb) = asset_image(&a) {
                    let tw = body_h * 16.0 / 9.0;
                    let n = ((w / tw).ceil() as usize).clamp(1, 60);
                    el = el.child(div().absolute().top(px(18.)).left_0().flex().opacity(0.85).children((0..n).map(|i| {
                        // GPUI clips to rectangles, so the strip's own outer corners carry the clip's rounding.
                        img(thumb.clone())
                            .w(px(tw))
                            .h(px(body_h))
                            .flex_none()
                            .when(i == 0, |d| d.rounded_bl(px(6.)))
                            .when(i + 1 == n && (i as f32 + 1.0) * tw <= w + 0.5, |d| d.rounded_br(px(6.)))
                            .object_fit(ObjectFit::Cover)
                    })));
                }
            }
        }
        if c.kind.is_audio() {
            // Beat ticks along the top of the waveform, when they're far enough apart to read.
            let beats: Vec<f32> = self.beat_times(c).iter().map(|t| ((t - c.start) * zoom) as f32).collect();
            if beats.len() > 1 && beats[1] - beats[0] >= 6.0 {
                el = el.child(
                    canvas(|_, _, _| {}, move |b: Bounds<Pixels>, _, window, _| {
                        for (i, x) in beats.iter().enumerate() {
                            let bar = i % 4 == 0;
                            window.paint_quad(fill(Bounds::new(point(b.origin.x + px(*x), b.origin.y), gpui::size(px(1.), px(if bar { 6. } else { 3. }))), white_a(if bar { 0.55 } else { 0.3 })));
                        }
                    })
                    .absolute()
                    .top(px(18.))
                    .left_0()
                    .right_0()
                    .h(px(6.)),
                );
            }
        }
        if let Some(wave) = self.waveform_for(c) {
            if c.kind.is_audio() {
                el = el.child(wave_canvas(wave, c.clone(), zoom).absolute().top(px(18.)).bottom(px(2.)).left_0().right_0());
            } else if !c.frozen() {
                // A video's own sound, along the bottom of its picture strip.
                el = el.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(px((body_h * 0.42).clamp(10.0, 22.0)))
                        .bg(black_overlay(0.45))
                        .child(wave_canvas(wave, c.clone(), zoom).size_full()),
                );
            }
        }
        if let Some(t) = &c.text {
            el = el.child(
                div()
                    .absolute()
                    .top(px(18.))
                    .bottom_0()
                    .left(px(8.))
                    .right(px(8.))
                    .flex()
                    .items_center()
                    .font_family(SharedString::from(t.font.clone()))
                    .font_weight(FontWeight(t.weight as f32))
                    .text_size(px(13.))
                    .text_color(white_a(0.85))
                    .child(div().truncate().child(t.content.split('\n').next().unwrap_or("").to_string())),
            );
        }
        el = el.child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(18.))
                .flex()
                .items_center()
                .gap(px(6.))
                .overflow_hidden()
                .rounded_t(px(6.))
                .bg(black_overlay(0.38))
                .px_2()
                .text_size(px(11.))
                .font_weight(FontWeight::MEDIUM)
                .whitespace_nowrap()
                .when_some(speed_badge(c), |d, b| d.child(type_timecode(div(), 9.).rounded(px(3.)).bg(alpha(signal(), 0.85)).text_color(black()).px_1().child(b)))
                .child(c.name.clone())
                .when_some(fx, |d, f| d.child(type_timecode(div(), 9.).rounded(px(3.)).bg(white_a(0.15)).px_1().child(f.to_uppercase()))),
        );
        // Fade wedges.
        let (fi, fo) = ((c.fade_in * zoom) as f32, (c.fade_out * zoom) as f32);
        if fi > 0.0 || fo > 0.0 {
            el = el.child(
                canvas(|_, _, _| {}, move |b: Bounds<Pixels>, _, window, _| {
                    let (o, bw, bh) = (b.origin, b.size.width, b.size.height);
                    let shade = black_overlay(0.55);
                    if fi > 0.0 {
                        let mut pb = PathBuilder::fill();
                        pb.move_to(o);
                        pb.line_to(point(o.x + px(fi), o.y + bh));
                        pb.line_to(point(o.x, o.y + bh));
                        pb.close();
                        if let Ok(p) = pb.build() {
                            window.paint_path(p, shade);
                        }
                    }
                    if fo > 0.0 {
                        let mut pb = PathBuilder::fill();
                        pb.move_to(point(o.x + bw, o.y));
                        pb.line_to(point(o.x + bw, o.y + bh));
                        pb.line_to(point(o.x + bw - px(fo), o.y + bh));
                        pb.close();
                        if let Ok(p) = pb.build() {
                            window.paint_path(p, shade);
                        }
                    }
                })
                .absolute()
                .top(px(18.))
                .bottom_0()
                .left_0()
                .right_0(),
            );
        }
        // Outline: selection or effect drop target.
        el = el.child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .rounded(px(6.))
                .border_color(if selected || fx_target || dragging { signal() } else { white_a(0.08) })
                .when(selected || fx_target || dragging, |d| d.border_2())
                .when(!(selected || fx_target || dragging), |d| d.border_1()),
        );
        // Keyframes: a diamond for each keyed moment, along the bottom edge.
        if !c.keys.is_empty() {
            let ed = cx.entity();
            for (i, kt) in keys::key_times(&c.keys).into_iter().filter(|t| *t >= -1e-6 && *t <= c.dur + 1e-6).enumerate() {
                let ease = c.keys.values().flatten().find(|k| (k.t - kt).abs() < keys::SAME).map(|k| k.ease);
                let x = (kt * zoom) as f32;
                let (cid, cid2, start) = (c.id.clone(), c.id.clone(), c.start);
                let ed = ed.clone();
                let at_head = (self.store.time - (c.start + kt)).abs() < keys::SAME;
                el = el.child(
                    div()
                        .id(SharedString::from(format!("key-{}-{i}", c.id)))
                        .absolute()
                        .left(px(x - 6.))
                        .bottom(px(1.))
                        .size(px(12.))
                        .cursor_pointer()
                        .child(diamond(if at_head { signal() } else { ink() }, ease == Some(keys::Ease::Hold)))
                        .tooltip(move |w, cx| gpui_component::tooltip::Tooltip::new("Keyframe. Click to go to it, right-click to change it.").build(w, cx))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if this.store.playing {
                                this.pause();
                            }
                            if !this.store.is_selected(&cid) {
                                this.store.select(Some(cid.clone()));
                            }
                            this.seek(start + kt);
                            cx.notify();
                        }))
                        .context_menu(move |menu, _, _| menus::key_menu(menu, &ed, cid2.clone(), kt, ease)),
                );
            }
        }
        // Trim handles. Option-drag the right one to change speed instead (rate stretch).
        let handle = |side: &'static str, mode: ClipMode, cid: String, cx: &mut Context<Self>| {
            div()
                .id(SharedString::from(format!("h{side}-{cid}")))
                .absolute()
                .top_0()
                .bottom_0()
                .when(side == "l", |d| d.left_0())
                .when(side == "r", |d| d.right_0())
                .w(px(8.))
                .cursor(gpui::CursorStyle::ResizeLeftRight)
                .child(
                    div()
                        .absolute()
                        .top(px(h / 2.0 - 4.0))
                        .when(side == "l", |d| d.left(px(3.)))
                        .when(side == "r", |d| d.right(px(3.)))
                        .w(px(2.))
                        .h(px(14.))
                        .rounded(px(2.))
                        .bg(white_a(0.5))
                        .when(!selected, |d| d.opacity(0.).group_hover(group_name.clone(), |s| s.opacity(1.))),
                )
                .when(side == "r", |d| d.tooltip(|w, cx| gpui_component::tooltip::Tooltip::new("Drag to trim. Hold ⌥ to change speed instead.").build(w, cx)))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.start_clip_drag(&cid, mode, ev.position, ev.modifiers, cx);
                }))
        };
        let ed = cx.entity();
        let rid = c.id.clone();
        el.child(handle("l", ClipMode::Left, idl, cx))
            .child(handle("r", ClipMode::Right, idr, cx))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.start_clip_drag(&id, ClipMode::Move, ev.position, ev.modifiers, cx);
            }))
            // Right-click: the menu acts on the selection, so a clip outside it becomes the selection.
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, _, _, cx| {
                if !this.store.is_selected(&rid) {
                    this.store.select(Some(rid.clone()));
                } else {
                    this.store.focus_in_selection(&rid);
                }
                cx.notify();
            }))
            .context_menu(move |menu, window, cx| menus::clip_menu(menu.min_w(px(220.)), &ed, window, cx))
    }

    /// The clip's sound as a waveform: imported audio and video from its decoded peaks, built-in
    /// music from its synthesised loop.
    fn waveform_for(&self, c: &Clip) -> Option<Wave> {
        match c.kind {
            AssetType::Audio | AssetType::Video => peaks(&c.asset).map(|p| Wave { peaks: p, rate: PEAK_RATE, loop_len: None }),
            AssetType::Music => {
                let a = asset(&c.asset)?;
                (a.bpm > 0.0).then(|| music_wave(&self.music_peaks, &a))
            }
            _ => None,
        }
    }
}

/// Peaks to draw: `rate` per second; built-in music repeats every `loop_len` seconds.
#[derive(Clone)]
pub struct Wave {
    peaks: Arc<Vec<f32>>,
    rate: f64,
    loop_len: Option<f64>,
}
impl Wave {
    /// The loudest peak between two source times.
    fn max_between(&self, s0: f64, s1: f64) -> f32 {
        let n = self.peaks.len();
        if n == 0 {
            return 0.0;
        }
        let (a, b) = (s0.min(s1), s0.max(s1));
        let idx = |s: f64| -> i64 {
            let s = match self.loop_len {
                Some(l) => s.rem_euclid(l),
                None => s,
            };
            (s * self.rate).floor() as i64
        };
        let (i0, mut i1) = (idx(a), idx(b));
        if self.loop_len.is_some() && i1 < i0 {
            i1 += n as i64;
        }
        let mut m = 0f32;
        let mut i = i0;
        // Zoomed far out, sample the range rather than reading every peak.
        let step = ((i1 - i0) / 64).max(1);
        while i <= i1 {
            let j = if self.loop_len.is_some() { i.rem_euclid(n as i64) } else { i };
            if j >= 0 && (j as usize) < n {
                m = m.max(self.peaks[j as usize]);
            }
            i += step;
        }
        m
    }
}

/// Each built-in track's waveform: 48 beats (where its pattern repeats) of its synthesised mix,
/// made once.
pub type MusicPeaks = Rc<RefCell<HashMap<String, Wave>>>;
fn music_wave(cache: &MusicPeaks, a: &Asset) -> Wave {
    if let Some(w) = cache.borrow().get(&a.id) {
        return w.clone();
    }
    let sr = 8000.0;
    let beat = 60.0 / a.bpm;
    let beats = 48i64;
    let len = beats as f64 * beat;
    // Render the second time round, so tails from the beats before ring into the start.
    let t0 = len;
    let frames = (len * sr).ceil() as usize;
    let mut out = vec![0f32; frames * 2];
    for b in 0..beats * 2 {
        crate::audio::mix::play_beat(&mut out, t0, sr, a, b, b as f64 * beat, 1.0);
    }
    let per = (sr / PEAK_RATE) as usize * 2;
    let mut p: Vec<f32> = out.chunks(per).map(|c| c.iter().fold(0f32, |m, v| m.max(v.abs()))).collect();
    // Scale to the track's usual level rather than its loudest hit, so the beat reads as a body
    // of sound with kicks standing out, not a row of spikes.
    let mut sorted = p.clone();
    sorted.sort_by(f32::total_cmp);
    let typical = sorted.get(sorted.len() * 3 / 4).copied().unwrap_or(0.0).max(1e-6);
    for v in p.iter_mut() {
        *v = (*v / typical * 0.55).min(1.0);
    }
    let w = Wave { peaks: Arc::new(p), rate: PEAK_RATE, loop_len: Some(len) };
    cache.borrow_mut().insert(a.id.clone(), w.clone());
    w
}

/// A clip's waveform, mirrored about the middle, drawn for the part of the clip on screen. Its
/// height follows the clip's volume (and volume keyframes) and fades.
fn wave_canvas(wave: Wave, c: Clip, zoom: f64) -> gpui::Canvas<()> {
    canvas(|_, _, _| {}, move |b: Bounds<Pixels>, _, window, _| {
        let mask = window.content_mask().bounds;
        let (bx, bw, bh) = (f32::from(b.origin.x), f32::from(b.size.width), f32::from(b.size.height));
        let x0 = (f32::from(mask.origin.x) - bx).max(0.0).floor();
        let x1 = (f32::from(mask.origin.x + mask.size.width) - bx).min(bw).ceil();
        if x1 <= x0 || bh <= 1.0 {
            return;
        }
        let color = gpui::Rgba { r: 120.0 / 255.0, g: 230.0 / 255.0, b: 160.0 / 255.0, a: 0.9 };
        let mid = f32::from(b.origin.y) + bh / 2.0;
        // Clip-local time → source time, as playback reads it.
        let src = |lt: f64| -> f64 {
            match c.kind {
                AssetType::Audio => c.in_ + lt * c.speed(),
                _ => c.in_ + c.src_offset(lt),
            }
        };
        let mut x = x0;
        while x < x1 {
            let lt = x as f64 / zoom;
            let lt2 = (x + 1.0) as f64 / zoom;
            let g = (c.volume_at(lt) * clip_fade(&c, c.start + lt)).clamp(0.0, 1.0) as f32;
            let v = wave.max_between(src(lt), src(lt2)) * g;
            // A gentle curve so quiet passages still read.
            let hgt = (v.powf(0.7) * bh * 0.94).max(if g > 0.0 { 1.0 } else { 0.0 });
            if hgt > 0.0 {
                window.paint_quad(fill(Bounds::new(point(px(bx + x), px(mid - hgt / 2.0)), gpui::size(px(1.), px(hgt))), color));
            }
            x += 1.0;
        }
    })
}

/// The little diamond that marks a keyframe (square-cornered when it holds).
fn diamond(color: Hsla, hold: bool) -> impl IntoElement {
    canvas(|_, _, _| {}, move |b: Bounds<Pixels>, _, window, _| {
        let (o, s) = (b.origin, f32::from(b.size.width));
        let (cx, cy, r) = (o.x + px(s / 2.0), o.y + px(s / 2.0), px(s / 2.0 - 1.0));
        let mut pb = PathBuilder::fill();
        if hold {
            let r = r * 0.8;
            pb.move_to(point(cx - r, cy - r));
            pb.line_to(point(cx + r, cy - r));
            pb.line_to(point(cx + r, cy + r));
            pb.line_to(point(cx - r, cy + r));
        } else {
            pb.move_to(point(cx, cy - r));
            pb.line_to(point(cx + r, cy));
            pb.line_to(point(cx, cy + r));
            pb.line_to(point(cx - r, cy));
        }
        pb.close();
        if let Ok(p) = pb.build() {
            window.paint_path(p, black());
        }
        let mut pb = PathBuilder::fill();
        let r = r - px(1.5);
        if hold {
            let r = r * 0.8;
            pb.move_to(point(cx - r, cy - r));
            pb.line_to(point(cx + r, cy - r));
            pb.line_to(point(cx + r, cy + r));
            pb.line_to(point(cx - r, cy + r));
        } else {
            pb.move_to(point(cx, cy - r));
            pb.line_to(point(cx + r, cy));
            pb.line_to(point(cx, cy + r));
            pb.line_to(point(cx - r, cy));
        }
        pb.close();
        if let Ok(p) = pb.build() {
            window.paint_path(p, color);
        }
    })
    .size_full()
}

/// What the clip's name bar says about its speed: 2×, ◀ backwards, a ramp or a freeze.
fn speed_badge(c: &Clip) -> Option<String> {
    if c.frozen() {
        return Some("FREEZE".into());
    }
    let rev = c.kind != AssetType::Audio && c.motion.as_ref().map(|m| m.reverse).unwrap_or(false);
    let sp = if c.ramped() {
        Some("RAMP".to_string())
    } else {
        let v = c.speed();
        ((v - 1.0).abs() > 1e-3).then(|| format!("{}×", (v * 100.0).round() / 100.0))
    };
    match (rev, sp) {
        (false, None) => None,
        (true, None) => Some("◀ REV".into()),
        (true, Some(s)) => Some(format!("◀ {s}")),
        (false, Some(s)) => Some(s),
    }
}
