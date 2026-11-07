// The editor window: owns the edit (Store), playback, the preview renderer, autosave, imports and
// every pointer drag. Panels are split across the files in this module.

mod export_ui;
pub mod geometry;
mod inspector;
mod library;
mod preview;
mod timeline;
mod top_bar;

use super::app::RaynaApp;
use super::images::{self};
use super::previewer::{Job, Previewer};
use super::theme::*;
use crate::audio::AudioEngine;
use crate::media::import::{self, file_name, new_media_id};
use crate::model::assets::asset;
use crate::model::store::{accepts, default_track, project_end, Store, min_clip};
use crate::model::time::{snap_frame, tc, FPS, H, W};
use crate::model::types::*;
use crate::storage::{self, MediaRecord, ProjectRecord};
use futures::StreamExt;
use geometry::{clip_box, local_rect, to_canvas, BoxR};
use gpui::prelude::*;
use gpui::Focusable;
use gpui::{
    canvas, div, px, App, Bounds, ClipboardEntry, ClipboardItem, Context, DispatchPhase, Entity, ExternalPaths, FocusHandle, KeyDownEvent,
    Modifiers, MouseMoveEvent, MouseUpEvent, Pixels, Point, RenderImage, SharedString, Subscription, Task, WeakEntity, Window,
};
use gpui_component::input::{InputEvent, InputState, TextareaState};
use gpui_component::WindowExt;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Clips go on the system clipboard as text with this prefix, so they paste across windows (and
/// between the browser and native versions).
const CLIP_MARK: &str = "rayna-clip:";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Media,
    Music,
    Images,
    Overlays,
    Text,
    Effects,
    Transitions,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PayloadKind {
    Asset,
    Effect,
    Transition,
}
#[derive(Clone, Debug)]
pub struct DragPayload {
    pub kind: PayloadKind,
    pub id: String,
    pub label: String,
    pub thumb: Option<Arc<RenderImage>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DropTarget {
    Lane { track: String, time: f64, ok: bool },
    Clip(String),
    Stage,
    Pool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClipMode {
    Move,
    Left,
    Right,
}

pub type Apply = Rc<dyn Fn(&mut Editor, f64)>;

pub enum Drag {
    Slider { key: String, min: f64, max: f64, step: f64, apply: Apply, history: bool },
    Scrub { was_playing: bool },
    Clip { id: String, mode: ClipMode, x0: f32, y0: f32, o_start: f64, o_dur: f64, o_in: f64, pts: Vec<f64>, moved: bool },
    StageMove { id: String, o: (f64, f64), b0: BoxR, start: (f32, f32), begun: bool },
    StageScale { id: String, s0: f64, center: (f32, f32), start: (f32, f32), begun: bool },
    StageRotate { id: String, r0: f64, center: (f32, f32), start: (f32, f32), begun: bool },
    Range { thumb: usize },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SaveState {
    Saved,
    Saving,
    Error,
}

pub struct Editor {
    app: WeakEntity<RaynaApp>,
    pub project: ProjectRecord,
    pub store: Store,
    pub focus: FocusHandle,
    audio: Rc<AudioEngine>,
    // Inputs
    name_input: Entity<InputState>,
    search_input: Entity<InputState>,
    text_input: Entity<TextareaState>,
    export_name: Entity<InputState>,
    text_bound: Option<String>,
    text_focused: bool,
    _subs: Vec<Subscription>,
    // Playback
    fallback_clock: Option<(Instant, f64)>,
    // Preview
    previewer: Previewer,
    frame: Option<Arc<RenderImage>>,
    last_job: Option<(f64, u64, bool)>,
    clips_arc: Arc<Vec<Clip>>,
    tracks_arc: Arc<Vec<Track>>,
    arc_rev: u64,
    media_rev: u64,
    seen_media_rev: u64,
    // Library
    pub tab: Tab,
    pub query: String,
    // Timeline
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub snap_at: Option<f64>,
    pub dragging_clip: Option<String>,
    // Pointer drags and geometry captured during layout
    drag: Option<Drag>,
    pub geo: Rc<RefCell<HashMap<String, Bounds<Pixels>>>>,
    pub drop_target: Option<DropTarget>,
    pub lib_drag: Option<DragPayload>,
    // Preview overlay
    pub hover_clip: Option<String>,
    pub stage_guides: Vec<(bool, f32)>,
    pub readout: Option<String>,
    // Saving
    pub save_state: SaveState,
    pub saved_at: i64,
    saved_rev: u64,
    save_task: Option<Task<()>>,
    // Export
    pub export: Option<export_ui::ExportUi>,
    pickers: HashMap<String, (Entity<gpui_component::color_picker::ColorPickerState>, Subscription)>,
    scheduled_rev: u64,
    stage_size: (f32, f32),
    _wake: Task<()>,
    _ticker: Task<()>,
}


impl Editor {
    pub fn new(
        project: ProjectRecord,
        audio: Rc<AudioEngine>,
        pending: Vec<PathBuf>,
        app: WeakEntity<RaynaApp>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Lay out (hit-testing, text boxes, new clips) in this project's canvas on the UI thread.
        crate::model::time::set_env(project.env());
        let pool = project.pool.clone();
        // Media first, so clips that use it can draw straight away.
        let mut live_pool = vec![];
        for id in &pool {
            if let Some(rec) = storage::get_media(id) {
                let thumb = storage::load_image(&storage::media_thumb_path(id));
                import::register_media(&rec, thumb, None);
                live_pool.push(id.clone());
                Self::load_payload_bg(rec, cx);
            }
        }
        let store = Store::new(project.name.clone(), project.clips.clone(), project.tracks.clone(), live_pool);
        let focus = cx.focus_handle();

        let name_input = cx.new(|cx| InputState::new(window, cx).default_value(store.name.clone()));
        // Test hook: RAYNA_QUERY pre-fills the library search.
        let query0 = std::env::var("RAYNA_QUERY").unwrap_or_default();
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search library").default_value(query0.clone()));
        let text_input = cx.new(|cx| TextareaState::new(window, cx).rows(2));
        let export_name = cx.new(|cx| InputState::new(window, cx));
        let mut subs = vec![];
        subs.push(cx.subscribe_in(&name_input, window, |this: &mut Editor, input, ev: &InputEvent, _w, cx| {
            if let InputEvent::Change = ev {
                this.store.name = input.read(cx).value().to_string();
                this.store.changed();
                cx.notify();
            }
        }));
        subs.push(cx.subscribe_in(&search_input, window, |this: &mut Editor, input, ev: &InputEvent, _w, cx| {
            if let InputEvent::Change = ev {
                this.query = input.read(cx).value().to_string();
                cx.notify();
            }
        }));
        subs.push(cx.subscribe_in(&text_input, window, |this: &mut Editor, input, ev: &InputEvent, _w, cx| match ev {
            InputEvent::Focus => {
                this.text_focused = true;
                this.store.begin_edit();
            }
            InputEvent::Blur => {
                this.text_focused = false;
                this.store.end_edit();
            }
            InputEvent::Change => {
                if let Some(id) = this.text_bound.clone() {
                    let v = input.read(cx).value().to_string();
                    if this.store.clip(&id).and_then(|c| c.text.as_ref()).map(|t| t.content != v).unwrap_or(false) {
                        this.store.update_text(&id, |t| t.content = v);
                        cx.notify();
                    }
                }
            }
            _ => {}
        }));
        subs.push(cx.subscribe_in(&export_name, window, |this: &mut Editor, input, ev: &InputEvent, _w, cx| {
            if let InputEvent::Change = ev {
                if let Some(e) = &mut this.export {
                    e.name = input.read(cx).value().to_string();
                    cx.notify();
                }
            }
        }));

        // Finished preview frames (and finished thumbnails) wake the editor.
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();
        let previewer = Previewer::new(tx.clone());
        library::warm_thumbs(tx);
        let wake = cx.spawn_in(window, async move |this, cx| {
            while rx.next().await.is_some() {
                let ok = this.update_in(cx, |this, window, cx| {
                    let next = this.previewer.latest.lock().take();
                    if let Some(img) = next {
                        if let Some(old) = this.frame.replace(img) {
                            cx.drop_image(old, Some(window));
                        }
                    }
                    cx.notify();
                });
                if ok.is_err() {
                    break;
                }
            }
        });
        // A slow heartbeat for things that change with time (the "Saved 2 min ago" label).
        let ticker = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(30)).await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        });

        let mut ed = Editor {
            app,
            saved_at: project.updated_at,
            project,
            store,
            focus,
            audio,
            name_input,
            search_input,
            text_input,
            export_name,
            text_bound: None,
            text_focused: false,
            _subs: subs,
            fallback_clock: None,
            previewer,
            frame: None,
            last_job: None,
            clips_arc: Arc::new(vec![]),
            tracks_arc: Arc::new(vec![]),
            arc_rev: u64::MAX,
            media_rev: 0,
            seen_media_rev: 0,
            tab: library::saved_tab(),
            query: query0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            snap_at: None,
            dragging_clip: None,
            drag: None,
            geo: Rc::new(RefCell::new(HashMap::new())),
            drop_target: None,
            lib_drag: None,
            hover_clip: None,
            stage_guides: vec![],
            readout: None,
            save_state: SaveState::Saved,
            saved_rev: 0,
            save_task: None,
            export: None,
            pickers: HashMap::new(),
            scheduled_rev: 0,
            stage_size: (0.0, 0.0),
            _wake: wake,
            _ticker: ticker,
        };
        ed.saved_rev = ed.store.rev;
        ed.scheduled_rev = ed.store.rev;
        if !pending.is_empty() {
            ed.import(pending, Some((0.0, None)), cx);
        }
        // New projects have no card thumbnail yet; make one once media has had a moment to load.
        if !storage::project_thumb_path(&ed.project.id).exists() && !ed.store.clips.is_empty() {
            ed.save_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                let _ = this.update(cx, |this, cx| this.save_now(cx));
            }));
        }
        // Test hooks: RAYNA_TIME=<seconds> parks the playhead, RAYNA_SELECT=<n> selects the nth clip.
        if let Some(t) = std::env::var("RAYNA_TIME").ok().and_then(|v| v.parse::<f64>().ok()) {
            ed.store.time = t;
        }
        if let Some(i) = std::env::var("RAYNA_SELECT").ok().and_then(|v| v.parse::<usize>().ok()) {
            ed.store.selected = ed.store.clips.get(i).map(|c| c.id.clone());
        }
        if let Ok(t) = std::env::var("RAYNA_TAB") {
            ed.tab = match t.as_str() {
                "effects" => Tab::Effects,
                "transitions" => Tab::Transitions,
                "text" => Tab::Text,
                "music" => Tab::Music,
                _ => ed.tab,
            };
        }
        if std::env::var("RAYNA_EXPORT").is_ok() {
            cx.defer_in(window, |this: &mut Editor, window, cx| this.open_export(window, cx));
        }
        window.focus(&ed.focus, cx);
        ed
    }

    fn load_payload_bg(rec: MediaRecord, cx: &mut Context<Self>) {
        let task = cx.background_spawn(async move {
            let (bitmap, _) = import::load_payload(&rec);
            (rec, bitmap)
        });
        cx.spawn(async move |this, cx| {
            let (rec, bitmap) = task.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(b) = bitmap {
                    crate::model::assets::update_asset(&rec.id, |a| a.bitmap = Some(Arc::new(b)));
                }
                this.media_rev += 1;
                cx.notify();
            });
        })
        .detach();
    }

    // ---------- Navigation ----------
    /// The project's canvas, frame rate and background.
    pub fn env(&self) -> crate::model::time::Env {
        self.project.env()
    }

    pub fn go_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::model::time::set_env(Default::default());
        self.pause();
        self.audio.preview(None);
        self.save_now_blocking();
        if let Some(app) = self.app.upgrade() {
            cx.defer_in(window, move |_, window, cx| app.update(cx, |a, cx| a.go_home(window, cx)));
        }
    }

    // ---------- Playback ----------
    pub fn play(&mut self) {
        let end = self.store.end();
        if self.store.time >= end - 1.0 / FPS() {
            self.store.time = 0.0;
        }
        self.store.playing = true;
        self.sync_audio_timeline();
        self.audio.play(self.store.time);
        self.fallback_clock = Some((Instant::now(), self.store.time));
    }
    pub fn pause(&mut self) {
        self.store.playing = false;
        self.audio.stop();
        self.fallback_clock = None;
    }
    pub fn toggle_play(&mut self) {
        if self.store.playing {
            self.pause()
        } else {
            self.play()
        }
    }
    pub fn seek(&mut self, t: f64) {
        let end = self.store.end();
        self.store.time = t.min(end.max(1.0)).max(0.0);
        if self.store.playing {
            self.audio.play(self.store.time);
            self.fallback_clock = Some((Instant::now(), self.store.time));
        }
    }
    pub fn step(&mut self, frames: i32) {
        if self.store.playing {
            self.pause();
        }
        self.seek(snap_frame(self.store.time + frames as f64 / FPS()));
    }
    fn restart_audio(&mut self) {
        if self.store.playing {
            self.sync_audio_timeline();
            self.audio.play(self.store.time);
        }
    }
    fn sync_audio_timeline(&mut self) {
        self.refresh_arcs();
        self.audio.set_timeline(self.clips_arc.clone(), self.tracks_arc.clone());
    }
    fn refresh_arcs(&mut self) {
        if self.arc_rev != self.store.rev {
            self.clips_arc = Arc::new(self.store.clips.clone());
            self.tracks_arc = Arc::new(self.store.tracks.clone());
            self.arc_rev = self.store.rev;
        }
    }
    /// Advance the playhead from the audio clock (or wall time without an output device).
    fn tick(&mut self, window: &mut Window) {
        if !self.store.playing {
            return;
        }
        let t = self
            .audio
            .time()
            .or_else(|| self.fallback_clock.filter(|_| !self.audio.available()).map(|(at, t0)| t0 + at.elapsed().as_secs_f64()))
            .unwrap_or(self.store.time);
        let end = self.store.end();
        if t >= end {
            if self.store.looping {
                self.store.time = 0.0;
                self.audio.play(0.0);
                self.fallback_clock = Some((Instant::now(), 0.0));
            } else {
                self.store.time = (end - 1.0 / FPS()).max(0.0);
                self.pause();
            }
        } else {
            self.store.time = t;
        }
        window.request_animation_frame();
    }

    // ---------- Keyboard ----------
    fn text_entry_focused(&self, window: &Window, cx: &App) -> bool {
        self.name_input.read(cx).focus_handle(cx).contains_focused(window, cx)
            || self.search_input.read(cx).focus_handle(cx).contains_focused(window, cx)
            || self.text_input.read(cx).focus_handle(cx).contains_focused(window, cx)
            || self.export_name.read(cx).focus_handle(cx).contains_focused(window, cx)
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.export.is_some() {
            if ev.keystroke.key == "escape" && !self.export.as_ref().map(|e| e.running()).unwrap_or(false) {
                self.export = None;
                cx.notify();
            }
            return;
        }
        if self.text_entry_focused(window, cx) {
            return;
        }
        let k = ev.keystroke.key.as_str();
        let m = ev.keystroke.modifiers;
        let cmd = m.platform || m.control;
        let s = &mut self.store;
        if k == "space" {
            self.toggle_play();
        } else if cmd {
            match k {
                "z" if m.shift => s.redo(),
                "z" => s.undo(),
                "y" => s.redo(),
                "d" => s.duplicate(),
                "c" => self.copy_to_clipboard(cx),
                "x" => {
                    self.store.cut();
                    self.write_clipboard(cx);
                }
                "v" => self.paste_from_clipboard(cx),
                _ => return,
            }
        } else if m.alt {
            // Option + arrows nudge the selected picture or text on screen (Shift for 10px).
            let d = if m.shift { 10.0 } else { 1.0 };
            let dir = match k {
                "left" => (-d, 0.0),
                "right" => (d, 0.0),
                "up" => (0.0, -d),
                "down" => (0.0, d),
                _ => return,
            };
            if let Some(c) = s.selected_clip().cloned().filter(|c| c.visual()) {
                if !ev.is_held {
                    s.commit();
                }
                s.update_clip(&c.id, |k| {
                    k.x += dir.0;
                    k.y += dir.1;
                });
            }
        } else {
            match k {
                "s" => s.split(),
                "delete" | "backspace" => s.remove(),
                "left" => self.step(if m.shift { -10 } else { -1 }),
                "right" => self.step(if m.shift { 10 } else { 1 }),
                "home" => self.seek(0.0),
                "end" => {
                    let e = self.store.end();
                    self.seek(e - 1.0 / FPS())
                }
                "n" => s.snap = !s.snap,
                "l" => s.looping = !s.looping,
                "g" => s.guides = !s.guides,
                "=" | "+" => self.zoom_by(1.25, None),
                "-" => self.zoom_by(0.8, None),
                "escape" => s.select(None),
                _ => return,
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    // ---------- Clipboard ----------
    fn write_clipboard(&mut self, cx: &mut Context<Self>) {
        if let Some(c) = &self.store.clipboard {
            if let Ok(json) = serde_json::to_string(c) {
                cx.write_to_clipboard(ClipboardItem::new_string(format!("{CLIP_MARK}{json}")));
            }
        }
    }
    fn copy_to_clipboard(&mut self, cx: &mut Context<Self>) {
        if self.store.copy().is_some() {
            self.write_clipboard(cx);
        }
    }
    fn paste_from_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            self.store.paste(None, None);
            return;
        };
        // 1. A picture (a screenshot, a copied image) goes straight onto the timeline.
        for e in item.entries() {
            if let ClipboardEntry::Image(img) = e {
                let ext = match img.format {
                    gpui::ImageFormat::Jpeg => "jpg",
                    gpui::ImageFormat::Gif => "gif",
                    gpui::ImageFormat::Webp => "webp",
                    gpui::ImageFormat::Bmp => "bmp",
                    gpui::ImageFormat::Tiff => "tiff",
                    _ => "png",
                };
                let dir = storage::root().join("pasted");
                let _ = std::fs::create_dir_all(&dir);
                let path = dir.join(format!("Pasted image {}.{ext}", storage::new_id()));
                if std::fs::write(&path, &img.bytes).is_ok() {
                    let t = self.store.time;
                    self.import(vec![path], Some((t, None)), cx);
                    return;
                }
            }
        }
        let text = item.text().unwrap_or_default();
        // 2. A clip copied from Liemo.
        if let Some(json) = text.strip_prefix(CLIP_MARK) {
            match serde_json::from_str::<Clip>(json) {
                Ok(c) => self.store.paste(Some(c), None),
                Err(_) => self.store.toast("That clipboard content couldn't be read as a clip"),
            }
            return;
        }
        // 3. A copied file path (from Finder's "Copy as Pathname").
        let trimmed = text.trim();
        let paths: Vec<PathBuf> = trimmed.lines().map(PathBuf::from).filter(|p| p.is_absolute() && p.exists() && import::kind_of(p).is_some()).collect();
        if !paths.is_empty() {
            let t = self.store.time;
            self.import(paths, Some((t, None)), cx);
            return;
        }
        // 4. Plain text becomes a text clip at the playhead.
        if !trimmed.is_empty() {
            let short = trimmed.chars().count() <= 40 && !trimmed.contains('\n');
            if let Some(c) = self.store.add_clip(if short { "tx-title" } else { "tx-caption" }, Some("T1"), None) {
                let content: String = trimmed.chars().take(280).collect();
                self.store.update_text(&c.id, |t| t.content = content);
                let name = if short { trimmed.to_string() } else { "Pasted text".into() };
                self.store.update_clip(&c.id, |k| k.name = name);
                self.store.toast("Added text from the clipboard");
            }
            return;
        }
        // 5. Nothing usable on the system clipboard: fall back to the last copied clip.
        self.store.paste(None, None);
    }

    // ---------- Import ----------
    pub fn pick_files(&mut self, place: Option<(f64, Option<String>)>, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui::PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("Import".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |this, cx| this.import(paths, place, cx));
            }
        })
        .detach();
    }

    /// Add files to the media pool. With `place`, each file also lands on the timeline once it
    /// has been read: the first at the given time, the rest one after another.
    pub fn import(&mut self, paths: Vec<PathBuf>, place: Option<(f64, Option<String>)>, cx: &mut Context<Self>) {
        let mut accepted = vec![];
        for p in paths {
            if import::kind_of(&p).is_none() {
                self.store.toast(format!("{} isn't a video, audio or image file", file_name(&p)));
                continue;
            }
            accepted.push(p);
        }
        if accepted.is_empty() {
            cx.notify();
            return;
        }
        let n = accepted.len();
        let where_ = if place.is_some() { "the timeline" } else { "the media pool" };
        self.store.toast(if n == 1 { format!("Adding 1 file to {where_}") } else { format!("Adding {n} files to {where_}") });
        let cursor = Rc::new(RefCell::new(place.as_ref().map(|p| p.0).unwrap_or(0.0)));
        // Read files one after another so they land on the timeline in order.
        let jobs: Vec<(PathBuf, String)> = accepted.into_iter().map(|p| (p, new_media_id())).collect();
        cx.spawn(async move |this, cx| {
            for (path, id) in jobs {
                let task = cx.background_spawn(async move { import::read_file(&path, id) });
                let res = task.await;
                let cursor = cursor.clone();
                let place = place.clone();
                let ok = this.update(cx, move |this, cx| {
                    match res {
                        Ok(imp) => {
                            let rec = imp.record.clone();
                            if let Some(t) = &imp.thumb {
                                let _ = storage::save_jpeg(t, &storage::media_thumb_path(&rec.id));
                            }
                            let _ = storage::put_media(&rec);
                            import::register_media(&rec, imp.thumb, imp.bitmap);
                            if !this.store.pool.contains(&rec.id) {
                                this.store.pool.push(rec.id.clone());
                                this.store.changed();
                            }
                            if let Some((_, track)) = &place {
                                let at = *cursor.borrow();
                                if let Some(c) = this.store.add_clip(&rec.id, track.as_deref(), Some(at)) {
                                    *cursor.borrow_mut() = c.start + c.dur;
                                }
                            }
                            if rec.kind != AssetType::Photo {
                                Self::load_payload_bg(rec, cx);
                            }
                            this.media_rev += 1;
                        }
                        Err(e) => this.store.toast(e.to_string()),
                    }
                    cx.notify();
                });
                if ok.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    // ---------- Saving ----------
    /// The project as it is now, including edits not yet saved.
    pub fn record(&self) -> ProjectRecord {
        let s = &self.store;
        ProjectRecord {
            name: if s.name.trim().is_empty() { "Untitled project".into() } else { s.name.trim().to_string() },
            clips: s.clips.clone(),
            tracks: s.tracks.clone(),
            pool: s.pool.clone(),
            duration: project_end(&s.clips),
            clip_count: s.clips.len(),
            updated_at: storage::now_ms(),
            ..self.project.clone()
        }
    }
    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self.save_state = SaveState::Saving;
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(600)).await;
            let _ = this.update(cx, |this, cx| this.save_now(cx));
        }));
    }
    pub fn save_now(&mut self, cx: &mut Context<Self>) {
        let record = self.record();
        let rev = self.store.rev;
        self.project = record.clone();
        let env = self.env();
        let task = cx.background_spawn(async move {
            crate::model::time::set_env(env);
            let mut pool = crate::render::frames::DecoderPool::new(640, 640);
            let thumb = crate::render::thumbs::stage_snapshot(&record.clips, &record.tracks, &mut pool, 384);
            let r = storage::put_project(&record);
            match thumb {
                Some(t) => {
                    let _ = storage::save_jpeg(&t, &storage::project_thumb_path(&record.id));
                }
                None => {
                    let _ = std::fs::remove_file(storage::project_thumb_path(&record.id));
                }
            }
            images::forget(&format!("project:{}", record.id));
            (r.is_ok(), record.updated_at)
        });
        self.save_task = Some(cx.spawn(async move |this, cx| {
            let (ok, at) = task.await;
            let _ = this.update(cx, |this, cx| {
                if ok {
                    this.saved_rev = this.saved_rev.max(rev);
                    this.saved_at = at;
                    this.save_state = if this.saved_rev == this.store.rev { SaveState::Saved } else { SaveState::Saving };
                } else {
                    this.save_state = SaveState::Error;
                }
                cx.notify();
            });
        }));
    }
    /// Save straight away, without a thumbnail refresh (leaving the editor, quitting).
    pub fn save_now_blocking(&mut self) {
        if self.saved_rev == self.store.rev && self.save_state == SaveState::Saved {
            return;
        }
        let record = self.record();
        if storage::put_project(&record).is_ok() {
            self.saved_rev = self.store.rev;
            self.save_state = SaveState::Saved;
        }
    }

    // ---------- Zoom ----------
    /// Keep the time under the cursor (or the centre of the view) still while zooming.
    pub fn zoom_by(&mut self, f: f64, anchor_x: Option<f32>) {
        let view = self.geo.borrow().get("tl-view").copied();
        let view_w = view.map(|b| f32::from(b.size.width)).unwrap_or(800.0);
        let ax = anchor_x.unwrap_or(view_w / 2.0);
        let old = self.store.zoom;
        let t = (self.scroll_x + ax) as f64 / old;
        self.store.set_zoom(old * f);
        self.scroll_x = ((t * self.store.zoom) as f32 - ax).max(0.0);
    }
    pub fn zoom_to(&mut self, z: f64) {
        let f = z / self.store.zoom;
        self.zoom_by(f, None);
    }

    // ---------- Geometry ----------
    pub fn bounds(&self, key: &str) -> Option<Bounds<Pixels>> {
        self.geo.borrow().get(key).copied()
    }
    /// An invisible element that records its bounds under `key` each frame.
    pub fn track_bounds(&self, key: impl Into<String>) -> impl IntoElement {
        let geo = self.geo.clone();
        let key = key.into();
        canvas(move |b, _, _| {
            geo.borrow_mut().insert(key, b);
        }, |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
    /// Timeline time under a window x position.
    pub fn time_at(&self, x: Pixels) -> f64 {
        let Some(v) = self.bounds("tl-view") else { return 0.0 };
        (((f32::from(x) - f32::from(v.origin.x)) + self.scroll_x) as f64 / self.store.zoom).max(0.0)
    }
    /// A window point → canvas units (1280×720) on the stage.
    pub fn stage_point(&self, p: Point<Pixels>) -> Option<(f32, f32)> {
        let b = self.bounds("stage")?;
        let w = f32::from(b.size.width).max(1.0);
        let h = f32::from(b.size.height).max(1.0);
        Some(((f32::from(p.x) - f32::from(b.origin.x)) / w * W(), (f32::from(p.y) - f32::from(b.origin.y)) / h * H()))
    }
    fn lane_at(&self, p: Point<Pixels>) -> Option<String> {
        let view = self.bounds("tl-lanes-view")?;
        if !view.contains(&p) {
            return None;
        }
        let geo = self.geo.borrow();
        self.store.tracks.iter().find(|t| geo.get(&format!("lane:{}", t.id)).map(|b| b.contains(&p)).unwrap_or(false)).map(|t| t.id.clone())
    }

    // ---------- Pointer drags ----------
    pub fn begin_drag(&mut self, d: Drag, cx: &mut Context<Self>) {
        self.drag = Some(d);
        cx.notify();
    }
    pub fn begin_slider(&mut self, key: String, min: f64, max: f64, step: f64, history: bool, apply: Apply, pos: Point<Pixels>, cx: &mut Context<Self>) {
        if history {
            self.store.begin_edit();
        }
        self.drag = Some(Drag::Slider { key, min, max, step, apply, history });
        self.drag_move(pos, Modifiers::default(), cx);
    }

    fn drag_move(&mut self, pos: Point<Pixels>, mods: Modifiers, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else { return };
        let mut drag = drag;
        match &mut drag {
            Drag::Slider { key, min, max, step, apply, .. } => {
                if let Some(b) = self.bounds(key) {
                    let f = ((f32::from(pos.x) - f32::from(b.origin.x)) / f32::from(b.size.width).max(1.0)).clamp(0.0, 1.0) as f64;
                    let mut v = *min + (*max - *min) * f;
                    if *step > 0.0 {
                        v = (((v - *min) / *step).round() * *step + *min).clamp(*min, *max);
                    }
                    let apply = apply.clone();
                    apply(self, v);
                }
            }
            Drag::Scrub { .. } => {
                let t = snap_frame(self.time_at(pos.x));
                self.seek(t);
            }
            Drag::Clip { id, mode, x0, y0, o_start, o_dur, o_in, pts, moved } => {
                self.clip_drag_move(id.clone(), *mode, *x0, *y0, *o_start, *o_dur, *o_in, pts, moved, pos);
            }
            Drag::StageMove { id, o, b0, start, begun } => {
                if let Some(p) = self.stage_point(pos) {
                    self.stage_move(id, *o, *b0, *start, begun, p, mods);
                }
            }
            Drag::StageScale { id, s0, center, start, begun } => {
                if let Some(p) = self.stage_point(pos) {
                    self.stage_scale(id, *s0, *center, *start, begun, p);
                }
            }
            Drag::StageRotate { id, r0, center, start, begun } => {
                if let Some(p) = self.stage_point(pos) {
                    self.stage_rotate(id, *r0, *center, *start, begun, p, mods);
                }
            }
            Drag::Range { thumb } => {
                if let Some(b) = self.bounds("ex-range") {
                    let f = ((f32::from(pos.x) - f32::from(b.origin.x)) / f32::from(b.size.width).max(1.0)).clamp(0.0, 1.0) as f64;
                    let dur = self.store.end().max(1.0 / FPS());
                    let v = (f * dur * FPS()).round() / FPS();
                    if let Some(e) = &mut self.export {
                        e.custom[*thumb] = v;
                    }
                }
            }
        }
        self.drag = Some(drag);
        cx.notify();
    }

    fn drag_end(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else { return };
        match drag {
            Drag::Slider { history, .. } => {
                if history {
                    self.store.end_edit();
                }
            }
            Drag::Scrub { was_playing } => {
                if was_playing {
                    self.play();
                }
            }
            Drag::Clip { id, mode, moved, .. } => {
                self.snap_at = None;
                self.dragging_clip = None;
                if moved {
                    self.store.settle(&id, mode == ClipMode::Move);
                    self.restart_audio();
                }
            }
            Drag::StageMove { begun, .. } | Drag::StageScale { begun, .. } | Drag::StageRotate { begun, .. } => {
                if begun {
                    self.store.end_edit();
                }
                self.stage_guides.clear();
                self.readout = None;
            }
            Drag::Range { .. } => {}
        }
        cx.notify();
    }

    #[allow(clippy::too_many_arguments)]
    fn clip_drag_move(&mut self, id: String, mode: ClipMode, x0: f32, y0: f32, o_start: f64, o_dur: f64, o_in: f64, pts: &[f64], moved: &mut bool, pos: Point<Pixels>) {
        let (x, y) = (f32::from(pos.x), f32::from(pos.y));
        if !*moved && (x - x0).hypot(y - y0) < 3.0 {
            return;
        }
        if !*moved {
            self.store.commit();
            *moved = true;
            self.dragging_clip = Some(id.clone());
        }
        let Some(cur) = self.store.clip(&id).cloned() else { return };
        let zoom = self.store.zoom;
        let snap = self.store.snap;
        let snap_to = |t: f64| -> (f64, Option<f64>) {
            if !snap {
                return (t, None);
            }
            let tol = 8.0 / zoom;
            let mut best: Option<f64> = None;
            for &p in pts {
                if (p - t).abs() < tol && best.map(|b| (p - t).abs() < (b - t).abs()).unwrap_or(true) {
                    best = Some(p);
                }
            }
            match best {
                Some(b) => (b, Some(b)),
                None => (t, None),
            }
        };
        let dx = (x - x0) as f64 / zoom;
        let media = matches!(cur.kind, AssetType::Video | AssetType::Audio);
        let a = asset(&cur.asset);
        let mut hit = None;
        let mut start = cur.start;
        let mut dur = cur.dur;
        let mut in_ = cur.in_;
        let mut track = cur.track.clone();
        match mode {
            ClipMode::Move => {
                let mut ns = (o_start + dx).max(0.0);
                let a1 = snap_to(ns);
                let a2 = snap_to(ns + cur.dur);
                if a1.1.is_some() {
                    ns = a1.0;
                    hit = a1.1;
                } else if a2.1.is_some() {
                    ns = a2.0 - cur.dur;
                    hit = a2.1;
                }
                start = snap_frame(ns.max(0.0));
                if let Some(tid) = self.lane_at(pos) {
                    if tid != cur.track && accepts(&self.store.tracks, &tid, cur.kind) && !self.store.track(&tid).locked {
                        track = tid;
                    }
                }
            }
            ClipMode::Left => {
                let end = o_start + o_dur;
                let sn = snap_to(o_start + dx);
                hit = sn.1;
                let mut ns = sn.0.min(end - min_clip()).max(0.0);
                if self.store.edit_mode == crate::model::store::EditMode::Insert {
                    // Insert mode never cuts: the start edge stops at the previous clip.
                    let prev_end = self
                        .store
                        .clips
                        .iter()
                        .filter(|k| k.track == cur.track && k.id != id && k.start + k.dur <= o_start + 1e-6)
                        .fold(0.0f64, |m, k| m.max(k.start + k.dur));
                    ns = ns.max(prev_end);
                }
                if media {
                    ns = ns.max(o_start - o_in);
                }
                start = snap_frame(ns);
                dur = end - start;
                in_ = (o_in + (start - o_start)).max(0.0);
            }
            ClipMode::Right => {
                let sn = snap_to(o_start + o_dur + dx);
                hit = sn.1;
                let mut d = (sn.0 - cur.start).max(min_clip());
                if media {
                    if let Some(a) = &a {
                        d = d.min(a.dur - cur.in_);
                    }
                }
                dur = snap_frame(d);
            }
        }
        self.store.update_clip(&id, |c| {
            c.start = start;
            c.dur = dur;
            c.in_ = in_;
            c.track = track;
            c.fade_in = c.fade_in.min(dur / 2.0);
            c.fade_out = c.fade_out.min(dur / 2.0);
        });
        self.snap_at = hit;
    }

    // Stage gestures (move with snapping to centre lines and safe margins; scale from a corner;
    // rotate about the box centre). Coordinates are canvas units.
    #[allow(clippy::too_many_arguments)]
    fn stage_move(&mut self, id: &str, o: (f64, f64), b0: BoxR, start: (f32, f32), begun: &mut bool, p: (f32, f32), mods: Modifiers) {
        let k = self.bounds("stage").map(|b| f32::from(b.size.width) / W()).unwrap_or(1.0);
        if !*begun {
            if ((p.0 - start.0) * k).hypot((p.1 - start.1) * k) < 2.0 {
                return;
            }
            *begun = true;
            self.store.begin_edit();
        }
        let mut nx = o.0 + (p.0 - start.0) as f64;
        let mut ny = o.1 + (p.1 - start.1) as f64;
        let mut found = vec![];
        if self.store.snap && !mods.alt {
            const SAFE: f32 = 0.05;
            let tol = 7.0 / k;
            let cx = b0.cx + (nx - o.0) as f32;
            let cy = b0.cy + (ny - o.1) as f32;
            let (hw, hh) = (b0.w / 2.0, b0.h / 2.0);
            let rotated = (b0.rot % 90.0).abs() > 0.5;
            let mut xs = vec![(W() / 2.0, cx)];
            let mut ys = vec![(H() / 2.0, cy)];
            if !rotated {
                xs.push((W() * SAFE, cx - hw));
                xs.push((W() * (1.0 - SAFE), cx + hw));
                ys.push((H() * SAFE, cy - hh));
                ys.push((H() * (1.0 - SAFE), cy + hh));
            }
            if let Some((t, c)) = xs.iter().find(|(t, c)| (t - c).abs() < tol) {
                nx += (t - c) as f64;
                found.push((true, *t));
            }
            if let Some((t, c)) = ys.iter().find(|(t, c)| (t - c).abs() < tol) {
                ny += (t - c) as f64;
                found.push((false, *t));
            }
        }
        let (nx, ny) = (nx.round(), ny.round());
        self.store.update_clip(id, |c| {
            c.x = nx;
            c.y = ny;
        });
        self.stage_guides = found;
        self.readout = Some(format!("X {nx}  Y {ny}"));
    }
    fn stage_scale(&mut self, id: &str, s0: f64, center: (f32, f32), start: (f32, f32), begun: &mut bool, p: (f32, f32)) {
        if !*begun {
            *begun = true;
            self.store.begin_edit();
        }
        let Some(clip) = self.store.clip(id).cloned() else { return };
        let d0 = (start.0 - center.0).hypot(start.1 - center.1).max(1.0) as f64;
        let s = ((s0 * (p.0 - center.0).hypot(p.1 - center.1) as f64) / d0).clamp(0.05, 8.0);
        // Keep the box centre where it is: compensate the offset for the scale change.
        let r = local_rect(&clip);
        let lc = ((r.x + r.w / 2.0 - W() / 2.0) as f64, (r.y + r.h / 2.0 - H() / 2.0) as f64);
        let a = clip.rot.to_radians();
        let ds = s - clip.scale;
        let shift = ((lc.0 * a.cos() - lc.1 * a.sin()) * ds, (lc.0 * a.sin() + lc.1 * a.cos()) * ds);
        let s = (s * 1000.0).round() / 1000.0;
        self.store.update_clip(id, |c| {
            c.scale = s;
            c.x = (clip.x - shift.0).round();
            c.y = (clip.y - shift.1).round();
        });
        self.readout = Some(format!("{}%", (s * 100.0).round()));
    }
    #[allow(clippy::too_many_arguments)]
    fn stage_rotate(&mut self, id: &str, r0: f64, center: (f32, f32), start: (f32, f32), begun: &mut bool, p: (f32, f32), mods: Modifiers) {
        if !*begun {
            *begun = true;
            self.store.begin_edit();
        }
        let Some(clip) = self.store.clip(id).cloned() else { return };
        let a0 = (start.1 - center.1).atan2(start.0 - center.0);
        let a1 = (p.1 - center.1).atan2(p.0 - center.0);
        let mut r = r0 + ((a1 - a0) as f64).to_degrees();
        r = (((r + 180.0) % 360.0) + 360.0) % 360.0 - 180.0;
        if mods.shift {
            r = (r / 15.0).round() * 15.0;
        }
        r = r.round();
        // Rotation pivots on the canvas centre; move the clip so it turns about its own box.
        let rect = local_rect(&clip);
        let lc = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
        let before = to_canvas(&clip, lc.0, lc.1);
        let after = to_canvas(&Clip { rot: r, ..clip.clone() }, lc.0, lc.1);
        self.store.update_clip(id, |c| {
            c.rot = r;
            c.x = (clip.x + (before.0 - after.0) as f64).round();
            c.y = (clip.y + (before.1 - after.1) as f64).round();
        });
        self.readout = Some(format!("{r}°"));
    }

    pub fn start_clip_drag(&mut self, id: &str, mode: ClipMode, pos: Point<Pixels>, cx: &mut Context<Self>) {
        self.store.select(Some(id.to_string()));
        let Some(c) = self.store.clip(id).cloned() else { return };
        if self.store.track(&c.track).locked {
            let n = self.store.track(&c.track).name.clone();
            self.store.toast(format!("{n} is locked"));
            cx.notify();
            return;
        }
        let mut pts = vec![0.0, self.store.time];
        for k in self.store.clips.iter().filter(|k| k.id != id) {
            pts.push(k.start);
            pts.push(k.start + k.dur);
        }
        self.begin_drag(
            Drag::Clip { id: id.to_string(), mode, x0: f32::from(pos.x), y0: f32::from(pos.y), o_start: c.start, o_dur: c.dur, o_in: c.in_, pts, moved: false },
            cx,
        );
    }

    pub fn start_stage_gesture(&mut self, kind: &str, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(c) = self.store.selected_clip().cloned() else { return };
        if self.locked_on_stage(&c) {
            cx.notify();
            return;
        }
        let Some(p) = self.stage_point(pos) else { return };
        let b = clip_box(&c);
        let d = match kind {
            "scale" => Drag::StageScale { id: c.id.clone(), s0: c.scale, center: (b.cx, b.cy), start: p, begun: false },
            "rotate" => Drag::StageRotate { id: c.id.clone(), r0: c.rot, center: (b.cx, b.cy), start: p, begun: false },
            _ => Drag::StageMove { id: c.id.clone(), o: (c.x, c.y), b0: b, start: p, begun: false },
        };
        self.begin_drag(d, cx);
    }
    pub fn locked_on_stage(&mut self, c: &Clip) -> bool {
        let t = self.store.track(&c.track).clone();
        if t.locked {
            self.store.toast(format!("{} is locked. Unlock it to move things on it.", t.name));
        }
        t.locked
    }

    // ---------- Library drag and file drops ----------
    fn update_drop_target(&mut self, pos: Point<Pixels>, payload: Option<&DragPayload>) {
        let target = (|| {
            let kind = payload.map(|p| p.kind).unwrap_or(PayloadKind::Asset);
            if kind != PayloadKind::Asset {
                // Effects and transitions go onto a clip.
                let view = self.bounds("tl-lanes-view")?;
                if !view.contains(&pos) {
                    return None;
                }
                let geo = self.geo.borrow();
                return self.store.clips.iter().find(|c| geo.get(&format!("clip:{}", c.id)).map(|b| b.contains(&pos)).unwrap_or(false)).map(|c| DropTarget::Clip(c.id.clone()));
            }
            if let Some(track) = self.lane_at(pos) {
                let mut time = snap_frame(self.time_at(pos.x));
                if self.store.edit_mode == crate::model::store::EditMode::Insert {
                    // Same rule as the drop itself: inside a clip lands before it (first half) or after it.
                    if let Some(h) = self.store.clips.iter().find(|k| k.track == track && time > k.start && time < k.start + k.dur) {
                        time = if time - h.start < h.dur / 2.0 { h.start } else { h.start + h.dur };
                    }
                }
                let ok = match payload {
                    Some(p) => asset(&p.id).map(|a| accepts(&self.store.tracks, &track, a.kind)).unwrap_or(false),
                    None => true,
                } && !self.store.track(&track).locked;
                return Some(DropTarget::Lane { track, time, ok });
            }
            if self.bounds("stage-wrap").map(|b| b.contains(&pos)).unwrap_or(false) {
                return Some(DropTarget::Stage);
            }
            if payload.is_none() {
                return Some(DropTarget::Pool);
            }
            None
        })();
        self.drop_target = target;
    }

    fn drop_payload(&mut self, p: &DragPayload, cx: &mut Context<Self>) {
        let target = self.drop_target.take();
        self.lib_drag = None;
        let Some(t) = target else { return };
        match (&t, p.kind) {
            (DropTarget::Clip(id), PayloadKind::Effect) => self.store.apply_effect(&p.id, Some(id)),
            (DropTarget::Clip(id), PayloadKind::Transition) => self.store.apply_transition(&p.id, Some(id)),
            (DropTarget::Stage, PayloadKind::Asset) => {
                self.store.add_clip(&p.id, None, None);
            }
            (DropTarget::Lane { track, time, ok }, PayloadKind::Asset) => {
                let tr = self.store.track(track).clone();
                if tr.locked {
                    self.store.toast(format!("{} is locked. Unlock it to drop clips there.", tr.name));
                    return;
                }
                let kind = asset(&p.id).map(|a| a.kind).unwrap_or(AssetType::Scene);
                if !ok {
                    let def = self.store.track(default_track(kind)).name.clone();
                    self.store.toast(format!("{} doesn't take {} clips, so it went on {def}.", tr.name, format!("{kind:?}").to_lowercase()));
                }
                self.store.add_clip(&p.id, if *ok { Some(track.as_str()) } else { None }, Some(*time));
            }
            _ => {}
        }
        cx.notify();
    }

    fn drop_files(&mut self, paths: &ExternalPaths, cx: &mut Context<Self>) {
        let target = self.drop_target.take();
        let files: Vec<PathBuf> = paths.paths().to_vec();
        match target {
            Some(DropTarget::Lane { track, time, .. }) => {
                if self.store.track(&track).locked {
                    let n = self.store.track(&track).name.clone();
                    self.store.toast(format!("{n} is locked. Unlock it to drop clips there."));
                    cx.notify();
                    return;
                }
                self.import(files, Some((time, Some(track))), cx);
            }
            Some(DropTarget::Stage) => {
                let t = self.store.time;
                self.import(files, Some((t, None)), cx);
            }
            _ => self.import(files, None, cx),
        }
    }

    pub fn add_at_playhead(&mut self, id: &str) {
        if let Some(c) = self.store.add_clip(id, None, None) {
            self.store.toast(format!("Added {} at {}", c.name, tc(c.start)));
        }
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.tick(window);
        // Text-field bounds are re-recorded every frame; drop ones whose field has gone.
        self.geo.borrow_mut().retain(|k, _| !k.starts_with("input:"));

        // Changes to the edit: autosave, sound and picture follow.
        if self.store.rev != self.scheduled_rev {
            self.scheduled_rev = self.store.rev;
            self.schedule_save(cx);
        }
        let rev_changed = self.store.rev != self.arc_rev;
        if rev_changed {
            self.sync_audio_timeline();
        }
        let media_changed = self.media_rev != self.seen_media_rev;
        self.seen_media_rev = self.media_rev;
        let key = (self.store.time, self.store.rev, media_changed);
        if self.last_job.map(|k| k.0 != key.0 || k.1 != key.1).unwrap_or(true) || media_changed {
            self.refresh_arcs();
            self.previewer.request(Job { time: self.store.time, clips: self.clips_arc.clone(), tracks: self.tracks_arc.clone(), reset_media: false, env: self.env() });
            self.last_job = Some(key);
        }

        // Toasts from edit actions.
        if !self.store.toasts.is_empty() {
            let msgs: Vec<String> = self.store.toasts.drain(..).collect();
            cx.defer_in(window, move |_, window, cx| {
                for m in msgs {
                    window.push_notification(SharedString::from(m), cx);
                }
            });
        }

        // Keep the clip's text box in step with the selection (and with undo).
        let sel_text = self.store.selected_clip().and_then(|c| c.text.as_ref().map(|t| (c.id.clone(), t.content.clone())));
        match sel_text {
            Some((id, content)) => {
                let cur = self.text_input.read(cx).value().to_string();
                if self.text_bound.as_deref() != Some(&id) || (!self.text_focused && cur != content) {
                    self.text_bound = Some(id);
                    self.text_input.update(cx, |s, cx| s.set_value(content, window, cx));
                }
            }
            None => self.text_bound = None,
        }
        let name_now = self.name_input.read(cx).value().to_string();
        if name_now != self.store.name && !self.name_input.read(cx).focus_handle(cx).is_focused(window) {
            let n = self.store.name.clone();
            self.name_input.update(cx, |s, cx| s.set_value(n, window, cx));
        }

        let entity = cx.entity();
        let dragging = self.drag.is_some();
        div()
            .id("editor")
            .key_context("Editor")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_down(gpui::MouseButton::Left, cx.listener(|this, ev: &gpui::MouseDownEvent, window, cx| {
                // Clicking away from a text field gives the keyboard back to the editor. A click in
                // a text field (which has just focused it) or anywhere else (an open menu) keeps focus.
                let in_field = this.geo.borrow().iter().any(|(k, b)| k.starts_with("input:") && b.contains(&ev.position));
                if !in_field && this.text_entry_focused(window, cx) {
                    window.focus(&this.focus, cx);
                }
            }))
            .on_drag_move::<DragPayload>(cx.listener(|this, ev: &gpui::DragMoveEvent<DragPayload>, _, cx| {
                let p = ev.drag(cx).clone();
                this.lib_drag = Some(p.clone());
                let before = this.drop_target.clone();
                this.update_drop_target(ev.event.position, Some(&p));
                if before != this.drop_target {
                    cx.notify();
                }
            }))
            .on_drop::<DragPayload>(cx.listener(|this, p: &DragPayload, _, cx| this.drop_payload(p, cx)))
            .on_drag_move::<ExternalPaths>(cx.listener(|this, ev: &gpui::DragMoveEvent<ExternalPaths>, _, cx| {
                let before = this.drop_target.clone();
                this.update_drop_target(ev.event.position, None);
                if before != this.drop_target {
                    cx.notify();
                }
            }))
            .on_drop::<ExternalPaths>(cx.listener(|this, p: &ExternalPaths, _, cx| this.drop_files(p, cx)))
            .size_full()
            .bg(canvas_bg())
            .text_color(ink())
            .font_family(SANS)
            .text_size(px(14.))
            .flex()
            .flex_col()
            .gap_2()
            .px_4()
            .pb_3()
            .child(self.render_top_bar(window, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_2()
                    .child(self.render_library(window, cx))
                    .child(self.render_preview(window, cx))
                    .child(self.render_inspector(window, cx)),
            )
            .child(div().h(px(320.)).min_h(px(240.)).flex_none().flex().flex_col().child(self.render_timeline(window, cx)))
            .when_some(self.export.is_some().then_some(()), |d, _| d.child(self.render_export(window, cx)))
            .when(dragging, |d| {
                // While a pointer drag runs, follow the pointer anywhere in the window.
                let e1 = entity.clone();
                let e2 = entity.clone();
                d.child(
                    canvas(|_, _, _| {}, move |_, _, window, _| {
                        let e1 = e1.clone();
                        window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _w, cx| {
                            if phase == DispatchPhase::Bubble {
                                e1.update(cx, |this, cx| this.drag_move(ev.position, ev.modifiers, cx));
                            }
                        });
                        let e2 = e2.clone();
                        window.on_mouse_event(move |_: &MouseUpEvent, phase, _w, cx| {
                            if phase == DispatchPhase::Bubble {
                                e2.update(cx, |this, cx| this.drag_end(cx));
                            }
                        });
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
            })
    }
}
