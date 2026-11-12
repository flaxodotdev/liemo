// The command palette (⌘K): every editor action, plus the library — text styles, backgrounds,
// effects, transitions, text animations and caption styles — searchable from one box.

use super::{layout, Editor};
use crate::model::assets::{music, overlays, shaders, text_presets, TEXT_ANIMS};
use crate::model::captions::CAPTION_STYLES;
use crate::model::types::AssetType;
use crate::ui::theme::*;
use gpui::prelude::*;
use gpui::{anchored, deferred, div, px, AnyElement, Context, Entity, ScrollHandle, Window};
use gpui_component::input::{Input, InputState};
use std::cell::RefCell;
use std::rc::Rc;

type Act = Rc<dyn Fn(&mut Editor, &mut Window, &mut Context<Editor>)>;

struct Cmd {
    group: &'static str,
    label: String,
    keys: &'static str,
    act: Act,
}

pub struct Palette {
    pub open: bool,
    pub input: Entity<InputState>,
    hi: usize,
    scroll: ScrollHandle,
    /// The query the highlight belongs to; a new query starts at the top again.
    last: RefCell<String>,
}

impl Palette {
    pub fn new(window: &mut Window, cx: &mut Context<Editor>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Type a command, effect, transition, text style…"));
        Palette { open: std::env::var("RAYNA_PALETTE").is_ok(), input, hi: 0, scroll: ScrollHandle::new(), last: RefCell::new(String::new()) }
    }
}

fn cmd(group: &'static str, label: impl Into<String>, keys: &'static str, f: impl Fn(&mut Editor, &mut Window, &mut Context<Editor>) + 'static) -> Cmd {
    Cmd { group, label: label.into(), keys, act: Rc::new(f) }
}

