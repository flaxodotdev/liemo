// The Swift-facing API (UniFFI). The iOS app drives everything through `Engine`: the same JSON
// tools as the MCP server for every edit (the editor UI and Liemo AI use them alike), plus
// rendering frames and sound for the preview, playback and export.

use crate::mcp::tools::{self, FileWorkspace, Workspace, R};
use crate::model::store::{project_end, Store};
use crate::render::canvas::{to_straight, Canvas};
use crate::storage::{self, ProjectRecord};
use parking_lot::Mutex;
use serde_json::Value;
use std::sync::Arc;
use tiny_skia::Pixmap;

/// A picture: straight-alpha RGBA, row by row.
#[derive(uniffi::Record)]
pub struct MediaFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(uniffi::Record)]
pub struct MediaProbe {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub has_video: bool,
    pub has_audio: bool,
}

/// What the app does for the engine with AVFoundation and Image I/O.
#[uniffi::export(with_foreign)]
pub trait MediaHost: Send + Sync {
    /// Length, size and streams of a video or audio file.
    fn probe(&self, path: String) -> Option<MediaProbe>;
    /// The video picture at `time` seconds, fitting in `max_w`×`max_h`.
    fn video_frame(&self, path: String, time: f64, max_w: u32, max_h: u32) -> Option<MediaFrame>;
    /// A picture the engine can't decode itself (HEIC…), long side at most `max`.
    fn decode_image(&self, path: String, max: u32) -> Option<MediaFrame>;
    /// The file's sound as 48 kHz interleaved stereo.
    fn decode_audio(&self, path: String) -> Option<Vec<f32>>;
}

#[derive(uniffi::Record)]
pub struct ToolResult {
    pub ok: bool,
    /// JSON (or an error message when !ok).
    pub text: String,
    /// render_frame's picture.
    pub png: Option<Vec<u8>>,
}

fn frame_of(pm: &Pixmap) -> MediaFrame {
    MediaFrame { width: pm.width(), height: pm.height(), rgba: to_straight(pm) }
}

/// The project open in the editor, with its undo history.
struct Open {
    rec: ProjectRecord,
    store: Store,
}

/// Edits go to the open project's Store (live, undoable); other projects are edited on disk.
struct SessionWs<'a> {
    open: &'a mut Option<Open>,
}

impl SessionWs<'_> {
    fn current(&mut self, project: &str) -> Option<&mut Open> {
        self.open.as_mut().filter(|o| o.rec.id == project)
    }
}

fn record_of(o: &Open) -> ProjectRecord {
    let mut rec = o.rec.clone();
    rec.name = o.store.name.clone();
    rec.clips = o.store.clips.clone();
    rec.tracks = o.store.tracks.clone();
    rec.pool = o.store.pool.clone();
    rec.duration = project_end(&rec.clips);
    rec.clip_count = rec.clips.len();
    rec
}

fn save(o: &mut Open) {
    let mut rec = record_of(o);
    rec.updated_at = storage::now_ms();
    let _ = storage::put_project(&rec);
    o.rec = rec;
}

impl Workspace for SessionWs<'_> {
    fn record(&mut self, project: &str) -> R<ProjectRecord> {
        match self.current(project) {
            Some(o) => Ok(record_of(o)),
            None => FileWorkspace.record(project),
        }
    }
    fn edit(&mut self, project: &str, f: &mut dyn FnMut(&mut Store) -> R<Value>) -> R<Value> {
        let Some(o) = self.current(project) else { return FileWorkspace.edit(project, f) };
        let old = crate::model::time::env();
        crate::model::time::set_env(o.rec.env());
        let rev = o.store.rev;
        let r = f(&mut o.store);
        if r.is_ok() && o.store.rev != rev {
            save(o);
        }
        crate::model::time::set_env(old);
        r
    }
    fn open(&mut self, project: &str) -> R<()> {
        if self.current(project).is_some() {
            Ok(())
        } else {
            Err("On iPhone and iPad the project the person has open is the one you edit; other projects are edited directly.".into())
        }
    }
    fn live(&self) -> bool {
        self.open.is_some()
    }
}

#[derive(uniffi::Object)]
pub struct Engine {
    open: Mutex<Option<Open>>,
}

#[uniffi::export]
impl Engine {
    /// `data_dir`: where projects live (the app's Application Support folder).
    #[uniffi::constructor]
    pub fn new(data_dir: String, host: Arc<dyn MediaHost>) -> Arc<Self> {
        std::env::set_var("RAYNA_DATA", &data_dir);
        let _ = std::fs::create_dir_all(&data_dir);
        crate::media::set_host(host);
        // Index the fonts this device has, off the calling thread.
        std::thread::spawn(|| {
            crate::render::text::system_families();
        });
        Arc::new(Engine { open: Mutex::new(None) })
    }

