// The projects page: start something new, or open, rename, duplicate or delete a saved project.

use super::app::RaynaApp;
use super::images::{cached, put, to_image};
use super::prefs;
use super::theme::*;
use crate::model::assets::asset;
use crate::model::time::tc;
use crate::storage::{self, ProjectSummary};
use gpui::prelude::*;
use gpui::Focusable;
use gpui::{div, img, px, AnyElement, Context, Entity, FontWeight, MouseButton, ObjectFit, RenderImage, SharedString, Subscription, WeakEntity, Window};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::color_picker::ColorPickerState;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::{Sizable, WindowExt};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
enum Sort {
    Edited,
    Created,
    Name,
}

pub struct Home {
    app: WeakEntity<RaynaApp>,
    projects: Option<Vec<ProjectSummary>>,
    failed: bool,
    search: Entity<InputState>,
    query: String,
    sort: Sort,
    busy: Option<&'static str>,
    renaming: Option<ProjectSummary>,
    rename_input: Entity<InputState>,
    deleting: Option<ProjectSummary>,
    /// Format for new projects.
    format: &'static str,
    /// The New project dialog, open with a starting point picked.
    new_dialog: Option<&'static str>,
    new_cat: Option<crate::model::format::Category>,
    new_name: Entity<InputState>,
    /// The Connect AI dialog.
    connect_ai: bool,
    /// The update dialog, and which menu-initiated check it last answered.
    update_open: bool,
    update_asked: u64,
    new_fps: u32,
    new_res: u32,
    new_bg: String,
    custom_w: Entity<InputState>,
    custom_h: Entity<InputState>,
    /// Custom-colour pickers (per colour row), cached across renders.
    pickers: HashMap<String, (Entity<ColorPickerState>, Subscription)>,
    _subs: Vec<Subscription>,
}

/// Create a project from a template and save it.
pub use crate::storage::{create_project, NewProject};

