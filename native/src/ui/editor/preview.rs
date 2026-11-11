// The preview: the composited frame, direct manipulation on top of it (click to select, drag to
// move, corners to scale, the round handle to rotate), safe-area guides and the transport.

use super::geometry::{clip_box, corners, hit_test};
use super::{DropTarget, Editor};
use crate::model::time::{tc, FPS, H, W};
use crate::model::types::Clip;
use gpui_component::button::ButtonVariants;
use gpui_component::menu::{ContextMenuExt, DropdownMenu};
use crate::ui::theme::*;
use gpui::prelude::*;
use gpui::Focusable;
use gpui::{canvas, div, img, point, px, Bounds, Context, MouseButton, MouseDownEvent, MouseMoveEvent, ObjectFit, PathBuilder, Pixels, Point, Window};

impl Editor {
    pub(super) fn render_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Fit a 16:9 stage inside whatever space the panel has (measured last frame).
        let (mut pw0, mut ph0) = (0.0, 0.0);
        let (w, h) = match self.bounds("stage-wrap") {
            Some(b) => {
                pw0 = f32::from(b.size.width);
                ph0 = f32::from(b.size.height);
                let (pw, ph) = (f32::from(b.size.width), f32::from(b.size.height));
                let mut w = pw;
                // Fit the project's shape (16:9, 9:16, 1:1, 4:5) inside the space.
                let ar = W() / H();
                let mut h = w / ar;
                if h > ph {
                    h = ph;
                    w = h * ar;
                }
                (w.max(0.0) * self.pv_zoom, h.max(0.0) * self.pv_zoom)
            }
            None => (0.0, 0.0),
        };
        // Zoomed in, the picture can be panned up to its edges.
        let (mx, my) = (((w - pw0) / 2.0).max(0.0), ((h - ph0) / 2.0).max(0.0));
        self.pv_pan = (self.pv_pan.0.clamp(-mx, mx), self.pv_pan.1.clamp(-my, my));
        let (pan_x, pan_y) = self.pv_pan;
        if (w, h) != self.stage_size {
            self.stage_size = (w, h);
            window.request_animation_frame();
        }
        let over_stage = matches!(self.drop_target, Some(DropTarget::Stage));
        let playing = self.store.playing;
        let looping = self.store.looping;
        let guides = self.store.guides;
        let (now, end) = (self.store.time, self.store.end());

        // Absolutely placed, so the stage never props its panel open: the space it fits into is
        // measured, never derived from the stage itself.
        let stage = div()
            .absolute()
            .left(px((pw0 - w) / 2.0 + pan_x))
            .top(px((ph0 - h) / 2.0 + pan_y))
            .w(px(w))
            .h(px(h))
            .overflow_hidden()
            .rounded(px(6.))
            .bg(black())
            .shadow_lg()
            .child(self.track_bounds("stage"))
            .when_some(self.frame.clone(), |d, f| d.child(img(f).absolute().size_full().rounded(px(6.)).object_fit(ObjectFit::Fill)))
            .child(self.render_stage_overlay(w, cx))
            .when(guides, |d| {
                d.child(div().absolute().top(px(h * 0.05)).left(px(w * 0.05)).w(px(w * 0.9)).h(px(h * 0.9)).border_1().border_dashed().border_color(white_a(0.25)))
                    .child(div().absolute().top(px(h * 0.1)).left(px(w * 0.1)).w(px(w * 0.8)).h(px(h * 0.8)).border_1().border_dashed().border_color(alpha(signal(), 0.4)))
            })
            // The stage's edge, drawn over the picture: as a border on the stage itself it would
            // shrink the picture and overlay inside it by a pixel a side.
            .child(div().absolute().top_0().left_0().size_full().rounded(px(6.)).border_1().border_color(if over_stage { signal() } else { white_a(0.1) }));