    /// Run a Liemo tool (the MCP tools: list_projects, create_project, add_clip, update_clip,
    /// apply_effect, set_transition, render_frame, undo…). `args_json` is a JSON object.
    pub fn call(&self, tool: String, args_json: String) -> ToolResult {
        let args: Value = serde_json::from_str(&args_json).unwrap_or(Value::Object(Default::default()));
        let mut open = self.open.lock();
        let mut ws = SessionWs { open: &mut open };
        match tools::call(&tool, &args, &mut ws) {
            Ok(o) => ToolResult { ok: true, text: o.text, png: o.png },
            Err(e) => ToolResult { ok: false, text: e, png: None },
        }
    }

    /// The tools, as MCP tool definitions (name, description, inputSchema), for Liemo AI.
    pub fn tool_definitions(&self) -> String {
        serde_json::to_string(&tools::definitions()).unwrap_or_else(|_| "[]".into())
    }

    /// What the AI is told about Liemo.
    pub fn instructions(&self) -> String {
        "Liemo is a video editor. Projects have five tracks: T1 titles, V2 overlays and picture-in-picture, V1 main video, \
         A1 music, A2 voice. Add clips from list_library (footage, shader backgrounds, overlays, music) or imported media, \
         titles with add_text, looks with apply_effect, transitions with set_transition, and check your work with render_frame."
            .into()
    }

    /// Open a project for editing; returns its JSON (as get_project shows it).
    pub fn open_project(&self, id: String) -> ToolResult {
        let Some(rec) = storage::get_project(&id) else { return ToolResult { ok: false, text: format!("No project with id {id}"), png: None } };
        crate::model::time::set_env(rec.env());
        tools::ensure_media(&rec, true);
        let store = Store::new(rec.name.clone(), rec.clips.clone(), rec.tracks.clone(), rec.pool.clone());
        *self.open.lock() = Some(Open { rec, store });
        self.call("get_project".into(), format!("{{\"project\":\"{id}\"}}"))
    }