impl Home {
    pub fn new(app: WeakEntity<RaynaApp>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search projects"));
        let rename_input = cx.new(|cx| InputState::new(window, cx));
        let subs = vec![
            cx.subscribe_in(&search, window, |this: &mut Home, input, ev: &InputEvent, _, cx| {
                if let InputEvent::Change = ev {
                    this.query = input.read(cx).value().to_string();
                    cx.notify();
                }
            }),
            cx.subscribe_in(&rename_input, window, |this: &mut Home, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.save_rename(window, cx);
                }
            }),
        ];
        let sort = match prefs::get_str("sort").as_deref() {
            Some("created") => Sort::Created,
            Some("name") => Sort::Name,
            _ => Sort::Edited,
        };
        let format = crate::model::format::format(&prefs::get_str("format").unwrap_or_default()).id;
        let new_name = cx.new(|cx| InputState::new(window, cx).placeholder("Untitled project"));
        let custom_w = cx.new(|cx| InputState::new(window, cx).default_value("1080"));
        let custom_h = cx.new(|cx| InputState::new(window, cx).default_value("1920"));
        let format = if prefs::get_str("format").as_deref() == Some("custom") { "custom" } else { format };
        let mut h = Home { format, new_dialog: None, new_cat: None, new_name, connect_ai: std::env::var("RAYNA_AI").is_ok(), update_open: false, update_asked: crate::update::asked(), new_fps: 30, new_res: 1080, new_bg: "#000000".into(), custom_w, custom_h, pickers: HashMap::new(), app, projects: None, failed: false, search, query: String::new(), sort, busy: None, renaming: None, rename_input, deleting: None, _subs: subs };
        h.refresh();
        // Follow the updater: redraw when its state moves, and open the dialog when the user
        // asked from the menu (they see the answer even if there's nothing new).
        cx.spawn(async move |this, cx| {
            let mut seen = 0;
            loop {
                cx.background_executor().timer(std::time::Duration::from_millis(300)).await;
                let g = crate::update::generation();
                if g == seen {
                    continue;
                }
                seen = g;
                let ok = this.update(cx, |h, cx| {
                    let asked = crate::update::asked();
                    if asked != h.update_asked && !matches!(crate::update::state(), crate::update::State::Checking) {
                        h.update_asked = asked;
                        h.update_open = true;
                    }
                    cx.notify();
                });
                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();
        // Test hook: LIEMO_UPDATE_DIALOG=1 opens the update dialog.
        if std::env::var("LIEMO_UPDATE_DIALOG").is_ok() {
            h.update_open = true;
        }
        // Test hook: RAYNA_NEW=1 opens the New project dialog.
        if std::env::var("RAYNA_NEW").is_ok() {
            h.new_dialog = Some("blank");
        }
        h
    }

    pub fn refresh(&mut self) {
        match storage::list_projects() {
            Ok(p) => {
                self.projects = Some(p);
                self.failed = false;
            }
            Err(_) => self.failed = true,
        }
    }

    fn open(&mut self, id: String, pending: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(app) = self.app.upgrade() {
            cx.defer_in(window, move |_, window, cx| app.update(cx, |a, cx| a.open_project(&id, pending, window, cx)));
        }
    }

    fn start(&mut self, sample: bool, files: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = Some(if !files.is_empty() { "video" } else if sample { "sample" } else { "blank" });
        let typed = self.new_name.read(cx).value().trim().to_string();
        let name = if !typed.is_empty() { Some(typed) } else { files.first().and_then(|p| p.file_stem()).and_then(|s| s.to_str()).map(String::from) };
        match create_project(sample, name, &self.settings(cx)) {
            Ok(id) => self.open(id, files, window, cx),
            Err(_) => {
                window.push_notification("Couldn't create a project. The projects folder isn't writable.", cx);
                self.busy = None;
            }
        }
        cx.notify();
    }

    fn pick_and_start(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("Start".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| this.start(false, paths, window, cx));
            }
        })
        .detach();
    }

    fn save_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.renaming.take() else { return };
        let name = self.rename_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            self.renaming = Some(p);
            return;
        }
        let _ = storage::rename_project(&p.id, &name);
        self.refresh();
        window.push_notification("Renamed", cx);
        cx.notify();
    }

    fn thumb(p: &ProjectSummary) -> Option<Arc<RenderImage>> {
        let key = format!("project:{}", p.id);
        if let Some(i) = cached(&key) {
            return Some(i);
        }
        let pm = storage::load_image(p.thumb.as_ref()?)?;
        let i = to_image(&pm);
        put(&key, i.clone());
        Some(i)
    }

    fn scene_thumb(id: &str) -> Option<Arc<RenderImage>> {
        let key = format!("asset:{id}");
        if let Some(i) = cached(&key) {
            return Some(i);
        }
        let pm = crate::render::thumbs::asset_thumb(&*asset(id)?)?;
        let i = to_image(&pm);
        put(&key, i.clone());
        Some(i)
    }

    fn start_card(&self, id: &'static str, title: &str, body: &str, icon_name: &str, art: Option<AnyElement>, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.busy.is_some();
        let spinning = self.busy == Some(id);
        div()
            .id(id)
            .flex_1()
            .min_h(px(160.))
            .flex()
            .flex_col()
            .justify_between()
            .gap_6()
            .overflow_hidden()
            .rounded(px(20.))
            .bg(surface1())
            .p_5()
            .when(!busy, |d| d.cursor_pointer().hover(|s| s.bg(surface2())))
            .when(busy, |d| d.opacity(0.6))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child(div().size(px(40.)).flex_none().rounded_full().bg(surface3()).flex().items_center().justify_center().child(icon(if spinning { "loader-2" } else { icon_name }, 16., ink())))
                    .children(art),
            )
            .child(div().flex().flex_col().gap(px(6.)).child(type_headline(div()).child(title.to_string())).child(type_caption(div()).font_weight(FontWeight::NORMAL).text_color(ink_muted()).child(body.to_string())))
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.busy.is_some() {
                    return;
                }
                let _ = window;
                this.new_dialog = Some(id);
                cx.notify();
            }))
    }

    fn project_card(&self, p: &ProjectSummary, cx: &mut Context<Self>) -> impl IntoElement {
        let thumb = Self::thumb(p);
        let id = p.id.clone();
        let (id_open, id_menu) = (p.id.clone(), p.id.clone());
        let me = cx.entity();
        let summary = p.clone();
        let group_name = SharedString::from(format!("pc-{}", p.id));
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .id(SharedString::from(format!("open-{id}")))
                    .group(group_name.clone())
                    .relative()
                    .w_full()
                    .aspect_ratio(16. / 9.)
                    .overflow_hidden()
                    .rounded(px(15.))
                    .bg(surface1())
                    .cursor_pointer()
                    .border_1()
                    .border_color(alpha(ink(), 0.0))
                    .hover(|s| s.border_color(hairline()))
                    .child(match thumb {
                        Some(t) => img(t).absolute().size_full().rounded(px(14.)).object_fit(ObjectFit::Cover).into_any_element(),
                        None => div().absolute().size_full().flex().items_center().justify_center().child(icon("clapperboard", 24., ink_muted())).into_any_element(),
                    })
                    .child(type_timecode(div(), 11.).absolute().right(px(8.)).bottom(px(8.)).rounded(px(4.)).bg(black_overlay(0.65)).px(px(6.)).py(px(2.)).child(tc(p.duration)))
                    .when(p.format != "youtube", |d| {
                        d.child(type_label(div()).absolute().left(px(8.)).bottom(px(8.)).rounded(px(4.)).bg(black_overlay(0.65)).px(px(6.)).py(px(2.)).child(format!("{} · {}", p.label, p.aspect)))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| this.open(id_open.clone(), vec![], window, cx))),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .flex_1()
                            .min_w_0()
                            .child(type_body_sm(div()).truncate().child(p.name.clone()))
                            .child(micro(format!("Edited {} · {} {}", storage::relative_time(p.updated_at), p.clip_count, if p.clip_count == 1 { "clip" } else { "clips" }))),
                    )
                    .child(Button::new(SharedString::from(format!("more-{id_menu}"))).ghost().small().icon(gpui_component::Icon::empty().path("rayna/icons/more-horizontal.svg")).dropdown_menu_with_anchor(gpui::Anchor::TopRight, move |menu, _, _| {
                        let (m1, m2, m3, m4) = (me.clone(), me.clone(), me.clone(), me.clone());
                        let (s1, s2, s3, s4) = (summary.clone(), summary.clone(), summary.clone(), summary.clone());
                        menu.min_w(px(176.))
                            .item(PopupMenuItem::new("Open").on_click(move |_, window, cx| m1.update(cx, |this, cx| this.open(s1.id.clone(), vec![], window, cx))))
                            .item(PopupMenuItem::new("Rename").on_click(move |_, window, cx| {
                                m2.update(cx, |this, cx| {
                                    let n = s2.name.clone();
                                    this.rename_input.update(cx, |st, cx| st.set_value(n, window, cx));
                                    this.renaming = Some(s2.clone());
                                    let h = this.rename_input.read(cx).focus_handle(cx);
                                    window.focus(&h, cx);
                                    cx.notify();
                                })
                            }))
                            .item(PopupMenuItem::new("Duplicate").on_click(move |_, window, cx| {
                                m3.update(cx, |this, cx| {
                                    match storage::duplicate_project(&s3.id) {
                                        Ok(Some(_)) => window.push_notification(SharedString::from(format!("Duplicated {}", s3.name)), cx),
                                        _ => window.push_notification("Couldn't duplicate that project", cx),
                                    }
                                    this.refresh();
                                    cx.notify();
                                })
                            }))
                            .separator()
                            .item(PopupMenuItem::new("Delete").on_click(move |_, _, cx| {
                                m4.update(cx, |this, cx| {
                                    this.deleting = Some(s4.clone());
                                    cx.notify();
                                })
                            }))
                    })),
            )
    }

    /// The custom size typed in, if it's a usable one (64–8192 px, at most 4:1).
    fn custom_size(&self, cx: &gpui::App) -> Option<[u32; 2]> {
        let w: u32 = self.custom_w.read(cx).value().trim().parse().ok()?;
        let h: u32 = self.custom_h.read(cx).value().trim().parse().ok()?;
        let ok = (64..=8192).contains(&w) && (64..=8192).contains(&h) && (w.max(h) as f32 / w.min(h) as f32) <= 4.0;
        ok.then_some([w, h])
    }
    fn settings(&self, cx: &gpui::App) -> NewProject {
        NewProject {
            format: self.format.into(),
            size: if self.format == "custom" { self.custom_size(cx) } else { None },
            fps: self.new_fps,
            resolution: self.new_res,
            background: self.new_bg.clone().into(),
        }
    }

    /// A picture `w`×`h` units in shape, fitted inside a `bw`×`bh` box.
    fn fitted_wh(w: f32, h: f32, bw: f32, bh: f32) -> impl IntoElement {
        let k = (bw / w).min(bh / h);
        let (dw, dh) = (w * k, h * k);
        let key = format!("format:{w}x{h}:{}", (dh * 2.0).round());
        let image = cached(&key).unwrap_or_else(|| {
            let i = to_image(&crate::render::thumbs::format_preview(w, h, (dh * 2.0).round() as u32));
            put(&key, i.clone());
            i
        });
        div().w(px(bw)).h(px(bh)).flex().items_center().justify_center().child(img(image).w(px(dw)).h(px(dh)).rounded(px(6.)).object_fit(ObjectFit::Fill))
    }

    /// The "Custom size" tile, always first.
    fn custom_card(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.format == "custom";
        div()
            .id("fmt-custom")
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(15.))
            .border_1()
            .border_dashed()
            .border_color(if active { signal() } else { hairline() })
            .bg(if active { surface2() } else { surface1() })
            .p_2()
            .cursor_pointer()
            .hover(|s| s.bg(surface2()))
            .child(
                div()
                    .h(px(100.))
                    .rounded(px(10.))
                    .bg(canvas_bg())
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(div().size(px(36.)).rounded_full().bg(surface3()).flex().items_center().justify_center().child(icon("scan", 16., ink())))
                    .child(type_micro(div()).text_color(ink_muted()).child("Any width × height")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px_1()
                    .pb_1()
                    .child(type_caption(div()).child("Custom size"))
                    .child(type_micro(div()).text_size(px(11.)).text_color(ink_muted()).truncate().child("Your own canvas, in pixels")),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.format = "custom";
                prefs::set("format", serde_json::Value::String("custom".into()));
                cx.notify();
            }))
    }

    /// Frame rate, export resolution and background: what most editors ask when you start.
    fn settings_section(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let me = cx.entity();
        let me2 = me.clone();
        let fps = segmented("np-fps", &[(24u32, "24"), (25, "25"), (30, "30"), (50, "50"), (60, "60")], &self.new_fps, |_| false, move |v, _, cx| {
            me.update(cx, |this, cx| {
                this.new_fps = v;
                cx.notify();
            })
        });
        let res = segmented("np-res", &[(720u32, "720p"), (1080, "1080p"), (1440, "1440p"), (2160, "4K")], &self.new_res, |_| false, move |v, _, cx| {
            me2.update(cx, |this, cx| {
                this.new_res = v;
                cx.notify();
            })
        });
        let me3 = cx.entity();
        // A custom-colour button next to the palette; picks are remembered for every row.
        let bg_apply: Rc<dyn Fn(&mut Home, String)> = Rc::new(|this: &mut Home, col: String| this.new_bg = col);
        let bg_picker = color_picker(&mut self.pickers, "np-bg", &self.new_bg, bg_apply, window, cx);
        let bg = swatches("np-bg", &["#000000", "#ffffff", "#141414", "#1b0f2e", "#0b1220", "#ffd9a0", "#22c55e", "#0099ff"], &self.new_bg, 16., Some(bg_picker), move |c, _, cx| {
            me3.update(cx, |this, cx| {
                this.new_bg = c;
                cx.notify();
            })
        });
        let row = |l: &str, el: gpui::AnyElement| div().flex().flex_col().gap(px(6.)).child(type_micro(div()).text_color(ink_muted()).child(l.to_string())).child(el);
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(label("Settings"))
            .child(row("Frame rate (fps)", fps.into_any_element()))
            .child(row("Export resolution", res.into_any_element()))
            .child(row("Background", bg.into_any_element()))
    }

    /// A format's picture: the sample scene in that shape, `px_h` pixels tall (cached).
    fn format_image(f: &crate::model::format::Format, px_h: u32) -> Arc<RenderImage> {
        let key = format!("format:{}x{}:{px_h}", f.w, f.h);
        if let Some(i) = cached(&key) {
            return i;
        }
        let i = to_image(&crate::render::thumbs::format_preview(f.w, f.h, px_h));
        put(&key, i.clone());
        i
    }

    /// A picture of `f` fitted inside a `bw`×`bh` box.
    fn fitted(f: &crate::model::format::Format, bw: f32, bh: f32) -> impl IntoElement {
        let k = (bw / f.w).min(bh / f.h);
        let (w, h) = (f.w * k, f.h * k);
        div()
            .w(px(bw))
            .h(px(bh))
            .flex()
            .items_center()
            .justify_center()
            .child(img(Self::format_image(f, (h * 2.0).round() as u32)).w(px(w)).h(px(h)).rounded(px(6.)).object_fit(ObjectFit::Fill))
    }

    /// One format tile: its picture, name, ratio and size.
    fn format_card(&self, f: &'static crate::model::format::Format, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.format == f.id;
        div()
            .id(SharedString::from(format!("fmt-{}", f.id)))
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(15.))
            .border_1()
            .border_color(if active { signal() } else { hairline() })
            .bg(if active { surface2() } else { surface1() })
            .p_2()
            .cursor_pointer()
            .hover(|s| s.bg(surface2()))
            .child(div().rounded(px(10.)).bg(canvas_bg()).py_2().flex().justify_center().child(Self::fitted(f, 132., 84.)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px_1()
                    .pb_1()
                    .min_w_0()
                    .child(type_caption(div()).truncate().child(f.name))
                    .child(type_micro(div()).text_size(px(11.)).text_color(ink_muted()).truncate().child(format!("{} · {}", f.aspect, f.pixels))),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.format = f.id;
                this.new_res = if f.id == "vertical-cinema" { 2160 } else { 1080 };
                prefs::set("format", serde_json::Value::String(f.id.into()));
                cx.notify();
            }))
    }

    /// The New project dialog: formats by category on the left, the chosen one, a name and what
    /// to start with on the right.
    fn new_project_dialog(&mut self, start: &'static str, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::model::format::{CATEGORIES, FORMATS};
        let f = crate::model::format::format(self.format);
        let custom = self.format == "custom";
        let size = self.custom_size(cx);
        let (pw, ph) = match size {
            Some([w, h]) => crate::storage::design_canvas(w, h),
            None => (720.0, 1280.0),
        };
        let cat = self.new_cat;
        let tabs = div().flex().flex_wrap().gap(px(6.)).children(CATEGORIES.iter().map(|(c, label)| {
            let active = *c == cat;
            let c = *c;
            div()
                .id(SharedString::from(format!("cat-{label}")))
                .h(px(30.))
                .px_3()
                .flex()
                .items_center()
                .rounded_full()
                .border_1()
                .border_color(if active { alpha(ink(), 0.0) } else { hairline() })
                .bg(if active { ink() } else { alpha(ink(), 0.0) })
                .text_color(if active { black() } else { ink_muted() })
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .when(!active, |d| d.hover(|s| s.bg(surface2()).text_color(ink())))
                .child(*label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.new_cat = c;
                    cx.notify();
                }))
        }));
        let shown: Vec<_> = FORMATS.iter().filter(|x| cat.map(|c| x.category == c).unwrap_or(true)).collect();
        let grid = div()
            .id("fmt-grid")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(div().grid().grid_cols(3).gap_2().pb_1().child(self.custom_card(cx)).children(shown.into_iter().map(|f| self.format_card(f, cx).into_any_element())));

        let starts = [
            ("blank", "Blank", "plus", "An empty timeline"),
            ("sample", "The sample", "sparkles", "Nature reel, laid out for this shape"),
            ("video", "Video files", "upload", "Your footage, straight onto the timeline"),
        ];
        let start_list = div().flex().gap(px(6.)).children(starts.iter().map(|(id, title, ic, desc)| {
            let active = *id == start;
            let id = *id;
            div()
                .id(SharedString::from(format!("start-{id}")))
                .flex()
                .items_center()
                .gap_3()
                .rounded(px(10.))
                .border_1()
                .border_color(if active { signal() } else { hairline() })
                .bg(if active { surface2() } else { alpha(ink(), 0.0) })
                .px_3()
                .py_2()
                .cursor_pointer()
                .hover(|s| s.bg(surface2()))
                .child(div().size(px(28.)).flex_none().rounded_full().bg(surface3()).flex().items_center().justify_center().child(icon(ic, 14., ink())))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .min_w_0()
                        .child(type_caption(div()).child(*title))
                        .child(type_micro(div()).text_size(px(11.)).text_color(ink_muted()).truncate().child(*desc)),
                )
                .w(px(214.))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.new_dialog = Some(id);
                    cx.notify();
                }))
        }));
        let create_label = match start {
            "sample" => "Create from the sample".to_string(),
            "video" => "Choose files…".to_string(),
            _ if custom => "Create custom project".to_string(),
            _ => format!("Create {}", f.name),
        };
        let can_create = !custom || size.is_some();
        let detail = |k: &str, v: &str| {
            div()
                .flex()
                .justify_between()
                .gap_3()
                .child(type_micro(div()).text_color(ink_muted()).child(k.to_string()))
                .child(type_micro(div()).text_right().child(v.to_string()))
        };
        // The details scroll if the window is short; Create stays pinned at the bottom.
        let details = div()
            .id("np-details")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_4()
            .pr_1()
            .child(div().rounded(px(15.)).bg(canvas_bg()).py_3().flex().justify_center().child(if custom { Self::fitted_wh(pw, ph, 260., 130.).into_any_element() } else { Self::fitted(f, 260., 130.).into_any_element() }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().font_family(DISPLAY).text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child(if custom { "Custom size" } else { f.name }))
                    .when(!custom, |d| {
                        d.child(detail("Export size", &format!("{} px", f.pixels)))
                            .child(detail("Ratio", f.aspect))
                            .child(detail("Best for", f.best_for))
                            .child(detail("Length", f.length))
                    })
                    .when(custom, |d| {
                        let field = |input: &Entity<InputState>| {
                            div().flex_1().h(px(36.)).flex().items_center().px_3().rounded(px(10.)).bg(canvas_bg()).border_1().border_color(hairline()).child(div().flex_1().child(Input::new(input).appearance(false)))
                        };
                        d.child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .child(field(&self.custom_w))
                                .child(type_micro(div()).text_color(ink_muted()).child("×"))
                                .child(field(&self.custom_h))
                                .child(icon_button("np-swap", "repeat", "Swap width and height", None, 32., Variant::Ghost, cx.listener(|this, _, window, cx| {
                                    let (w, h) = (this.custom_w.read(cx).value().to_string(), this.custom_h.read(cx).value().to_string());
                                    this.custom_w.update(cx, |s, cx| s.set_value(h, window, cx));
                                    this.custom_h.update(cx, |s, cx| s.set_value(w, window, cx));
                                    cx.notify();
                                }))),
                        )
                        .child(match size {
                            Some([w, h]) => detail("Ratio", &crate::storage::ratio_key(w, h)).into_any_element(),
                            None => type_micro(div()).text_color(coral()).child("Use 64 to 8192 px per side, no more than 4:1.").into_any_element(),
                        })
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(label("Name"))
                    .child(div().h(px(36.)).flex().items_center().px_3().rounded(px(10.)).bg(canvas_bg()).border_1().border_color(hairline()).child(div().flex_1().child(Input::new(&self.new_name).appearance(false).aria_label("Project name")))),
            )
            .child(self.settings_section(window, cx));
        let summary = div()
            .w(px(300.))
            .flex_none()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .pl_6()
            .border_l_1()
            .border_color(hairline_soft())
            .child(details);
        let create = button("np-create", Variant::Primary, 40.)
                    .flex_none()
                    .px_6()
                    .child(icon(if start == "video" { "upload" } else { "plus" }, 16., black()))
                    .child(create_label)
                    .when(!can_create, |d| d.opacity(0.4))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !can_create {
                            return;
                        }
                        this.new_dialog = None;
                        match start {
                            "sample" => this.start(true, vec![], window, cx),
                            "video" => this.pick_and_start(window, cx),
                            _ => this.start(false, vec![], window, cx),
                        }
                    }));
        let footer = div()
            .flex()
            .items_end()
            .gap_4()
            .pt_4()
            .border_t_1()
            .border_color(hairline_soft())
            .child(div().flex().flex_col().gap_2().child(label("Start with")).child(start_list))
            .child(spacer())
            .child(create);

        div()
            .flex()
            .flex_col()
            .gap_5()
            .h(px(720.))
            .max_h_full()
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_4()
                    .child(div().flex().flex_col().gap_1().child(type_headline(div()).child("New project")).child(micro("Choose where it's going. The shape sets the canvas; you can still export to other shapes later.")))
                    .child(icon_button("np-close", "x", "Close", None, 32., Variant::Ghost, cx.listener(|this, _, _, cx| {
                        this.new_dialog = None;
                        cx.notify();
                    }))),
            )
            .child(div().flex().flex_1().min_h_0().gap_6().child(div().flex().flex_col().gap_3().flex_1().min_w_0().min_h_0().child(tabs).child(grid)).child(summary))
            .child(footer)
    }

    /// How to plug Liemo into an AI harness, with copy buttons.
    fn connect_ai_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "rayna".into());
        let claude_code = format!("claude mcp add liemo -- \"{exe}\" mcp");
        let json_cfg = serde_json::to_string_pretty(&serde_json::json!({ "mcpServers": { "liemo": { "command": exe, "args": ["mcp"] } } })).unwrap_or_default();
        let block = |id: &'static str, title: &str, hint: &str, code: String, cx: &mut Context<Self>| {
            let copy = code.clone();
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .items_end()
                        .justify_between()
                        .gap_3()
                        .child(div().flex().flex_col().gap(px(2.)).child(type_caption(div()).child(title.to_string())).child(micro(hint.to_string())))
                        .child(
                            button(id, Variant::Secondary, 30.)
                                .child(icon("copy", 14., ink()))
                                .child("Copy")
                                .on_click(cx.listener(move |_, _, window, cx| {
                                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy.clone()));
                                    window.push_notification("Copied", cx);
                                })),
                        ),
                )
                .child(type_timecode(div(), 12.).font_weight(FontWeight::NORMAL).rounded(px(10.)).bg(canvas_bg()).border_1().border_color(hairline()).p_3().text_color(ink()).child(code))
        };
        let can = [
            ("Projects", "list, create in any format, open in this window"),
            ("Timeline", "add footage, music, overlays, text; move, trim, split, duplicate"),
            ("Look", "100+ effects and filters, transitions, fades, transforms"),
            ("Check", "render a frame as an image to see the result"),
            ("Deliver", "export MP4, WebM, GIF, stills or audio"),
        ];
        div()
            .flex()
            .flex_col()
            .gap_5()
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap_1()
                            .child(type_headline(div()).child("Connect an AI"))
                            .child(micro("Liemo includes an MCP server, so Claude, Cursor or any MCP client can edit your videos. While Liemo is open, its edits appear live in the editor and can be undone like your own.")),
                    )
                    .child(icon_button("ai-close", "x", "Close", None, 32., Variant::Ghost, cx.listener(|this, _, _, cx| {
                        this.connect_ai = false;
                        cx.notify();
                    }))),
            )
            .child(block("ai-cc", "Claude Code", "Run this in a terminal", claude_code, cx))
            .child(block("ai-json", "Claude Desktop, Cursor and other clients", "Add to the client's MCP settings (claude_desktop_config.json, .cursor/mcp.json…)", json_cfg, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(label("What the AI can do"))
                    .children(can.iter().map(|(k, v)| {
                        div().flex().gap_3().child(type_caption(div()).w(px(72.)).flex_none().child(*k)).child(micro(*v))
                    })),
            )
    }

    /// The header's "Update" pill, while a newer release is waiting or installing.
    fn update_pill(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        use crate::update::State;
        let text = match crate::update::state() {
            State::Available(r) => format!("Update to {}", r.version),
            State::Downloading(..) | State::Installing(_) => "Updating…".to_string(),
            State::Done => "Restarting…".to_string(),
            _ => return None,
        };
        Some(
            button("update-pill", Variant::Secondary, 32.)
                .child(div().size(px(7.)).rounded_full().bg(success()))
                .child(text)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.update_open = true;
                    cx.notify();
                })),
        )
    }

    fn update_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::update::{current_version, State};
        let st = crate::update::state();
        let open_page = |url: String| {
            move |_: &gpui::ClickEvent, _: &mut Window, _: &mut gpui::App| {
                let _ = std::process::Command::new("/usr/bin/open").arg(&url).spawn();
            }
        };
        let (title, body): (String, String) = match &st {
            State::Idle | State::Checking => ("Checking for updates…".into(), String::new()),
            State::UpToDate => ("Liemo is up to date".into(), format!("You have the newest version, {}.", current_version())),
            State::Available(r) | State::Downloading(r, ..) | State::Installing(r) => (format!("Liemo {} is available", r.version), format!("You have {}.", current_version())),
            State::Done => ("Restarting into the new version…".into(), String::new()),
            State::Failed(e, _) => ("The update didn't work".into(), e.clone()),
        };
        let notes = match &st {
            State::Available(r) | State::Downloading(r, ..) | State::Installing(r) if !r.notes.is_empty() => Some(r.notes.clone()),
            _ => None,
        };
        let progress = match &st {
            State::Downloading(_, got, total) if *total > 0 => Some((*got as f32 / *total as f32, format!("Downloading… {} of {} MB", got >> 20, total >> 20))),
            State::Downloading(_, got, _) => Some((0.0, format!("Downloading… {} MB", got >> 20))),
            State::Installing(_) => Some((1.0, "Checking the signature and installing…".to_string())),
            _ => None,
        };
        let busy = matches!(st, State::Checking | State::Downloading(..) | State::Installing(_) | State::Done);
        let mut actions = div().flex().justify_end().gap_2();
        if !busy {
            actions = actions.child(button("update-close", Variant::Ghost, 34.).child(if matches!(st, State::Available(_)) { "Later" } else { "Close" }).on_click(cx.listener(|this, _, _, cx| {
                this.update_open = false;
                cx.notify();
            })));
        }
        match &st {
            State::Available(r) => {
                let r = r.clone();
                actions = actions
                    .child(button("update-notes", Variant::Secondary, 34.).child("Release page").on_click(open_page(r.page.clone())))
                    .child(button("update-install", Variant::Primary, 34.).child(icon("download", 14., black())).child("Install and restart").on_click(cx.listener(move |_, _, _, cx| {
                        crate::update::install(r.clone());
                        cx.notify();
                    })));
            }
            State::Failed(_, rel) => {
                let page = rel.as_ref().map(|r| r.page.clone()).unwrap_or_else(|| format!("https://github.com/{}/releases/latest", crate::update::REPO));
                actions = actions
                    .child(button("update-page", Variant::Secondary, 34.).child("Download from GitHub").on_click(open_page(page)))
                    .child(button("update-retry", Variant::Primary, 34.).child("Try again").on_click(cx.listener(|_, _, _, cx| {
                        crate::update::check(false);
                        cx.notify();
                    })));
            }
            _ => {}
        }
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().flex().flex_col().gap_1().child(type_headline(div()).child(title)).when(!body.is_empty(), |d| d.child(micro(body))))
            .when_some(notes, |d, n| {
                d.child(
                    div()
                        .id("update-notes-scroll")
                        .max_h(px(260.))
                        .overflow_y_scroll()
                        .rounded(px(12.))
                        .bg(canvas_bg())
                        .border_1()
                        .border_color(hairline())
                        .p_3()
                        .child(div().text_color(ink_muted()).child(crate::ui::editor::agent_ui::markdown(&n))),
                )
            })
            .when_some(progress, |d, (f, text)| {
                d.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(div().h(px(6.)).w_full().rounded_full().bg(surface3()).child(div().h_full().rounded_full().bg(ink()).w(gpui::relative(f.clamp(0.02, 1.0)))))
                        .child(micro(text)),
                )
            })
            .child(actions)
    }

    fn modal(&self, id: &'static str, content: impl IntoElement, cx: &mut Context<Self>) -> impl IntoElement {
        self.modal_w(id, 440.0, content, cx)
    }

    fn modal_w(&self, id: &'static str, width: f32, content: impl IntoElement, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id(id)
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
                this.renaming = None;
                this.deleting = None;
                this.new_dialog = None;
                this.connect_ai = false;
                if !matches!(crate::update::state(), crate::update::State::Downloading(..) | crate::update::State::Installing(_) | crate::update::State::Done) {
                    this.update_open = false;
                }
                cx.notify();
            }))
            .child(
                div()
                    .w(px(width))
                    .max_h(gpui::relative(0.92))
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
                    .child(content),
            )
    }
}