impl Editor {
    pub(super) fn toggle_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette.open = !self.palette.open;
        if self.palette.open {
            self.palette.hi = 0;
            self.palette.input.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.focus(window, cx);
            });
        } else {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    fn commands(&self) -> Vec<Cmd> {
        let s = &self.store;
        let has_sel = s.selected.is_some();
        let text_sel = s.selected_clip().is_some_and(|c| c.kind == AssetType::Text);
        let mut v = vec![
            cmd("Edit", "Undo", "⌘Z", |e, _, _| e.store.undo()),
            cmd("Edit", "Redo", "⇧⌘Z", |e, _, _| e.store.redo()),
            cmd("Edit", "Cut", "⌘X", |e, _, cx| e.cut_to_clipboard(cx)),
            cmd("Edit", "Copy", "⌘C", |e, _, cx| e.copy_to_clipboard(cx)),
            cmd("Edit", "Paste", "⌘V", |e, _, cx| e.paste_from_clipboard(cx)),
            cmd("Edit", "Duplicate", "⌘D", |e, _, _| e.store.duplicate()),
            cmd("Edit", "Delete", "⌫", |e, _, _| e.store.remove()),
            cmd("Edit", "Select all", "⌘A", |e, _, _| e.store.select_all(None)),
            cmd("Edit", "Split at the playhead", "S", |e, _, _| e.store.split()),
            cmd("Edit", "Add or remove a marker", "M", |e, _, _| e.store.toggle_marker()),
            cmd("Edit", "Set the in point", "I", |e, _, _| {
                let t = e.store.time;
                e.store.set_in(Some(t));
            }),
            cmd("Edit", "Set the out point", "O", |e, _, _| {
                let t = e.store.time;
                e.store.set_out(Some(t));
            }),
            cmd("Edit", "Clear in and out", "⌥X", |e, _, _| {
                e.store.set_in(None);
                e.store.set_out(None);
            }),
            cmd("Playback", "Play or pause", "Space", |e, _, _| e.toggle_play()),
            cmd("Playback", "Go to the start", "Home", |e, _, _| e.seek(0.0)),
            cmd("Playback", "Go to the end", "End", |e, _, _| {
                let end = e.store.end();
                e.seek(end - 1.0 / crate::model::time::FPS());
            }),
            cmd("Playback", "Watch full screen", "F", |e, w, cx| e.toggle_cinema(w, cx)),
            cmd("Playback", if s.looping { "Stop looping" } else { "Loop playback" }, "L", |e, _, _| e.store.looping = !e.store.looping),
            cmd("View", "Show or hide the library", "⌘B", |e, _, cx| e.toggle_panel(layout::Edge::Library, cx)),
            cmd("View", "Show or hide the inspector", "⌥⌘B", |e, _, cx| e.toggle_panel(layout::Edge::Inspector, cx)),
            cmd("View", "Show or hide the timeline", "⌘J", |e, _, cx| e.toggle_panel(layout::Edge::Timeline, cx)),
            cmd("View", "Show or hide the AI chat", "⌘I", |e, _, cx| e.toggle_agent(cx)),
            cmd("View", if s.snap { "Turn snapping off" } else { "Turn snapping on" }, "N", |e, _, _| e.store.snap = !e.store.snap),
            cmd("View", if s.guides { "Hide guides" } else { "Show guides" }, "G", |e, _, _| e.store.guides = !e.store.guides),
            cmd("View", if self.rulers { "Hide rulers" } else { "Show rulers" }, "⇧R", |e, _, _| e.toggle_rulers()),
            cmd("View", "Fit the preview", "⌘0", |e, _, _| e.set_preview_zoom(1.0, None)),
            cmd("View", "Zoom to the selection", "⇧2", |e, _, _| e.zoom_to_selection()),
            cmd("AI", "Add the selection to the AI chat", "", |e, w, cx| e.mention_selection(w, cx)),
            cmd("AI", "Show this frame to the AI chat", "", |e, w, cx| e.snapshot_to_chat(None, w, cx)),
            cmd("AI", "Snapshot part of the frame for the AI chat", "", |e, _, _| {
                e.snap_mode = true;
                e.pause();
            }),
            cmd("File", "Import media…", "", |e, w, cx| e.pick_files(None, w, cx)),
            cmd("File", "Search stock photos and video…", "", |e, w, cx| e.open_stock(w, cx)),
            cmd("Check", "Check the design", "", |e, _, cx| e.run_design_check(cx)),
            cmd("AI", "AI mode · Auto (edits straight away)", "", |e, _, _| e.set_agent_mode(crate::agent::Mode::Auto)),
            cmd("AI", "AI mode · Ask first", "", |e, _, _| e.set_agent_mode(crate::agent::Mode::Ask)),
            cmd("AI", "AI mode · Plan (changes nothing)", "", |e, _, _| e.set_agent_mode(crate::agent::Mode::Plan)),
            cmd("AI", "Open this project's documents", "", |e, _, cx| {
                let d = crate::pipeline::dir(&e.project.id);
                let _ = std::fs::create_dir_all(&d);
                cx.reveal_path(&d);
            }),
            cmd("File", "Export…", "", |e, w, cx| e.open_export(w, cx)),
            cmd("File", "Record a voice-over", "", |e, _, cx| e.toggle_recording(cx)),
            cmd("File", "Back to projects", "", |e, w, cx| e.go_home(w, cx)),
            cmd("File", "Save the selection as a component", "", |e, _, cx| e.save_component(cx)),
            cmd("Brand", "Brand kit…", "", |e, w, cx| e.open_brand(w, cx)),
            cmd("Brand", "Apply brand to the selected text", "", |e, _, _| e.apply_brand()),
            cmd("Brand", "Add the brand logo", "", |e, _, cx| e.add_logo(cx)),
            cmd("Captions", "Import SRT or VTT…", "", |e, w, cx| e.pick_subtitles(w, cx)),
        ];
        let _ = has_sel;
        for svc in [crate::integrations::Service::Pexels, crate::integrations::Service::ElevenLabs] {
            if crate::integrations::connected(svc) {
                v.push(cmd("Connections", format!("Disconnect {}", svc.name()), "", move |e, _, _| e.disconnect(svc)));
            } else {
                v.push(cmd("Connections", format!("Connect {}…", svc.name()), "", move |e, w, cx| e.open_connect(svc, w, cx)));
            }
        }
        for st in CAPTION_STYLES.iter() {
            let id = st.id;
            v.push(cmd("Auto captions", format!("Auto captions · {}", st.name), "", move |e, _, cx| e.auto_captions(id, cx)));
        }
        for st in CAPTION_STYLES.iter() {
            let id = st.id;
            v.push(cmd("Restyle captions", format!("Caption style · {}", st.name), "", move |e, _, _| {
                if e.store.restyle_captions(id) == 0 {
                    e.store.toast("No captions to restyle");
                }
            }));
        }
        if text_sel {
            for (a, name) in TEXT_ANIMS {
                v.push(cmd("Text animation", format!("Animate text · {name}"), "", move |e, _, _| {
                    for id in e.store.selection() {
                        e.store.update_text(&id, |t| t.anim = a);
                    }
                }));
            }
        }
        let add = |v: &mut Vec<Cmd>, group: &'static str, prefix: &str, list: Vec<crate::model::assets::Asset>, track: &'static str| {
            for a in list {
                let id = a.id.clone();
                v.push(cmd(group, format!("{prefix} · {}", a.name), "", move |e, _, _| {
                    let t = e.store.time;
                    e.store.add_clip(&id, Some(track), Some(t));
                }));
            }
        };
        add(&mut v, "Add", "Add text", text_presets(), "T1");
        add(&mut v, "Add", "Add background", shaders(), "V1");
        add(&mut v, "Add", "Add overlay", overlays(), "V2");
        add(&mut v, "Add", "Add music", music(), "A1");
        for c in crate::ui::components::list() {
            let id = c.id.clone();
            v.push(cmd("Components", format!("Add component · {}", c.name), "", move |e, _, _| e.insert_component(&id)));
            let (id, name) = (c.id.clone(), c.name.clone());
            v.push(cmd("Components", format!("Remove component · {}", c.name), "", move |e, _, _| {
                crate::ui::components::delete(&id);
                e.store.toast(format!("Removed the component \"{name}\""));
            }));
        }
        for d in crate::fx::registry::effects().iter() {
            let id = d.id;
            v.push(cmd("Effects", format!("Effect · {}", d.name), "", move |e, _, _| e.store.apply_effect(id, None)));
        }
        for d in crate::fx::transitions::TRANSITIONS.iter() {
            let id = d.id;
            v.push(cmd("Transitions", format!("Transition · {}", d.name), "", move |e, _, _| e.store.apply_transition(id, None)));
        }
        v
    }

    /// Run the first palette command whose label starts with `label` (test hook and scripting).
    pub(super) fn run_command(&mut self, label: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(c) = self.commands().into_iter().find(|c| c.label.starts_with(label)) else { return false };
        (c.act)(self, window, cx);
        true
    }

    pub(super) fn render_palette(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let q = self.palette.input.read(cx).value().trim().to_lowercase();
        if *self.palette.last.borrow() != q {
            *self.palette.last.borrow_mut() = q.clone();
            self.palette.scroll.scroll_to_item(0);
        }
        let words: Vec<&str> = q.split_whitespace().collect();
        let mut found: Vec<(u8, Cmd)> = self
            .commands()
            .into_iter()
            .filter_map(|c| {
                let hay = format!("{} {}", c.group, c.label).to_lowercase();
                if !words.iter().all(|w| hay.contains(w)) {
                    return None;
                }
                // Labels that start with the query come first.
                let l = c.label.to_lowercase();
                let rank = if q.is_empty() { 1 } else if l.starts_with(&q) || l.split(" · ").nth(1).is_some_and(|n| n.starts_with(&q)) { 0 } else { 1 };
                Some((rank, c))
            })
            .collect();
        found.sort_by_key(|(r, _)| *r);
        found.truncate(300);
        let n = found.len();
        let hi = if n == 0 { 0 } else { self.palette.hi.min(n - 1) };
        let acts: Rc<Vec<Act>> = Rc::new(found.iter().map(|(_, c)| c.act.clone()).collect());
        let run = |this: &mut Editor, act: Act, window: &mut Window, cx: &mut Context<Editor>| {
            this.palette.open = false;
            window.focus(&this.focus, cx);
            act(this, window, cx);
            this.restart_audio();
            cx.notify();
        };
        let keys = {
            let acts = acts.clone();
            cx.listener(move |this: &mut Editor, ev: &gpui::KeyDownEvent, window, cx| {
                let n = acts.len();
                match ev.keystroke.key.as_str() {
                    "down" if n > 0 => {
                        this.palette.hi = (this.palette.hi.min(n - 1) + 1) % n;
                        this.palette.scroll.scroll_to_item(this.palette.hi);
                    }
                    "up" if n > 0 => {
                        this.palette.hi = (this.palette.hi.min(n - 1) + n - 1) % n;
                        this.palette.scroll.scroll_to_item(this.palette.hi);
                    }
                    "enter" if n > 0 => {
                        let act = acts[this.palette.hi.min(n - 1)].clone();
                        run(this, act, window, cx);
                    }
                    "escape" => {
                        this.palette.open = false;
                        window.focus(&this.focus, cx);
                    }
                    "k" if ev.keystroke.modifiers.platform || ev.keystroke.modifiers.control => {
                        this.palette.open = false;
                        window.focus(&this.focus, cx);
                    }
                    _ => {
                        // Typing moves the highlight back to the best match.
                        this.palette.hi = 0;
                        return;
                    }
                }
                cx.stop_propagation();
                cx.notify();
            })
        };
        let mut list = div().id("palette-list").flex().flex_col().p_1().max_h(px(380.)).overflow_y_scroll().track_scroll(&self.palette.scroll);
        for (i, (_, c)) in found.into_iter().enumerate() {
            let act = c.act.clone();
            list = list.child(
                div()
                    .id(("palette-item", i))
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .py(px(6.))
                    .rounded(px(7.))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .when(i == hi, |d| d.bg(surface2()))
                    .hover(|d| d.bg(surface2()))
                    .child(div().flex_1().min_w_0().truncate().child(c.label))
                    .child(type_caption(div()).text_color(ink_muted()).child(c.group))
                    .when(!c.keys.is_empty(), |d| d.child(kbd(c.keys)))
                    .on_click(cx.listener(move |this, _, window, cx| run(this, act.clone(), window, cx))),
            );
        }
        if n == 0 {
            list = list.child(div().px_2().py_3().child(micro("Nothing matches")));
        }
        let width = px(560.);
        let vw = window.viewport_size().width;
        let panel = div()
            .id("palette")
            .occlude()
            .w(width)
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(12.))
            .bg(surface1())
            .border_1()
            .border_color(hairline())
            .shadow_lg()
            .capture_key_down(keys)
            .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                this.palette.open = false;
                window.focus(&this.focus, cx);
                cx.notify();
            }))
            .child(
                div()
                    .relative()
                    .h(px(44.))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(icon("search", 14., ink_muted()))
                    .child(self.track_bounds("input:palette"))
                    .child(Input::new(&self.palette.input).appearance(false).px_0().text_size(px(14.)).w_full().aria_label("Search commands")),
            )
            .child(div().w_full().h(px(1.)).flex_none().bg(hairline()))
            .child(list);
        let at = gpui::point((vw - width) / 2.0, px(72.));
        deferred(anchored().position(at).snap_to_window_with_margin(px(8.)).child(panel)).with_priority(3).into_any_element()
    }
}