    /// Save, refresh the project's thumbnail and close it.
    pub fn close_project(&self) {
        if let Some(mut o) = self.open.lock().take() {
            save(&mut o);
            crate::model::time::set_env(o.rec.env());
            let mut pool = crate::render::frames::DecoderPool::new(640, 640);
            match crate::render::thumbs::stage_snapshot(&o.rec.clips, &o.rec.tracks, &mut pool, 384) {
                Some(t) => {
                    let _ = storage::save_jpeg(&t, &storage::project_thumb_path(&o.rec.id));
                }
                None => {
                    let _ = std::fs::remove_file(storage::project_thumb_path(&o.rec.id));
                }
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        self.open.lock().as_ref().is_some_and(|o| o.store.can_undo())
    }
    pub fn can_redo(&self) -> bool {
        self.open.lock().as_ref().is_some_and(|o| o.store.can_redo())
    }

    /// The open project's frame at `time`, its long side at most `max` px.
    pub fn render_frame(&self, time: f64, max: u32) -> Option<MediaFrame> {
        let open = self.open.lock();
        let o = open.as_ref()?;
        let env = o.rec.env();
        crate::model::time::set_env(env);
        // The compositor draws to whatever size the canvas is, so this serves the preview and
        // export (1080p, 4K) alike.
        let k = max as f32 / env.w.max(env.h);
        let (w, h) = (((env.w * k).round() as u32).max(2), ((env.h * k).round() as u32).max(2));
        let mut c = Canvas::from_pixmap(crate::render::canvas::pixmap(w, h));
        let mut pool = crate::render::frames::DecoderPool::new(w.max(1280), h.max(720));
        crate::render::compositor::render_frame(&mut c, &crate::render::compositor::Scene { clips: &o.store.clips, tracks: &o.store.tracks }, time, &mut pool);
        Some(frame_of(&c.into_pixmap()))
    }

    /// `frames` of the open project's sound from `start` seconds: 48 kHz interleaved stereo.
    pub fn render_audio(&self, start: f64, frames: u32) -> Vec<f32> {
        let mut out = vec![0f32; frames as usize * 2];
        if let Some(o) = self.open.lock().as_ref() {
            crate::audio::mix::mix(&mut out, &o.store.clips, &o.store.tracks, start, 48000.0, f64::NEG_INFINITY, f64::INFINITY, 1.0);
        }
        out
    }

    /// The open project's whole mix from `start` to `stop` seconds (48 kHz interleaved stereo),
    /// for export.
    pub fn render_audio_range(&self, start: f64, stop: f64) -> Vec<f32> {
        let open = self.open.lock();
        let Some(o) = open.as_ref() else { return vec![] };
        crate::audio::mix::render_range(&o.store.clips, &o.store.tracks, start, stop, &|| false).unwrap_or_default()
    }

    pub fn delete_project(&self, id: String) -> bool {
        storage::delete_project(&id).is_ok()
    }
    /// The copy's id.
    pub fn duplicate_project(&self, id: String) -> Option<String> {
        storage::duplicate_project(&id).ok().flatten()
    }

    /// A clip's loudness for drawing its waveform: `buckets` peak levels (0–1) across the clip.
    pub fn audio_peaks(&self, clip_id: String, buckets: u32) -> Vec<f32> {
        let open = self.open.lock();
        let Some(o) = open.as_ref() else { return vec![] };
        let Some(c) = o.store.clips.iter().find(|c| c.id == clip_id) else { return vec![] };
        let sr = 2000.0;
        let frames = ((c.dur * sr) as usize).max(1);
        let mut out = vec![0f32; frames * 2];
        // The clip alone, on an unmuted track, at its place on the timeline.
        let mut solo = c.clone();
        solo.track = "solo".into();
        crate::audio::mix::mix(&mut out, std::slice::from_ref(&solo), &[], c.start, sr, f64::NEG_INFINITY, f64::INFINITY, 1.0);
        let n = buckets.max(1) as usize;
        let per = (frames / n).max(1);
        let peaks: Vec<f32> = (0..n)
            .map(|b| {
                let lo = (b * per).min(frames);
                let hi = ((b + 1) * per).min(frames);
                (lo..hi).map(|i| out[i * 2].abs().max(out[i * 2 + 1].abs())).fold(0.0, f32::max)
            })
            .collect();
        let top = peaks.iter().cloned().fold(1e-4, f32::max);
        peaks.into_iter().map(|p| (p / top).sqrt()).collect()
    }

    /// One clip of the open project alone at `time` (its effects and motion, nothing else), on
    /// a transparent frame: freeze frames.
    pub fn render_clip(&self, clip_id: String, time: f64, max: u32) -> Option<MediaFrame> {
        let open = self.open.lock();
        let o = open.as_ref()?;
        let clip = o.store.clips.iter().find(|c| c.id == clip_id)?.clone();
        let env = o.rec.env();
        crate::model::time::set_env(env);
        let k = max as f32 / env.w.max(env.h);
        let (w, h) = (((env.w * k).round() as u32).max(2), ((env.h * k).round() as u32).max(2));
        let mut c = Canvas::from_pixmap(crate::render::canvas::pixmap(w, h));
        let mut pool = crate::render::frames::DecoderPool::new(w.max(1280), h.max(720));
        crate::render::compositor::draw_clip(&mut c, &clip, time, &mut pool);
        Some(frame_of(&c.into_pixmap()))
    }

    /// A text style drawn on a dark card (`props_json`: the text fields as get_project shows
    /// them), for the style gallery.
    pub fn text_preview(&self, props_json: String, width: u32, height: u32) -> Option<MediaFrame> {
        let mut t: crate::model::types::TextProps = serde_json::from_str(&props_json).ok()?;
        t.anim = crate::model::types::TextAnim::None;
        t.y = 0.0;
        let old = crate::model::time::env();
        crate::model::time::set_env(crate::model::time::Env::default());
        let (ww, hh) = (crate::model::time::W(), crate::model::time::H());
        let mut c = Canvas::from_pixmap(crate::render::canvas::pixmap(width, height));
        c.fill_rect_color(0.0, 0.0, width as f32, height as f32, crate::render::canvas::hex("#5b6170"));
        // Fit the style's size to the card.
        let k = (width as f32 / ww).min(height as f32 / hh) * 1.6;
        c.translate(width as f32 / 2.0, height as f32 / 2.0);
        c.scale(k, k);
        c.translate(-ww / 2.0, -hh / 2.0);
        crate::render::textclip::draw_text(&mut c, &t, 1.0, 1.0);
        crate::model::time::set_env(old);
        Some(frame_of(&c.into_pixmap()))
    }

    /// A library card picture: footage, shader backgrounds, stills, overlays, text styles.
    pub fn asset_thumb(&self, id: String) -> Option<MediaFrame> {
        crate::render::thumbs::asset_thumb_id(&id).map(|p| frame_of(&p))
    }
    pub fn effect_thumb(&self, id: String) -> MediaFrame {
        frame_of(&crate::render::thumbs::effect_thumb(&id))
    }
    pub fn transition_thumb(&self, id: String) -> MediaFrame {
        frame_of(&crate::render::thumbs::transition_thumb(&id))
    }
    /// Where a project's thumbnail JPEG is (it may not exist yet).
    pub fn project_thumb_path(&self, id: String) -> String {
        storage::project_thumb_path(&id).display().to_string()
    }
    /// Installed font families on this device.
    pub fn installed_fonts(&self) -> Vec<String> {
        crate::render::text::system_families().clone()
    }
    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").into()
    }
}