        // A narrow preview (e.g. with the AI sidebar open) shows just the current time and drops
        // the frame-step buttons (← and → still step), so nothing collides.
        let compact = pw0 > 0.0 && pw0 < 600.0;
        let transport = div()
            .flex()
            .items_center()
            .gap_2()
            .border_t_1()
            .border_color(hairline_soft())
            .px_4()
            .py(px(10.))
            .child(
                // Left and right take equal shares, so the play controls sit at the true centre.
                div()
                    .flex()
                    .flex_1()
                    .flex_basis(px(0.))
                    .min_w_0()
                    .child(timecode(tc(now), if compact { 13. } else { 15. }).text_color(ink()))
                    .when(!compact, |d| d.child(timecode(" / ", 15.).text_color(ink_muted())).child(timecode(tc(end), 15.).text_color(ink_muted()))),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .gap(px(6.))
                    .child(icon_button("go-start", "skip-back", "Go to start", Some("Home"), 32., Variant::Ghost, cx.listener(|this, _, _, cx| {
                        this.pause();
                        this.seek(0.0);
                        cx.notify();
                    })))
                    .when(!compact, |d| {
                        d.child(icon_button("prev-frame", "chevron-left", "Previous frame", Some("←"), 32., Variant::Ghost, cx.listener(|this, _, _, cx| {
                            this.step(-1);
                            cx.notify();
                        })))
                    })
                    .child(
                        button("play", Variant::Primary, 44.)
                            .w(px(44.))
                            .px_0()
                            .child(icon(if playing { "pause-fill" } else { "play-fill" }, 18., black()))
                            .tooltip(move |w, cx| gpui_component::tooltip::Tooltip::new(if playing { "Pause  Space" } else { "Play  Space" }).build(w, cx))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.toggle_play();
                                cx.notify();
                            })),
                    )
                    .when(!compact, |d| {
                        d.child(icon_button("next-frame", "chevron-right", "Next frame", Some("→"), 32., Variant::Ghost, cx.listener(|this, _, _, cx| {
                            this.step(1);
                            cx.notify();
                        })))
                    })
                    .child(icon_button("go-end", "skip-forward", "Go to end", Some("End"), 32., Variant::Ghost, cx.listener(|this, _, _, cx| {
                        this.pause();
                        let e = this.store.end();
                        this.seek(e - 1.0 / FPS());
                        cx.notify();
                    }))),
            )
            .child(div().flex().flex_1().flex_basis(px(0.)).min_w_0().justify_end().gap_2().child(icon_button("loop", "repeat", "Loop playback", Some("L"), 32., Variant::Ghost, cx.listener(|this, _, _, cx| {
                this.store.looping = !this.store.looping;
                cx.notify();
            })).when(looping, |d| d.bg(surface3()).text_color(ink())))
            .child(icon_button("guides", "scan", "Safe-area guides", Some("G"), 32., Variant::Ghost, cx.listener(|this, _, _, cx| {
                this.store.guides = !this.store.guides;
                cx.notify();
            })).when(guides, |d| d.bg(surface3())))
            .when(!compact, |d| {
                let ed = cx.entity();
                let z = self.pv_zoom;
                d.child(
                    gpui_component::button::Button::new("pv-zoom")
                        .label(if z <= 1.0 { "Fit".to_string() } else { format!("{}×", (z * 10.0).round() / 10.0) })
                        .ghost()
                        .dropdown_caret(true)
                        .h(px(32.))
                        .text_size(px(12.))
                        .tooltip("Zoom the picture: ⌘= ⌘− ⌘0, or ⌘-scroll over it")
                        .dropdown_menu(move |mut menu, _, _| {
                            for (v, l) in [(1.0, "Fit"), (2.0, "2×"), (4.0, "4×"), (8.0, "8×")] {
                                let ed = ed.clone();
                                menu = menu.item(gpui_component::menu::PopupMenuItem::new(l).checked((z - v).abs() < 0.01).on_click(move |_, _, cx| {
                                    ed.update(cx, |e, cx| {
                                        e.set_preview_zoom(v, None);
                                        cx.notify();
                                    })
                                }));
                            }
                            menu
                        }),
                )
            })
            .child({
                let ed = cx.entity();
                let on = self.scope;
                icon_button("scopes", "activity", "Scopes", None, 32., Variant::Ghost, |_, _, _| {})
                    .when(on.is_some(), |d| d.bg(surface3()))
                    .on_click(move |_, _, cx| {
                        ed.update(cx, |e, cx| {
                            e.scope = if e.scope.is_some() { None } else { Some(crate::render::scopes::Scope::Waveform) };
                            e.scope_img = None;
                            e.last_job = None;
                            cx.notify();
                        })
                    })
            })
            .child(icon_button("cinema", "maximize", if self.cinema { "Leave full screen" } else { "Full-screen preview" }, Some("F"), 32., Variant::Ghost, cx.listener(|this, _, window, cx| this.toggle_cinema(window, cx)))));

        panel()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .id("stage-wrap")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .m_4()
                    .overflow_hidden()
                    .child(self.track_bounds("stage-wrap"))
                    .child(stage)
                    .when_some(self.scope_panel(cx), |d, p| d.child(p))
                    .on_scroll(cx.listener(|this, ev: &gpui::ScrollEvent, _, cx| {
                        let d = ev.delta.pixel_delta(px(20.));
                        if ev.modifiers.platform || ev.modifiers.control {
                            // ⌘-scroll zooms about the pointer.
                            let anchor = this.bounds("stage-wrap").map(|b| (f32::from(ev.position.x - b.origin.x), f32::from(ev.position.y - b.origin.y)));
                            let f = (f32::from(d.y) * 0.01).exp();
                            this.set_preview_zoom(this.pv_zoom * f, anchor);
                        } else if this.pv_zoom > 1.0 {
                            this.pv_pan.0 += f32::from(d.x);
                            this.pv_pan.1 += f32::from(d.y);
                        } else {
                            return;
                        }
                        cx.stop_propagation();
                        cx.notify();
                    })),
            )
            .child(transport)
    }

    /// The scope over the picture's corner, with its kind switch.
    fn scope_panel(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        use crate::render::scopes::{SCOPES, H as SH, W as SW};
        let kind = self.scope?;
        let ed = cx.entity();
        Some(
            div()
                .id("scope")
                .absolute()
                .right(px(8.))
                .bottom(px(8.))
                .w(px(SW as f32 + 16.))
                .flex()
                .flex_col()
                .gap_2()
                .p_2()
                .rounded(px(10.))
                .bg(black_overlay(0.82))
                .border_1()
                .border_color(white_a(0.1))
                .occlude()
                .child(segmented("scope-kind", &SCOPES.map(|(k, l)| (k, l)), &kind, |_| false, move |v, _, cx| {
                    ed.update(cx, |e, cx| {
                        e.scope = Some(v);
                        e.last_job = None;
                        cx.notify();
                    })
                }))
                .child(div().relative().w(px(SW as f32)).h(px(SH as f32)).bg(black()).rounded(px(4.)).when_some(self.scope_img.clone(), |d, i| d.child(img(i).absolute().size_full()))),
        )
    }

    fn render_stage_overlay(&mut self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let k = width / W();
        let s = &self.store;
        // Boxes follow keyframes: each clip as it stands at the playhead.
        let on_screen = |c: &Clip| c.visual() && !s.track(&c.track).hidden && s.time >= c.start && s.time < c.start + c.dur;
        let sel = s.selected_clip().map(|c| c.posed(s.time));
        let visible = sel.as_ref().filter(|c| on_screen(c)).cloned();
        let bx = visible.as_ref().map(clip_box);
        let hover_box = self.hover_clip.as_ref().filter(|h| Some(*h) != sel.as_ref().map(|c| &c.id)).and_then(|h| s.clip(h)).map(|c| clip_box(&c.posed(s.time)));
        // The rest of a multiple selection gets a plain outline.
        let others: Vec<super::geometry::BoxR> = s.multi.iter().filter_map(|id| s.clip(id)).filter(|c| on_screen(c)).map(|c| clip_box(&c.posed(s.time))).collect();
        let guides = self.stage_guides.clone();
        let readout = self.readout.clone();

        // Edges that land on the stage's edge are drawn just inside it, so a full-frame clip shows
        // all four sides evenly instead of losing the right and bottom ones to the clip.
        let (sw, sh) = (W() * k, H() * k);
        // clamp() panics when the stage hasn't been measured yet (lo > hi); this can't.
        let inside = |v: f32, lo: f32, hi: f32| v.min(hi).max(lo);
        let outline = move |b: super::geometry::BoxR, color: gpui::Hsla, width: f32| {
            canvas(|_, _, _| {}, move |bounds: Bounds<Pixels>, _, window, _| {
                let o = bounds.origin;
                let pts = corners(&b);
                let mut pb = PathBuilder::stroke(px(width));
                let half = width / 2.0;
                let at = |(x, y): (f32, f32)| point(o.x + px(inside(x * k, half, sw - half)), o.y + px(inside(y * k, half, sh - half)));
                pb.move_to(at(pts[0]));
                pb.line_to(at(pts[1]));
                pb.line_to(at(pts[2]));
                pb.line_to(at(pts[3]));
                pb.line_to(at(pts[0]));
                if let Ok(p) = pb.build() {
                    window.paint_path(p, color);
                }
            })
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        };

        let mut layer = div()
            .id("stage-overlay")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .when(self.hover_clip.is_some() || bx.is_some(), |d| d.cursor_move())
            .on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, window, cx| this.stage_mouse_down(ev, window, cx)))
            // Right-click a clip on the picture: its menu (the clip becomes the selection first).
            .on_mouse_down(MouseButton::Right, cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                let posed = this.posed_clips();
                let hit = this.stage_point(ev.position).and_then(|(x, y)| hit_test(&posed, &this.store.tracks, x, y, this.store.time)).map(|c| c.id.clone());
                match hit {
                    Some(id) if !this.store.is_selected(&id) => this.store.select(Some(id)),
                    Some(id) => this.store.focus_in_selection(&id),
                    None => this.store.select(None),
                }
                cx.notify();
            }))
            // A quick click releases before the window-wide drag listeners are drawn; end it here.
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| this.drag_end(cx)))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if this.drag.is_some() {
                    return;
                }
                let posed = this.posed_clips();
                let hit = this.stage_point(ev.position).and_then(|(x, y)| hit_test(&posed, &this.store.tracks, x, y, this.store.time)).map(|c| c.id.clone());
                if hit != this.hover_clip {
                    this.hover_clip = hit;
                    cx.notify();
                }
            }))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !hovered && this.hover_clip.is_some() {
                    this.hover_clip = None;
                    cx.notify();
                }
            }));
        for (vertical, at) in guides {
            layer = layer.child(if vertical {
                div().absolute().top_0().bottom_0().left(px(at * k)).w(px(1.)).bg(signal())
            } else {
                div().absolute().left_0().right_0().top(px(at * k)).h(px(1.)).bg(signal())
            });
        }
        if let Some(hb) = hover_box {
            layer = layer.child(outline(hb, white_a(0.45), 1.0));
        }
        for ob in others {
            layer = layer.child(outline(ob, alpha(signal(), 0.7), 1.0));
        }
        // The selected clip's mask, as its shape on the picture.
        if let Some((c, m)) = visible.as_ref().and_then(|c| c.mask.clone().map(|m| (c.clone(), m))) {
            let n = if m.shape == "rect" { 4 } else { 64 };
            let (a, (hw, hh)) = ((m.rot as f32).to_radians(), (m.w as f32 / 2.0, m.h as f32 / 2.0));
            let pts: Vec<(f32, f32)> = (0..n)
                .map(|i| {
                    let (lx, ly) = if m.shape == "rect" {
                        [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)][i]
                    } else {
                        let t = i as f32 / n as f32 * std::f32::consts::TAU;
                        (t.cos() * hw, t.sin() * hh)
                    };
                    let (rx, ry) = (lx * a.cos() - ly * a.sin(), lx * a.sin() + ly * a.cos());
                    let p = super::geometry::to_canvas(&c, W() / 2.0 + m.x as f32 + rx, H() / 2.0 + m.y as f32 + ry);
                    (p.0 * k, p.1 * k)
                })
                .collect();
            layer = layer.child(
                canvas(|_, _, _| {}, move |bounds: Bounds<Pixels>, _, window, _| {
                    let o = bounds.origin;
                    let mut pb = PathBuilder::stroke(px(1.));
                    pb.move_to(point(o.x + px(pts[0].0), o.y + px(pts[0].1)));
                    for p in pts.iter().skip(1).chain(std::iter::once(&pts[0])) {
                        pb.line_to(point(o.x + px(p.0), o.y + px(p.1)));
                    }
                    if let Ok(p) = pb.build() {
                        window.paint_path(p, hex("#ffd166"));
                    }
                })
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );
        }
        if let Some(b) = bx {
            layer = layer.child(outline(b, signal(), 1.5));
            let pts = corners(&b);
            for (i, p) in pts.iter().take(4).enumerate() {
                layer = layer.child(
                    div()
                        .id(("scale-handle", i))
                        .absolute()
                        .left(px(inside(p.0 * k, 5., sw - 5.) - 5.))
                        .top(px(inside(p.1 * k, 5., sh - 5.) - 5.))
                        .size(px(10.))
                        .rounded(px(2.))
                        .border_1()
                        .border_color(signal())
                        .bg(ink())
                        .cursor(if i % 2 == 0 { gpui::CursorStyle::ResizeUpLeftDownRight } else { gpui::CursorStyle::ResizeUpRightDownLeft })
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.start_stage_gesture("scale", ev.position, cx);
                        })),
                );
            }
            // The rotate handle sits 30px above the top edge, along the box's own "up".
            let a = b.rot.to_radians();
            let top = pts[4];
            let (ux, uy) = (a.sin(), -a.cos());
            let knob = (top.0 * k + ux * 30.0, top.1 * k + uy * 30.0);
            let stem_from = (top.0 * k, top.1 * k);
            layer = layer
                .child(
                    canvas(|_, _, _| {}, move |bounds: Bounds<Pixels>, _, window, _| {
                        let o = bounds.origin;
                        let mut pb = PathBuilder::stroke(px(1.));
                        pb.move_to(point(o.x + px(stem_from.0), o.y + px(stem_from.1)));
                        pb.line_to(point(o.x + px(knob.0), o.y + px(knob.1)));
                        if let Ok(p) = pb.build() {
                            window.paint_path(p, signal());
                        }
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .child(
                    div()
                        .id("rotate-handle")
                        .absolute()
                        .left(px(knob.0 - 6.))
                        .top(px(knob.1 - 6.))
                        .size(px(12.))
                        .rounded_full()
                        .border_1()
                        .border_color(signal())
                        .bg(ink())
                        .cursor_grab()
                        .tooltip(|w, cx| gpui_component::tooltip::Tooltip::new("Drag to rotate. Hold Shift to snap to 15°.").build(w, cx))
                        .on_mouse_down(MouseButton::Left, cx.listener(|this, ev: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.start_stage_gesture("rotate", ev.position, cx);
                        })),
                );
            if let Some(r) = readout {
                let y = (b.cy * k + b.h * k / 2.0 + 10.0).min(H() * k - 22.0);
                layer = layer.child(
                    type_timecode(div(), 10.)
                        .absolute()
                        .left(px(b.cx * k - 40.))
                        .top(px(y))
                        .w(px(80.))
                        .flex()
                        .justify_center()
                        .child(div().rounded_full().bg(black_overlay(0.75)).px_2().py(px(2.)).text_color(ink()).child(r)),
                );
            }
        }
        let ed = cx.entity();
        layer.context_menu(move |menu, window, cx| {
            if ed.read(cx).store.selected.is_none() {
                return menu;
            }
            super::menus::clip_menu(menu.min_w(px(220.)), &ed, window, cx)
        })
    }

    fn stage_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.stage_point(ev.position) else { return };
        let t = self.store.time;
        // The topmost clip under the pointer, as you see it: clicking a title over a selected
        // background picks the title.
        let posed = self.posed_clips();
        let hit = hit_test(&posed, &self.store.tracks, p.0, p.1, t).cloned();
        let Some(hit) = hit else {
            self.store.select(None);
            cx.notify();
            return;
        };
        // Shift- or ⌘-click adds to the selection, as on the timeline.
        if ev.modifiers.shift || ev.modifiers.platform {
            self.store.toggle_select(&hit.id);
            cx.notify();
            return;
        }
        if self.store.selected.as_deref() != Some(&hit.id) {
            self.store.select(Some(hit.id.clone()));
        }
        if ev.click_count >= 2 && hit.text.is_some() {
            // Edit the words: focus the inspector's text box.
            let h = self.text_input.read(cx).focus_handle(cx);
            window.focus(&h, cx);
            cx.notify();
            return;
        }
        self.start_stage_gesture("move", ev.position, cx);
        let _: Point<Pixels> = ev.position;
    }
}