impl Render for Home {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let q = self.query.trim().to_lowercase();
        let mut shown: Vec<ProjectSummary> = self.projects.clone().unwrap_or_default().into_iter().filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q)).collect();
        match self.sort {
            Sort::Name => shown.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            Sort::Created => shown.sort_by(|a, b| b.created_at.cmp(&a.created_at)),
            Sort::Edited => shown.sort_by(|a, b| b.updated_at.cmp(&a.updated_at)),
        }
        let vw = f32::from(window.viewport_size().width);
        let cols = (((vw.min(1200.0) - 64.0) / 266.0).floor() as u16).max(1);
        let total = self.projects.as_ref().map(|p| p.len());

        let empty = |title: String, body: &str| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .rounded(px(20.))
                .border_1()
                .border_dashed()
                .border_color(hairline())
                .px_6()
                .py_16()
                .child(type_headline(div()).child(title))
                .child(type_caption(div()).font_weight(FontWeight::NORMAL).text_color(ink_muted()).max_w(px(448.)).text_center().child(body.to_string()))
                .into_any_element()
        };
        let list: AnyElement = if self.failed {
            empty("Projects can't be stored here".into(), "Liemo couldn't open its projects folder in Application Support. Check the disk has space and the folder is writable.")
        } else if total == Some(0) {
            empty("No projects yet".into(), "Start one above. The sample is the quickest way to see what Liemo can do.")
        } else if shown.is_empty() {
            empty(format!("Nothing matches “{}”", self.query.trim()), "Try a different name, or clear the search.")
        } else {
            div().grid().grid_cols(cols).gap_x_4().gap_y_6().children(shown.iter().map(|p| self.project_card(p, cx).into_any_element())).into_any_element()
        };

        let sample_strip = div()
            .flex()
            .gap_1()
            .children(["sc-coast", "sc-drive", "sc-aurora"].iter().map(|id| {
                div().w(px(60.)).h(px(40.)).rounded(px(6.)).overflow_hidden().bg(surface3()).children(Self::scene_thumb(id).map(|t| img(t).size_full().rounded(px(6.)).object_fit(ObjectFit::Cover)))
            }))
            .into_any_element();
        let me = cx.entity();
        let sort_label = match self.sort {
            Sort::Edited => "Last edited",
            Sort::Created => "Newest",
            Sort::Name => "Name",
        };
        let sort_menu = Button::new("sort")
            .label(sort_label)
            .outline()
            .dropdown_caret(true)
            .h(px(36.))
            .px_4()
            .rounded(px(999.))
            .bg(surface1())
            .border_1()
            .border_color(hairline())
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM).dropdown_menu_with_anchor(gpui::Anchor::TopRight, move |menu, _, _| {
            let mut menu = menu;
            for (s, l, k) in [(Sort::Edited, "Last edited", "edited"), (Sort::Created, "Newest", "created"), (Sort::Name, "Name", "name")] {
                let me = me.clone();
                menu = menu.item(PopupMenuItem::new(l).on_click(move |_, _, cx| {
                    me.update(cx, |this, cx| {
                        this.sort = s;
                        prefs::set("sort", serde_json::Value::String(k.into()));
                        cx.notify();
                    })
                }));
            }
            menu
        });

        // The header row lines up with the page below (same width, same padding). Only when the
        // window is too narrow for the margin to clear the traffic lights does it step inward.
        let margin = ((f32::from(window.viewport_size().width) - 1200.) / 2.).max(0.);
        let lead = (TRAFFIC_LIGHTS_W - margin).max(32.);
        let header = div().relative().flex_none().border_b_1().border_color(hairline_soft()).bg(canvas_bg()).child(titlebar_drag("home-titlebar")).child(
            div()
                .relative()
                .mx_auto()
                .max_w(px(1200.))
                .w_full()
                .h(px(56.))
                .px_8()
                .pl(px(lead))
                .flex()
                .items_center()
                .gap_3()
                .child(div().flex().items_center().gap_2().child(icon("liemo-mark", 22., ink())).child(div().font_family(DISPLAY).text_size(px(22.)).font_weight(FontWeight::SEMIBOLD).child("Liemo")))
                .child(spacer())
                .when_some(self.update_pill(cx), |d, pill| d.child(pill))
                .child(button("connect-ai", Variant::Secondary, 32.).pl(px(5.)).gap(px(8.)).child(agent_stack(surface2())).child("Connect AI").on_click(cx.listener(|this, _, _, cx| {
                    this.connect_ai = true;
                    cx.notify();
                })))
                .child(button("new", Variant::Primary, 32.).child(icon("plus", 14., black())).child("New project").on_click(cx.listener(|this, _, window, cx| {
                    let _ = window;
                    if this.busy.is_none() {
                        this.new_dialog = Some("blank");
                        cx.notify();
                    }
                }))),
        );

        let main = div()
            .id("home-scroll")
            .flex_1()
            .overflow_y_scroll()
            .child(
                div()
                    .mx_auto()
                    .max_w(px(1200.))
                    .w_full()
                    .px_8()
                    .pt_16()
                    .pb_24()
                    .flex()
                    .flex_col()
                    .gap_12()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(div().font_family(DISPLAY).text_size(px(62.)).line_height(px(62.)).font_weight(FontWeight::SEMIBOLD).child("Projects"))
                            .child(type_body(div()).text_color(ink_muted()).child("Every edit saves as you work. Projects are kept in Liemo's folder on this Mac; imported media stays where it is.")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(self.start_card("blank", "Blank project", "Any shape: YouTube, Reels, TikTok, Shorts, Stories, square and portrait posts.", "plus", None, cx))
                            .child(self.start_card("sample", "Start from the sample", "A nature reel: footage, titles, music and effects, ready to remix in any shape.", "sparkles", Some(sample_strip), cx))
                            .child(self.start_card("video", "Start from a video", "Pick video, photo or audio files. They go straight onto a new timeline.", "upload", None, cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_5()
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_3()
                                    .child(type_headline(div()).child("Your projects"))
                                    .when_some(total, |d, n| d.child(timecode(n.to_string(), 12.).text_color(ink_muted())))
                                    .child(spacer())
                                    .child(
                                        // A 36px pill like the web field: Input only comes in fixed heights,
                                        // so a borderless one sits inside our own frame.
                                        div()
                                            .w(px(240.))
                                            .h(px(36.))
                                            .flex()
                                            .items_center()
                                            .gap_1()
                                            .pl_3()
                                            .pr_1()
                                            .rounded_full()
                                            .bg(surface1())
                                            .border_1()
                                            .border_color(hairline())
                                            .child(icon("search", 16., ink_muted()))
                                            .child(div().flex_1().min_w_0().text_size(px(13.)).child(Input::new(&self.search).appearance(false).aria_label("Search projects"))),
                                    )
                                    .child(sort_menu),
                            )
                            .child(list),
                    ),
            );

        // The New project dialog needs to build first: it may reach into Home (its own
        // custom-colour picker), and a builder chain can't borrow self twice.
        let np_dialog = self.new_dialog.clone().map(|start| {
            self.new_project_dialog(start, window, cx)
        });

        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(canvas_bg())
            .text_color(ink())
            .font_family(SANS)
            .text_size(px(14.))
            .child(header)
            .child(main)
            .when_some(self.renaming.clone(), |d, p| {
                let input = Input::new(&self.rename_input);
                d.child(self.modal(
                    "rename",
                    div()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(div().flex().flex_col().gap_1().child(type_headline(div()).child("Rename project")).child(micro("Shown on your projects page and in the editor.")))
                        .child(input)
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(button("rn-cancel", Variant::Secondary, 36.).child("Cancel").on_click(cx.listener(|this, _, _, cx| {
                                    this.renaming = None;
                                    cx.notify();
                                })))
                                .child(button("rn-save", Variant::Primary, 36.).child("Save name").on_click(cx.listener(|this, _, window, cx| this.save_rename(window, cx)))),
                        ),
                    cx,
                ))
                .when(p.id.is_empty(), |d| d)
            })
            .when(self.connect_ai, |d| d.child(self.modal_w("connect-ai", 600.0, self.connect_ai_dialog(cx), cx)))
            .when(self.update_open, |d| d.child(self.modal_w("update", 480.0, self.update_dialog(cx), cx)))
            .when_some(np_dialog, |d, dlg| d.child(self.modal_w("new-project", 1040.0, dlg, cx)))
            .when_some(self.deleting.clone(), |d, p| {
                d.child(self.modal(
                    "delete",
                    div()
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(type_headline(div()).child(format!("Delete “{}”?", p.name)))
                                .child(micro("The project is removed from Liemo. Imported media files on your Mac are not touched. This can't be undone.")),
                        )
                        .child(
                            div()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(button("del-keep", Variant::Secondary, 36.).child("Keep project").on_click(cx.listener(|this, _, _, cx| {
                                    this.deleting = None;
                                    cx.notify();
                                })))
                                .child(button("del-go", Variant::Destructive, 36.).child("Delete project").on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(p) = this.deleting.take() {
                                        match storage::delete_project(&p.id) {
                                            Ok(_) => window.push_notification(SharedString::from(format!("Deleted {}", p.name)), cx),
                                            Err(_) => window.push_notification("Couldn't delete that project", cx),
                                        }
                                        this.refresh();
                                    }
                                    cx.notify();
                                }))),
                        ),
                    cx,
                ))
            })
    }
}
