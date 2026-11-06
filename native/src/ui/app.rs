// The window's content: the projects page or an open project, and the audio output they share.

use super::editor::Editor;
use crate::mcp::tools::{self, Workspace, R};
use crate::storage::ProjectRecord;
use futures::StreamExt;
use serde_json::Value;
use super::home::Home;
use crate::audio::AudioEngine;
use crate::storage;
use gpui::prelude::*;
use gpui::{div, Context, Entity, Window};
use gpui_component::WindowExt;
use std::path::PathBuf;
use std::rc::Rc;

enum Screen {
    Home(Entity<Home>),
    Editor(Entity<Editor>),
}

pub struct RaynaApp {
    screen: Screen,
    audio: Rc<AudioEngine>,
}

impl RaynaApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let me = cx.entity().downgrade();
        let home = cx.new(|cx| Home::new(me, window, cx));
        // Test hook: RAYNA_OPEN=sample starts straight in a fresh copy of the sample project…
        if std::env::var("RAYNA_OPEN").as_deref() == Ok("sample") {
            if let Ok(id) = super::home::create_project(true, None, &super::home::NewProject::of(&std::env::var("RAYNA_FORMAT").unwrap_or_else(|_| "youtube".into()))) {
                cx.defer_in(window, move |this: &mut RaynaApp, window, cx| this.open_project(&id, vec![], window, cx));
            }
        } else if let Ok(id) = std::env::var("RAYNA_OPEN") {
            // …or any saved project by id.
            cx.defer_in(window, move |this: &mut RaynaApp, window, cx| this.open_project(&id, vec![], window, cx));
        }
        // Closing the window saves the open project straight away.
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            crate::mcp::unlisten();
            let _ = weak.update(cx, |a, cx| a.flush(cx));
            true
        });
        Self::take_ai_edits(window, cx);
        RaynaApp { screen: Screen::Home(home), audio: Rc::new(AudioEngine::new()) }
    }

    pub fn open_project(&mut self, id: &str, pending: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(record) = storage::get_project(id) else {
            window.push_notification("Project not found. It may have been deleted.", cx);
            return;
        };
        let me = cx.entity().downgrade();
        let audio = self.audio.clone();
        let ed = cx.new(|cx| Editor::new(record, audio, pending, me, window, cx));
        self.screen = Screen::Editor(ed);
        cx.notify();
    }

    pub fn go_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let me = cx.entity().downgrade();
        let home = cx.new(|cx| Home::new(me, window, cx));
        self.screen = Screen::Home(home);
        cx.notify();
    }

    /// Accept tool calls from `rayna mcp` (AI harnesses) while the app runs. Edits go to the
    /// open editor when it shows that project, so they appear live; rendering and export run on
    /// the socket thread from a snapshot, so the UI stays responsive.
    fn take_ai_edits(window: &mut Window, cx: &mut Context<Self>) {
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<AiRequest>();
        crate::mcp::listen(move |name, args| {
            if tools::is_heavy(&name) {
                let mut ws = Snapshot { tx: tx.clone() };
                return crate::mcp::to_mcp(tools::call(&name, &args, &mut ws));
            }
            let (rtx, rrx) = crossbeam_channel::bounded(1);
            if tx.unbounded_send(AiRequest::Tool(name, args, rtx)).is_err() {
                return crate::mcp::to_mcp(Err("Liemo is closing".into()));
            }
            rrx.recv().unwrap_or_else(|_| crate::mcp::to_mcp(Err("Liemo is closing".into())))
        });
        cx.spawn_in(window, async move |this, cx| {
            while let Some(req) = rx.next().await {
                if this.update_in(cx, |app, window, cx| app.handle_ai(req, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn handle_ai(&mut self, req: AiRequest, window: &mut Window, cx: &mut Context<Self>) {
        let editor = match &self.screen {
            Screen::Editor(e) => Some(e.clone()),
            _ => None,
        };
        match req {
            AiRequest::Record(id, reply) => {
                let r = match &editor {
                    Some(e) if e.read(cx).project.id == id => Ok(e.read(cx).record()),
                    _ => crate::storage::get_project(&id).ok_or_else(|| format!("No project with id {id}")),
                };
                let _ = reply.send(r);
            }
            AiRequest::Tool(name, args, reply) => {
                let mut ws = Live { editor, cx: &mut *cx, open: None, changed: false };
                let result = crate::mcp::to_mcp(tools::call(&name, &args, &mut ws));
                let (open, changed) = (ws.open.take(), ws.changed);
                let _ = reply.send(result);
                if let Some(id) = open {
                    self.open_project(&id, vec![], window, cx);
                } else if changed {
                    if let Screen::Home(h) = &self.screen {
                        h.update(cx, |h, cx| {
                            h.refresh();
                            cx.notify();
                        });
                    }
                }
            }
        }
    }

    /// Save straight away (the window is closing or the app is quitting).
    pub fn flush(&mut self, cx: &mut Context<Self>) {
        if let Screen::Editor(e) = &self.screen {
            e.update(cx, |e, _| e.save_now_blocking());
        }
    }
}

impl Render for RaynaApp {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(match &self.screen {
            Screen::Home(h) => h.clone().into_any_element(),
            Screen::Editor(e) => e.clone().into_any_element(),
        })
    }
}

enum AiRequest {
    Tool(String, Value, crossbeam_channel::Sender<Value>),
    Record(String, crossbeam_channel::Sender<R<ProjectRecord>>),
}

/// Reads the latest project state from the UI thread; used by rendering and export.
struct Snapshot {
    tx: futures::channel::mpsc::UnboundedSender<AiRequest>,
}
impl Workspace for Snapshot {
    fn record(&mut self, project: &str) -> R<ProjectRecord> {
        let (rtx, rrx) = crossbeam_channel::bounded(1);
        self.tx.unbounded_send(AiRequest::Record(project.into(), rtx)).map_err(|_| "Liemo is closing".to_string())?;
        rrx.recv().map_err(|_| "Liemo is closing".to_string())?
    }
    fn edit(&mut self, _: &str, _: &mut dyn FnMut(&mut crate::model::store::Store) -> R<Value>) -> R<Value> {
        Err("Not available here".into())
    }
    fn live(&self) -> bool {
        true
    }
}

/// Edits on the UI thread: into the open editor when it shows the project, else into its files.
struct Live<'a, 'b> {
    editor: Option<Entity<Editor>>,
    cx: &'a mut Context<'b, RaynaApp>,
    open: Option<String>,
    changed: bool,
}
impl Workspace for Live<'_, '_> {
    fn record(&mut self, project: &str) -> R<ProjectRecord> {
        match &self.editor {
            Some(e) if e.read(self.cx).project.id == project => Ok(e.read(self.cx).record()),
            _ => tools::FileWorkspace.record(project),
        }
    }
    fn edit(&mut self, project: &str, f: &mut dyn FnMut(&mut crate::model::store::Store) -> R<Value>) -> R<Value> {
        match self.editor.clone() {
            Some(e) if e.read(self.cx).project.id == project => e.update(self.cx, |ed, cx| {
                let before = ed.store.toasts.len();
                let r = f(&mut ed.store);
                // The editor shows these as toasts too; the AI gets them as notes.
                let notes = ed.store.toasts[before..].to_vec();
                cx.notify();
                r.map(|v| tools::with_notes(v, notes))
            }),
            _ => {
                self.changed = true;
                tools::FileWorkspace.edit(project, f)
            }
        }
    }
    fn open(&mut self, project: &str) -> R<()> {
        self.open = Some(project.into());
        Ok(())
    }
    fn projects_changed(&mut self) {
        self.changed = true;
    }
    fn live(&self) -> bool {
        true
    }
}
