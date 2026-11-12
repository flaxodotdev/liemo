// Two dialogs for the person, beside the AI's tools of the same names: the design check (what's
// wrong on screen, each issue a click away) and stock search (Pexels photos and video onto the
// timeline).

use super::Editor;
use crate::check::Issue;
use crate::integrations::{self, Service};
use crate::model::time::tc;
use crate::ui::theme::*;
use gpui::prelude::*;
use gpui::{div, img, px, relative, AppContext, Context, Entity, MouseButton, ObjectFit, Window};
use gpui_component::input::{Input, InputEvent, InputState};
use serde_json::Value;
use std::path::PathBuf;

pub struct CheckUi {
    running: bool,
    issues: Vec<Issue>,
}

pub struct StockUi {
    input: Entity<InputState>,
    video: bool,
    busy: bool,
    error: Option<String>,
    /// Each result with its downloaded preview, once it's here.
    results: Vec<(Value, Option<PathBuf>)>,
    /// Results being downloaded for the timeline.
    fetching: Vec<String>,
}

fn dialog(id: &'static str, width: f32) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w(px(width))
        .max_h(relative(0.88))
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
}

fn backdrop(id: &'static str) -> gpui::Stateful<gpui::Div> {
    div().id(id).absolute().top_0().left_0().size_full().bg(black_overlay(0.6)).occlude().flex().items_center().justify_center()
}

impl StockUi {
    pub fn input_entity(&self) -> Entity<InputState> {
        self.input.clone()
    }
    pub fn focused(&self, window: &Window, cx: &gpui::App) -> bool {
        use gpui::Focusable;
        self.input.read(cx).focus_handle(cx).contains_focused(window, cx)
    }
}

impl Editor {
    // ---------- Design check ----------
    pub fn run_design_check(&mut self, cx: &mut Context<Self>) {
        self.check = Some(CheckUi { running: true, issues: vec![] });
        let rec = self.record();
        cx.spawn(async move |this, cx| {
            let issues = cx.background_spawn(async move { crate::check::check_record(&rec) }).await;
            let _ = this.update(cx, |this, cx| {
                if let Some(c) = this.check.as_mut() {
                    c.running = false;
                    c.issues = issues;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_check(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = self.check.as_ref().unwrap();
        let mut list = div().id("check-list").flex().flex_col().gap_1().overflow_y_scroll().max_h(px(420.));
        for (i, is) in c.issues.iter().enumerate() {
            let (t, clip) = (is.time, is.clip.clone());
            list = list.child(
                div()
                    .id(("check-issue", i))
                    .flex()
                    .gap_3()
                    .px_3()
                    .py_2()
                    .rounded(px(10.))
                    .cursor_pointer()
                    .hover(|d| d.bg(surface2()))
                    .child(timecode(tc(t), 12.).text_color(ink_muted()).flex_none().w(px(84.)))
                    .child(div().flex().flex_col().gap(px(2.)).flex_1().min_w_0().child(label(is.kind.replace('-', " "))).child(div().text_size(px(13.)).line_height(px(18.)).child(is.message.clone())))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pause();
                        this.seek(t);
                        if let Some(id) = &clip {
                            this.store.select(Some(id.clone()));
                        }
                        cx.notify();
                    })),
            );
        }
        let n = c.issues.len();
        let summary = if c.running {
            "Checking every text, frame and sound…".to_string()
        } else if n == 0 {
            "No problems found: text is readable, nothing's off-frame, no gaps, and the sound doesn't clip.".to_string()
        } else {
            format!("{n} thing{} to look at. Click one to go there.", if n == 1 { "" } else { "s" })
        };
        let can_fix = !c.running && n > 0 && !crate::agent::installed().is_empty();
        backdrop("check-backdrop")
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.check = None;
                cx.notify();
            }))
            .child(
                dialog("check-dialog", 560.)
                    .child(div().flex().flex_col().gap_1().child(type_headline(div()).child("Design check")).child(div().text_size(px(13.)).text_color(ink_muted()).child(summary)))
                    .when(n > 0, |d| d.child(list))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(button("check-again", Variant::Ghost, 36.).child("Check again").on_click(cx.listener(|this, _, _, cx| this.run_design_check(cx))))
                            .child(div().flex_1())
                            .when(can_fix, |d| {
                                d.child(button("check-fix", Variant::Secondary, 36.).child("Ask the AI to fix them").on_click(cx.listener(|this, _, window, cx| {
                                    let Some(c) = this.check.take() else { return };
                                    let list: Vec<String> = c.issues.iter().map(|i| format!("- {} {}", tc(i.time), i.message)).collect();
                                    this.agent.open = true;
                                    this.agent_send(format!("The design check found these. Fix each one, then run design_check again:\n{}", list.join("\n")), window, cx);
                                })))
                            })
                            .child(button("check-done", Variant::Primary, 36.).child("Done").on_click(cx.listener(|this, _, _, cx| {
                                this.check = None;
                                cx.notify();
                            }))),
                    ),
            )
    }

    // ---------- Stock ----------
    pub fn open_stock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !integrations::connected(Service::Pexels) {
            self.open_connect(Service::Pexels, window, cx);
            self.store.toast("Connect Pexels to search stock photos and video");
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search Pexels: ocean at dusk, city timelapse, hands typing…"));
        input.update(cx, |i, cx| i.focus(window, cx));
        self._subs.push(cx.subscribe_in(&input, window, |this: &mut Editor, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.stock_search(cx);
            }
        }));
        self.stock = Some(StockUi { input, video: false, busy: false, error: None, results: vec![], fetching: vec![] });
        cx.notify();
    }

    pub fn stock_search_now(&mut self, cx: &mut Context<Self>) {
        self.stock_search(cx);
    }

    fn stock_search(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.stock.as_mut() else { return };
        let q = s.input.read(cx).value().trim().to_string();
        if q.is_empty() {
            return;
        }
        s.busy = true;
        s.error = None;
        s.results.clear();
        let video = s.video;
        cx.spawn(async move |this, cx| {
            let found = cx.background_spawn(async move { integrations::search_stock(&q, video, 24, None) }).await;
            let list = match found {
                Ok(l) => l,
                Err(e) => {
                    let _ = this.update(cx, |this, cx| {
                        if let Some(s) = this.stock.as_mut() {
                            s.busy = false;
                            s.error = Some(e.to_string());
                        }
                        cx.notify();
                    });
                    return;
                }
            };
            let _ = this.update(cx, |this, cx| {
                if let Some(s) = this.stock.as_mut() {
                    s.busy = false;
                    s.results = list.iter().cloned().map(|v| (v, None)).collect();
                }
                cx.notify();
            });
            // Previews, one by one, into a cache folder.
            for (i, v) in list.into_iter().enumerate() {
                let p = cx.background_spawn(async move { integrations::preview(&v) }).await;
                let ok = this.update(cx, |this, cx| {
                    if let Some((_, slot)) = this.stock.as_mut().and_then(|s| s.results.get_mut(i)) {
                        *slot = p;
                    }
                    cx.notify();
                });
                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// Download a result and put it on the timeline at the playhead.
    fn stock_add(&mut self, v: Value, cx: &mut Context<Self>) {
        let (Some(url), id) = (v["url"].as_str().map(String::from), v["id"].to_string()) else { return };
        if let Some(s) = self.stock.as_mut() {
            s.fetching.push(id.clone());
        }
        let name = v["alt"].as_str().filter(|a| !a.is_empty()).map(|a| a.chars().take(40).collect::<String>()).unwrap_or_else(|| format!("Pexels {id}"));
        let by = v["by"].as_str().unwrap_or("").to_string();
        cx.spawn(async move |this, cx| {
            let got = cx.background_spawn(async move { integrations::download_stock(&url, &name) }).await;
            let _ = this.update(cx, |this, cx| {
                if let Some(s) = this.stock.as_mut() {
                    s.fetching.retain(|f| *f != id);
                }
                match got {
                    Ok(p) => {
                        let t = this.store.time;
                        this.import(vec![p], Some((t, None)), cx);
                        if !by.is_empty() {
                            this.store.toast(format!("Photo by {by} on Pexels"));
                        }
                    }
                    Err(e) => this.store.toast(format!("Couldn't download it: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_stock(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.stock.as_ref().unwrap();
        let ed = cx.entity();
        let kind = segmented("stock-kind", &[(false, "Photos"), (true, "Videos")], &s.video, |_| false, move |v, _, cx| {
            ed.update(cx, |e, cx| {
                if let Some(s) = e.stock.as_mut() {
                    s.video = v;
                }
                e.stock_search(cx);
            })
        });
        let mut grid = div().flex().flex_wrap().gap_2();
        for (i, (v, preview)) in s.results.iter().enumerate() {
            let id = v["id"].to_string();
            let fetching = s.fetching.contains(&id);
            let by = v["by"].as_str().unwrap_or("").to_string();
            let secs = v["seconds"].as_f64();
            let v2 = v.clone();
            grid = grid.child(
                div()
                    .id(("stock", i))
                    .relative()
                    .w(px(168.))
                    .h(px(108.))
                    .rounded(px(8.))
                    .overflow_hidden()
                    .bg(surface2())
                    .cursor_pointer()
                    .when_some(preview.clone(), |d, p| d.child(img(p).absolute().size_full().object_fit(ObjectFit::Cover)))
                    .child(div().absolute().bottom_0().left_0().right_0().px_2().py_1().bg(black_overlay(0.55)).text_size(px(11.)).truncate().child(match secs {
                        Some(sec) => format!("{sec:.0} s · {by}"),
                        None => by,
                    }))
                    .when(fetching, |d| d.child(div().absolute().size_full().bg(black_overlay(0.5)).flex().items_center().justify_center().text_size(px(12.)).child("Adding…")))
                    .tooltip(|w, cx| gpui_component::tooltip::Tooltip::new("Add at the playhead").build(w, cx))
                    .on_click(cx.listener(move |this, _, _, cx| this.stock_add(v2.clone(), cx))),
            );
        }
        let status = if s.busy {
            Some("Searching…".to_string())
        } else if let Some(e) = &s.error {
            Some(e.clone())
        } else if s.results.is_empty() {
            Some("Type what you're after and press Return. Click a result to put it at the playhead.".to_string())
        } else {
            None
        };
        backdrop("stock-backdrop")
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.stock = None;
                cx.notify();
            }))
            .child(
                dialog("stock-dialog", 740.)
                    .child(div().flex().items_center().gap_3().child(type_headline(div()).flex_1().child("Stock")).child(kind))
                    .child(div().relative().child(self.track_bounds("input:stock")).child(Input::new(&s.input)))
                    .when_some(status, |d, st| d.child(div().text_size(px(13.)).text_color(ink_muted()).child(st)))
                    .child(div().id("stock-scroll").overflow_y_scroll().max_h(px(460.)).child(grid))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(div().flex_1().text_size(px(12.)).text_color(ink_muted()).child("Photos and videos provided by Pexels, free to use."))
                            .child(button("stock-done", Variant::Primary, 36.).child("Done").on_click(cx.listener(|this, _, _, cx| {
                                this.stock = None;
                                cx.notify();
                            }))),
                    ),
            )
    }
}
