// The tools an AI harness gets: one definition list (names, descriptions, JSON schemas) and one
// executor. Edits go through the editor's Store, so they place clips the way the UI does, keep
// undo history and report the same notes ("Moved 1 clip along to make room").
//
// Where the edit lands depends on the Workspace: the open editor when Liemo is running (the
// change shows up live), or the project files when it isn't.

use crate::fx::looks::look_def;
use crate::fx::registry::{effect_def, effects, ParamKind, EFFECT_GROUPS};
use crate::fx::transitions::{transition_def, TRANSITIONS};
use crate::media::import::{self, kind_of, new_media_id};
use crate::model::assets::{asset, font_def, images, music, overlays, scenes, shaders, text_presets, FONTS};
use crate::model::format::FORMATS;
use crate::model::store::{project_end, Store};
use crate::model::types::*;
use crate::storage::{self, ProjectRecord};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub type R<T> = Result<T, String>;

/// What a tool returns: text (usually JSON) and optionally a PNG.
pub struct ToolOut {
    pub text: String,
    pub png: Option<Vec<u8>>,
}
fn out(v: Value) -> R<ToolOut> {
    Ok(ToolOut { text: serde_json::to_string_pretty(&v).unwrap_or_default(), png: None })
}

pub trait Workspace {
    /// The project as it is now (including unsaved edits in an open editor).
    fn record(&mut self, project: &str) -> R<ProjectRecord>;
    /// Run an edit against the project's Store and keep the result.
    fn edit(&mut self, project: &str, f: &mut dyn FnMut(&mut Store) -> R<Value>) -> R<Value>;
    /// Show a project in the editor (needs the app).
    fn open(&mut self, _project: &str) -> R<()> {
        Err("Liemo isn't running, so there's no editor to open it in. Start Liemo and try again; every other tool works without it.".into())
    }
    /// A project was created, renamed or removed outside an editor.
    fn projects_changed(&mut self) {}
    /// Whether edits show up in a running app.
    fn live(&self) -> bool {
        false
    }
    /// Record from the microphone into the open editor: "start", "stop" or "status".
    fn record_audio(&mut self, _project: &str, _action: &str, _args: &Value) -> R<Value> {
        Err("Recording needs Liemo running with the project open in the editor (the microphone is the app's). Open it with open_project first.".into())
    }
}

/// Edits project files directly (Liemo not running, or the project isn't open).
pub struct FileWorkspace;

/// Put a project's imported media into the asset library, optionally with what playback and
/// export need (photo pixels, decoded sound).
pub fn ensure_media(rec: &ProjectRecord, payload: bool) {
    for id in &rec.pool {
        let Some(m) = storage::get_media(id) else { continue };
        let fresh = asset(id).is_none();
        if fresh {
            import::register_media(&m, storage::load_image(&storage::media_thumb_path(id)), None);
        }
        if payload && (fresh || (m.kind == AssetType::Photo && asset(id).and_then(|a| a.bitmap.clone()).is_none()) || (m.kind != AssetType::Photo && crate::media::audio::buffer(id).is_none())) {
            let (bitmap, _) = import::load_payload(&m);
            if let Some(b) = bitmap {
                crate::model::assets::update_asset(id, |a| a.bitmap = Some(std::sync::Arc::new(b)));
            }
        }
    }
}

impl Workspace for FileWorkspace {
    fn record(&mut self, project: &str) -> R<ProjectRecord> {
        storage::get_project(project).ok_or_else(|| format!("No project with id {project}"))
    }
    fn edit(&mut self, project: &str, f: &mut dyn FnMut(&mut Store) -> R<Value>) -> R<Value> {
        let mut rec = self.record(project)?;
        let old = crate::model::time::env();
        crate::model::time::set_env(rec.env());
        ensure_media(&rec, false);
        let mut store = rec.store(rec.pool.clone());
        let rev = store.rev;
        let r = f(&mut store);
        if r.is_ok() && store.rev != rev {
            rec.take(&store);
            rec.updated_at = storage::now_ms();
            storage::put_project(&rec).map_err(|e| format!("Couldn't save the project: {e}"))?;
            let mut pool = crate::render::frames::DecoderPool::new(640, 640);
            match crate::render::thumbs::stage_snapshot(&rec.clips, &rec.tracks, &mut pool, 384) {
                Some(t) => {
                    let _ = storage::save_jpeg(&t, &storage::project_thumb_path(&rec.id));
                }
                None => {
                    let _ = std::fs::remove_file(storage::project_thumb_path(&rec.id));
                }
            }
        }
        crate::model::time::set_env(old);
        let notes: Vec<String> = store.toasts.drain(..).collect();
        r.map(|v| with_notes(v, notes))
    }
}

/// Attach the editor's notes (what moved, what was refused) to a result.
pub fn with_notes(v: Value, notes: Vec<String>) -> Value {
    if notes.is_empty() {
        return v;
    }
    match v {
        Value::Object(mut m) => {
            m.insert("notes".into(), json!(notes));
            Value::Object(m)
        }
        other => json!({ "result": other, "notes": notes }),
    }
}

// ---------- Arguments ----------
fn s<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(|v| v.as_str())
}
fn f(a: &Value, k: &str) -> Option<f64> {
    a.get(k).and_then(|v| v.as_f64())
}
fn b(a: &Value, k: &str) -> Option<bool> {
    a.get(k).and_then(|v| v.as_bool())
}
fn req<'a>(a: &'a Value, k: &str) -> R<&'a str> {
    s(a, k).ok_or_else(|| format!("Missing \"{k}\""))
}

/// A project by id, or by name when the name is unique.
fn resolve(a: &Value) -> R<String> {
    let p = req(a, "project")?;
    if storage::get_project(p).is_some() {
        return Ok(p.to_string());
    }
    let all = storage::list_projects().map_err(|e| e.to_string())?;
    let hits: Vec<_> = all.iter().filter(|x| x.name.eq_ignore_ascii_case(p)).collect();
    match hits.len() {
        1 => Ok(hits[0].id.clone()),
        0 => Err(format!("No project called or with id \"{p}\". Use list_projects.")),
        _ => Err(format!("Several projects are called \"{p}\"; pass the id instead.")),
    }
}

fn clip_json(c: &Clip) -> Value {
    let mut v = serde_json::to_value(c).unwrap_or(Value::Null);
    if let Value::Object(m) = &mut v {
        m.insert("end".into(), json!(c.start + c.dur));
    }
    v
}

fn project_json(rec: &ProjectRecord) -> Value {
    let env = rec.env();
    let mut clips: Vec<&Clip> = rec.clips.iter().collect();
    clips.sort_by(|a, b| a.track.cmp(&b.track).then(a.start.total_cmp(&b.start)));
    let pool: Vec<Value> = rec
        .pool
        .iter()
        .filter_map(|id| storage::get_media(id))
        .map(|m| json!({ "id": m.id, "type": m.kind, "name": m.name, "duration": m.dur, "path": m.path, "width": m.width, "height": m.height }))
        .collect();
    json!({
        "id": rec.id,
        "name": rec.name,
        "format": rec.format_label(),
        "aspect": rec.aspect_key(),
        "fps": rec.fps,
        "exportResolution": rec.resolution,
        "background": rec.background,
        "canvas": { "width": env.w, "height": env.h, "note": "Clip x/y are offsets in these units from the centre; text y is from the centre too." },
        "duration": project_end(&rec.clips),
        "tracks": rec.tracks,
        "clips": clips.into_iter().map(clip_json).collect::<Vec<_>>(),
        "markers": rec.markers,
        "in": rec.in_point,
        "out": rec.out_point,
        "media": pool,
    })
}

// ---------- Tool definitions ----------
/// What each sound preset sets, so an AI can pick one or tweak from it.
fn sound_presets_doc() -> String {
    let list: Vec<String> = crate::audio::fx::PRESETS
        .iter()
        .map(|(k, l, f)| {
            if f.is_identity() {
                return format!("{k} ({l}: resets everything)");
            }
            let mut parts = vec![format!("speed {}", f.speed)];
            for (n, v) in [("reverb", f.reverb), ("bass", f.bass), ("muffle", f.muffle), ("pan8d", f.pan8d)] {
                if v > 0.0 {
                    parts.push(format!("{n} {v}"));
                }
            }
            if f.reverb > 0.0 {
                parts.push(format!("room {}", f.room));
            }
            format!("{k} ({l}: {})", parts.join(", "))
        })
        .collect();
    format!("Start from a preset; the other fields given then adjust it. {}", list.join("; "))
}
fn tool(name: &str, title: &str, desc: &str, props: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": desc,
        "inputSchema": { "type": "object", "properties": props, "required": required },
    })
}
fn project_prop() -> Value {
    json!({ "type": "string", "description": "Project id (from list_projects), or its exact name" })
}
fn clip_prop() -> Value {
    json!({ "type": "string", "description": "Clip id, e.g. \"c12\" (from get_project)" })
}

pub fn definitions() -> Vec<Value> {
    // On iOS the app exports through AVFoundation itself, so there's no export tool.
    let all = all_definitions();
    if cfg!(liemo_mobile) {
        all.into_iter().filter(|t| t["name"] != "export").collect()
    } else {
        all
    }
}

fn all_definitions() -> Vec<Value> {
    let text_props = json!({
        "content": { "type": "string", "description": "The words; use \\n for a second line" },
        "font": { "type": "string", "description": format!("A bundled font ({}) or any font installed on this Mac (list_library kind \"font\")", FONTS.iter().map(|f| f.family).collect::<Vec<_>>().join(", ")) },
        "size": { "type": "number", "description": "Font size in canvas units" },
        "weight": { "type": "number", "enum": [400, 500, 600, 700, 800] },
        "color": { "type": "string", "description": "#rrggbb" },
        "blend": { "type": "string", "enum": ["normal", "invert", "exclusion"], "description": "Invert flips each colour channel to the opposite of what's underneath, so the text stays readable on any footage; exclusion is the softer version of the same idea" },
        "align": { "type": "string", "enum": ["left", "center", "right"] },
        "y": { "type": "number", "description": "Vertical offset from the centre, in canvas units (negative is up)" },
        "animation": { "type": "string", "enum": ["none", "fade", "rise", "slide", "type", "pop", "bounce", "zoom", "blur", "wave", "glitch", "flicker"] },
        "box": { "type": "boolean", "description": "Dark rounded backing behind the text" },
        "outline": { "type": "string", "description": "Outline colour #rrggbb, or \"none\"" },
        "outlineWidth": { "type": "number", "description": "Outline thickness, 1–24 (default 6)" },
        "shadow": { "type": "string", "description": "Soft drop-shadow colour #rrggbb, or \"none\"" },
        "glow": { "type": "string", "description": "Neon glow colour #rrggbb, or \"none\"" },
        "highlight": { "type": "string", "description": "Rounded highlight behind each line, colour #rrggbb, or \"none\"" },
        "tiltX": { "type": "number", "description": "3D: tip the text back (positive) or forward, degrees −80…80" },
        "tiltY": { "type": "number", "description": "3D: swing the text to face right (positive) or left, degrees −80…80" },
    });
    let mut text_update = text_props.clone();
    let mut clip_fields = json!({
        "project": project_prop(),
        "clip": clip_prop(),
        "start": { "type": "number", "description": "Seconds on the timeline" },
        "duration": { "type": "number", "description": "Seconds" },
        "in": { "type": "number", "description": "Seconds into the source media where the clip begins (video/audio)" },
        "track": { "type": "string", "description": "Track id from get_project (T1 titles, V2 overlays, V1 main video, A1 music, A2 voice, plus any added)" },
        "name": { "type": "string" },
        "x": { "type": "number", "description": "Horizontal offset from the centre, canvas units" },
        "y": { "type": "number", "description": "Vertical offset from the centre, canvas units" },
        "scale": { "type": "number", "description": "1 is full size" },
        "rotation": { "type": "number", "description": "Degrees" },
        "opacity": { "type": "number", "minimum": 0, "maximum": 1 },
        "volume": { "type": "number", "minimum": 0, "maximum": 2, "description": "1 is as recorded; up to 2 (+6 dB)" },
        "fadeIn": { "type": "number", "description": "Seconds" },
        "fadeOut": { "type": "number", "description": "Seconds" },
        "effectsOn": { "type": "string", "enum": ["clip", "frame"], "description": "Effects stay on the clip's own shape, or fill the whole frame" },
        "blend": { "type": "string", "enum": BLEND_MODES.iter().map(|(k, _)| *k).collect::<Vec<_>>(), "description": "Picture clips: how the clip blends over what's below (lighter = add). On text clips, blend is the text style (normal, invert, exclusion)" },
        "mask": { "description": "A shape limiting where the clip shows (moves with the clip): { shape: ellipse|rect, x, y (centre offset), w, h, rot, feather, radius, invert }, or null to remove. Mask values can be keyframed as mask.x, mask.w…" },
        "duck": { "type": "number", "minimum": 0, "maximum": 1, "description": "Sound clips: dip by this much while voice plays on other tracks (music under a voice-over: 0.7)" },
        "normalize": { "type": "boolean", "description": "Set volume so the clip's sound averages about −16 dB with peaks under −1 dB" },
        "text": { "type": "object", "properties": text_update.take(), "description": "Text clips only" },
        "sound": {
            "type": "object",
            "description": "Audio clips only: rework the sound. Only the fields given change. A new speed stretches or shrinks the clip to cover the same part of the file.",
            "properties": {
                "preset": { "type": "string", "enum": crate::audio::fx::PRESETS.iter().map(|(k, _, _)| *k).collect::<Vec<_>>(), "description": sound_presets_doc() },
                "speed": { "type": "number", "minimum": 0.5, "maximum": 2.0, "description": "0.8 is slowed, 1.25 sped up" },
                "keepPitch": { "type": "boolean", "description": "Change speed without changing pitch. Default false: pitch follows speed, like a record" },
                "reverb": { "type": "number", "minimum": 0, "maximum": 1, "description": "Reverb level" },
                "room": { "type": "number", "minimum": 0, "maximum": 1, "description": "Reverb room size" },
                "bass": { "type": "number", "minimum": 0, "maximum": 1, "description": "Bass boost, up to +12 dB" },
                "muffle": { "type": "number", "minimum": 0, "maximum": 1, "description": "Low-pass: 1 sounds underwater" },
                "pan8d": { "type": "number", "minimum": 0, "maximum": 1, "description": "8D audio: sound circling the listener" },
                "eqLow": { "type": "number", "minimum": -12, "maximum": 12, "description": "EQ lows (~110 Hz shelf), dB" },
                "eqMid": { "type": "number", "minimum": -12, "maximum": 12, "description": "EQ mids (1 kHz bell), dB" },
                "eqHigh": { "type": "number", "minimum": -12, "maximum": 12, "description": "EQ highs (7 kHz shelf), dB" },
            },
        },
    });
    // Motion (picture clips), kept apart to stay inside json!'s recursion limit.
    let anims = json!(["none", "fade", "zoom-in", "zoom-out", "slide-left", "slide-right", "slide-up", "slide-down", "whip-left", "whip-right", "spin", "pop", "drop", "rise", "swing", "flip", "stretch", "shake"]);
    let motion = json!({
        "speed": { "type": "number", "minimum": 0.25, "maximum": 4, "description": "Playback speed of a picture clip (its sound follows); the clip's length changes to keep the same footage" },
        "reverse": { "type": "boolean", "description": "Play the clip backwards" },
        "freeze": { "type": "boolean", "description": "Hold one frame (the one at `in`) for the whole clip. To freeze mid-clip, split at that time and freeze the second part, or use set_keyframes on speed for a ramp" },
        "flipH": { "type": "boolean", "description": "Mirror left–right" },
        "flipV": { "type": "boolean", "description": "Flip upside down" },
        "animIn": { "type": "string", "enum": anims, "description": "How the clip enters" },
        "animInDur": { "type": "number", "description": "Seconds, default 0.5" },
        "animOut": { "type": "string", "enum": anims, "description": "How the clip leaves" },
        "animOutDur": { "type": "number", "description": "Seconds, default 0.5" },
        "animLoop": { "type": "string", "enum": ["none", "breathe", "float", "wiggle", "pulse", "spin", "shake", "sway"], "description": "Movement for the whole clip" },
        "animLoopSpeed": { "type": "number", "description": "0.25–4, default 1" },
    });
    if let (Some(m), Some(c)) = (motion.as_object(), clip_fields.as_object_mut()) {
        c.extend(m.clone());
    }
    vec![
        tool("list_projects", "List projects", "Every Liemo project: id, name, format, length, clip count and when it was edited.", json!({}), &[]),
        tool("create_project", "Create a project", "Make a new project. Pick a format (see list_formats) or give width/height for a custom size. template \"sample\" starts from the nature reel sample.",
            json!({
                "name": { "type": "string" },
                "format": { "type": "string", "enum": FORMATS.iter().map(|f| f.id).chain(["custom"]).collect::<Vec<_>>(), "description": "Defaults to youtube (16:9)" },
                "width": { "type": "integer", "description": "Custom format only, pixels" },
                "height": { "type": "integer", "description": "Custom format only, pixels" },
                "fps": { "type": "integer", "enum": [24, 25, 30, 50, 60] },
                "resolution": { "type": "integer", "enum": [720, 1080, 1440, 2160], "description": "Default export short side" },
                "background": { "type": "string", "description": "#rrggbb shown where no clip covers the frame" },
                "template": { "type": "string", "enum": ["blank", "sample"] },
                "open": { "type": "boolean", "description": "Open it in the editor (when Liemo is running). Default true." },
            }), &[]),
        tool("get_project", "Get a project", "Everything about a project: settings, canvas size, tracks, every clip (timing, transform, text, effects, transition, sound) and imported media.", json!({ "project": project_prop() }), &["project"]),
        tool("open_project", "Open in the editor", "Show a project in Liemo's editor so the user can watch edits happen. Needs Liemo running.", json!({ "project": project_prop() }), &["project"]),
        tool("set_project", "Change project settings", "Rename a project or change its background colour.",
            json!({ "project": project_prop(), "name": { "type": "string" }, "background": { "type": "string", "description": "#rrggbb" } }), &["project"]),
        tool("list_library", "List the library", "Built-in footage (scenes), stills, overlays, music, text styles, fonts, and the project's imported media. Use the ids with add_clip.",
            json!({ "project": { "type": "string", "description": "Optional: include this project's imported media" }, "kind": { "type": "string", "enum": ["scene", "image", "overlay", "music", "text", "font", "imported"] } }), &[]),
        tool("list_effects", "List effects", "Every effect with its group and parameters (Instagram filters, colour scale tints, shader filters, stylize, retro, distort, colour).",
            json!({ "group": { "type": "string", "enum": ["filters", "scale", "shader", "stylize", "retro", "distort", "color"] } }), &[]),
        tool("list_transitions", "List transitions", "Transitions a clip can start with, plus the fade options.", json!({}), &[]),
        tool("list_formats", "List formats", "Project formats with canvas size, ratio and usual upload size.", json!({}), &[]),
        tool("import_media", "Import media", "Add video, audio or image files (absolute paths) to a project's media pool, and optionally onto the timeline one after another.",
            json!({
                "project": project_prop(),
                "paths": { "type": "array", "items": { "type": "string" } },
                "addToTimeline": { "type": "boolean", "description": "Default true" },
                "start": { "type": "number", "description": "Where the first file lands, seconds. Default: end of the main track" },
                "track": { "type": "string", "description": "Track id from get_project" },
            }), &["project", "paths"]),
        tool("add_clip", "Add a clip", "Put a library asset or imported media on the timeline. Clips that would overlap are pushed along (insert edit).",
            json!({
                "project": project_prop(),
                "asset": { "type": "string", "description": "Asset id from list_library, e.g. sc-coast, mu-golden, ov-grain, or an imported media id" },
                "track": { "type": "string", "description": "Track id from get_project; default the asset's usual track" },
                "start": { "type": "number", "description": "Seconds; default 0" },
                "duration": { "type": "number", "description": "Seconds; default the asset's length" },
                "in": { "type": "number", "description": "Seconds into the source (video/audio)" },
            }), &["project", "asset"]),
        tool("add_text", "Add text", "Add a title, caption or lower third on the Titles track.",
            json!({
                "project": project_prop(),
                "style": { "type": "string", "enum": text_presets().iter().map(|a| a.id.clone()).collect::<Vec<_>>(), "description": "Starting style; default tx-title" },
                "content": { "type": "string" },
                "start": { "type": "number" },
                "duration": { "type": "number" },
                "font": text_props["font"], "size": text_props["size"], "weight": text_props["weight"], "color": text_props["color"],
                "align": text_props["align"], "y": text_props["y"], "animation": text_props["animation"], "box": text_props["box"],
                "outline": text_props["outline"], "outlineWidth": text_props["outlineWidth"], "shadow": text_props["shadow"], "glow": text_props["glow"], "highlight": text_props["highlight"],
                "tiltX": text_props["tiltX"], "tiltY": text_props["tiltY"],
                "blend": text_props["blend"],
            }), &["project", "content"]),
        tool("update_clip", "Update a clip", "Change any of a clip's timing, track, transform, opacity, volume, fades, name, text or sound (speed, reverb, slowed + reverb and other presets). Only the fields given change.", clip_fields, &["project", "clip"]),
        tool("set_keyframes", "Set keyframes", "Animate a clip property over time. Between keys the value moves in a straight line (linear), eases in and out (smooth) or stays put (hold); before the first key and after the last, the nearest key's value holds. Keys on speed make a speed ramp (the clip keeps its length; its footage runs at the keyed speed). An empty keys list removes the animation and leaves the property at its value at the clip's start.",
            json!({
                "project": project_prop(), "clip": clip_prop(),
                "property": { "type": "string", "description": "x, y (offsets from centre, canvas units), scale, rotation, opacity, volume, speed (picture clips, 0.25–4), or an effect's strength / numeric parameter as fx.<uid>.amount or fx.<uid>.<param> (uid from apply_effect or get_project)" },
                "keys": { "type": "array", "items": { "type": "object", "properties": {
                    "time": { "type": "number", "description": "Seconds on the timeline (inside the clip)" },
                    "value": { "type": "number" },
                    "ease": { "type": "string", "enum": ["linear", "smooth", "hold"], "description": "How the value travels to the next key; default linear" },
                }, "required": ["time", "value"] } },
            }), &["project", "clip", "property", "keys"]),
        tool("transcribe", "Transcribe the sound", "Listen to the project's sound (voice, music, video audio as mixed) with Apple's speech recognition on the Mac and return what's said as short caption cues with timeline times. Pass them to add_captions (fix wording first if needed). The user may be asked to allow speech recognition. Mac app only.",
            json!({ "project": project_prop() }), &["project"]),
        tool("add_captions", "Add captions", "Put captions on the Titles track as text clips, from an SRT or WebVTT file (path) or from cues (e.g. from transcribe). Styles: subtitle (the Caption text style), pop (big bold words that pop in), clean, boxed.",
            json!({
                "project": project_prop(),
                "path": { "type": "string", "description": "Absolute path to a .srt or .vtt file" },
                "cues": { "type": "array", "items": { "type": "object", "properties": { "start": { "type": "number" }, "end": { "type": "number" }, "text": { "type": "string" } }, "required": ["start", "end", "text"] } },
                "style": { "type": "string", "enum": ["subtitle", "pop", "clean", "boxed"], "description": "Default subtitle" },
            }), &["project"]),
        tool("export_captions", "Export captions", "Write the project's titles and captions as a subtitle file (SRT or WebVTT) for upload alongside the video.",
            json!({ "project": project_prop(), "path": { "type": "string", "description": "Where to write, ending .srt or .vtt; default ~/Downloads/<name>.srt" } }), &["project"]),
        tool("remove_clip", "Remove a clip", "Delete a clip from the timeline.", json!({ "project": project_prop(), "clip": clip_prop() }), &["project", "clip"]),
        tool("split_clip", "Split a clip", "Cut a clip in two at a time (seconds). Without a clip, splits the main-video clip under that time.",
            json!({ "project": project_prop(), "time": { "type": "number" }, "clip": clip_prop() }), &["project", "time"]),
        tool("duplicate_clip", "Duplicate a clip", "Copy a clip to just after itself.", json!({ "project": project_prop(), "clip": clip_prop() }), &["project", "clip"]),
        tool("apply_effect", "Apply an effect", "Add an effect to a visual clip (effects stack, top first). Returns its uid.",
            json!({
                "project": project_prop(), "clip": clip_prop(),
                "effect": { "type": "string", "description": "Effect id from list_effects, e.g. ig-clarendon, vhs, ascii" },
                "amount": { "type": "number", "minimum": 0, "maximum": 1, "description": "Strength/mix, default 1" },
                "params": { "type": "object", "description": "Parameter overrides (see list_effects)" },
            }), &["project", "clip", "effect"]),
        tool("update_effect", "Update an effect", "Change an applied effect's strength, on/off or parameters.",
            json!({ "project": project_prop(), "clip": clip_prop(), "uid": { "type": "string" }, "amount": { "type": "number" }, "enabled": { "type": "boolean" }, "params": { "type": "object" } }), &["project", "clip", "uid"]),
        tool("remove_effect", "Remove an effect", "Take an effect off a clip.", json!({ "project": project_prop(), "clip": clip_prop(), "uid": { "type": "string" } }), &["project", "clip", "uid"]),
        tool("set_transition", "Set a transition", "The transition into a clip (between touching clips it blends across the cut). \"none\" removes it. fadein/fadeout/both set fades instead.",
            json!({ "project": project_prop(), "clip": clip_prop(), "transition": { "type": "string", "description": "Id from list_transitions, or none" }, "duration": { "type": "number", "description": "Seconds" } }), &["project", "clip", "transition"]),
        tool("set_track", "Set a track", "Hide, mute or lock a track.",
            json!({ "project": project_prop(), "track": { "type": "string", "description": "Track id from get_project (T1 titles, V2 overlays, V1 main video, A1 music, A2 voice, plus any added)" }, "hidden": { "type": "boolean" }, "muted": { "type": "boolean" }, "locked": { "type": "boolean" } }), &["project", "track"]),
        tool("record_audio", "Record audio", "Record a voice-over or any sound from the Mac's microphone. start begins a take (the user talks or plays; with playAlong the timeline plays meanwhile so they can speak over the picture), stop ends it and the take lands on A2 (Voice & SFX) at the time it started, status says whether a take is running and which microphone is used. Needs Liemo running with the project open (open_project). Tell the user when you start so they know to speak, and stop when they say they're done.",
            json!({
                "project": project_prop(),
                "action": { "type": "string", "enum": ["start", "stop", "status"] },
                "at": { "type": "number", "description": "start: timeline seconds the take starts at; default the playhead" },
                "playAlong": { "type": "boolean", "description": "start: play the timeline during the take (headphones advised). Default: the user's setting" },
            }), &["project", "action"]),
        tool("undo", "Undo", "Undo the last edit in the open editor (needs Liemo running with the project open).", json!({ "project": project_prop() }), &["project"]),
        tool("redo", "Redo", "Redo in the open editor.", json!({ "project": project_prop() }), &["project"]),
        tool("render_frame", "Look at a frame", "Render the edit at a time and return it as a PNG, so you can check what it looks like.",
            json!({ "project": project_prop(), "time": { "type": "number", "description": "Seconds" }, "width": { "type": "integer", "description": "Long side in pixels, default 768" } }), &["project", "time"]),
        tool("export", "Export", "Render the project to a file: video (mp4/webm), gif, a still image, or audio. Returns the file path. Runs to completion.",
            json!({
                "project": project_prop(),
                "kind": { "type": "string", "enum": ["video", "gif", "image", "audio"], "description": "Default video" },
                "path": { "type": "string", "description": "Where to write; default ~/Downloads/<name>.<ext>" },
                "container": { "type": "string", "enum": ["mp4", "webm"] },
                "resolution": { "type": "integer", "enum": [720, 1080, 1440, 2160] },
                "aspect": { "type": "string", "description": "Output shape, e.g. 9:16; default the project's" },
                "framing": { "type": "string", "enum": ["crop", "blur", "bars"], "description": "When the output shape differs" },
                "fps": { "type": "integer" },
                "quality": { "type": "string", "enum": ["low", "medium", "high", "best"] },
                "audio": { "type": "boolean" },
                "start": { "type": "number" }, "end": { "type": "number" },
                "time": { "type": "number", "description": "image: which frame" },
                "imageFormat": { "type": "string", "enum": ["png", "jpeg"] },
                "audioFormat": { "type": "string", "enum": ["wav", "m4a"] },
                "gifWidth": { "type": "integer" }, "gifFps": { "type": "integer" },
            }), &["project"]),
    ]
}

/// Tools that read a snapshot and do heavy work (rendering, encoding): in the app these run off
/// the UI thread.
pub fn is_heavy(name: &str) -> bool {
    matches!(name, "render_frame" | "export" | "transcribe")
}

// ---------- Executor ----------
pub fn call(name: &str, a: &Value, ws: &mut dyn Workspace) -> R<ToolOut> {
    match name {
        "list_projects" => {
            let mut all = storage::list_projects().map_err(|e| e.to_string())?;
            all.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            out(json!({
                "live": ws.live(),
                "projects": all.iter().map(|p| json!({ "id": p.id, "name": p.name, "format": p.label, "aspect": p.aspect, "duration": p.duration, "clips": p.clip_count, "edited": storage::relative_time(p.updated_at) })).collect::<Vec<_>>(),
            }))
        }
        "create_project" => {
            let format = s(a, "format").unwrap_or("youtube");
            let custom = format == "custom" || (a.get("width").is_some() && a.get("height").is_some());
            let mut np = crate::storage::NewProject::of(if custom { "custom" } else { format });
            if custom {
                let (w, h) = (f(a, "width").ok_or("Custom needs width")? as u32, f(a, "height").ok_or("Custom needs height")? as u32);
                if !(64..=8192).contains(&w) || !(64..=8192).contains(&h) || w.max(h) as f32 / w.min(h) as f32 > 4.0 {
                    return Err("Custom sizes are 64–8192 px per side and no wider than 4:1".into());
                }
                np.size = Some([w, h]);
            } else if !FORMATS.iter().any(|x| x.id == format) {
                return Err(format!("Unknown format {format}; see list_formats"));
            }
            if let Some(v) = f(a, "fps") {
                np.fps = v as u32;
            }
            if let Some(v) = f(a, "resolution") {
                np.resolution = v as u32;
            }
            if let Some(v) = s(a, "background") {
                np.background = v.into();
            }
            let sample = s(a, "template") == Some("sample");
            let id = crate::storage::create_project(sample, s(a, "name").map(String::from), &np).map_err(|e| e.to_string())?;
            ws.projects_changed();
            let opened = if b(a, "open").unwrap_or(true) && ws.live() { ws.open(&id).is_ok() } else { false };
            let rec = ws.record(&id)?;
            let mut v = project_json(&rec);
            if let Value::Object(m) = &mut v {
                m.insert("openedInEditor".into(), json!(opened));
            }
            out(v)
        }
        "get_project" => {
            let id = resolve(a)?;
            out(project_json(&ws.record(&id)?))
        }
        "open_project" => {
            let id = resolve(a)?;
            ws.open(&id)?;
            out(json!({ "opened": id }))
        }
        "set_project" => {
            let id = resolve(a)?;
            if let Some(bg) = s(a, "background") {
                let mut rec = storage::get_project(&id).ok_or("No such project")?;
                rec.background = bg.into();
                storage::put_project(&rec).map_err(|e| e.to_string())?;
            }
            let name = s(a, "name").map(String::from);
            let v = ws.edit(&id, &mut |st| {
                if let Some(n) = &name {
                    st.name = n.clone();
                    st.changed();
                }
                Ok(json!({ "name": st.name }))
            })?;
            ws.projects_changed();
            out(v)
        }
        "list_library" => {
            let kind = s(a, "kind");
            let want = |k: &str| kind.is_none() || kind == Some(k);
            let mut m = Map::new();
            let entry = |x: &crate::model::assets::Asset| json!({ "id": x.id, "name": x.name, "about": x.sub, "duration": x.dur });
            if want("scene") {
                m.insert("footage".into(), json!(scenes().iter().map(entry).collect::<Vec<_>>()));
                m.insert("shader_backgrounds".into(), json!(shaders().iter().map(entry).collect::<Vec<_>>()));
            }
            if want("image") {
                m.insert("stills".into(), json!(images().iter().map(entry).collect::<Vec<_>>()));
            }
            if want("overlay") {
                m.insert("overlays".into(), json!(overlays().iter().map(|x| json!({ "id": x.id, "name": x.name, "about": x.sub, "blend": x.blend })).collect::<Vec<_>>()));
            }
            if want("music") {
                m.insert("music".into(), json!(music().iter().map(|x| json!({ "id": x.id, "name": x.name, "about": x.sub, "bpm": x.bpm, "duration": x.dur })).collect::<Vec<_>>()));
            }
            if want("text") {
                m.insert("textStyles".into(), json!(text_presets().iter().map(|x| json!({ "id": x.id, "name": x.name, "about": x.sub, "text": x.text })).collect::<Vec<_>>()));
            }
            if want("font") {
                m.insert("fonts".into(), json!(FONTS.iter().map(|f| json!({ "family": f.family, "role": f.role, "weight": f.weight })).collect::<Vec<_>>()));
                m.insert("installedFonts".into(), json!(crate::render::text::system_families()));
            }
            if want("imported") {
                if let Ok(id) = resolve(a) {
                    let rec = ws.record(&id)?;
                    m.insert("imported".into(), json!(rec.pool.iter().filter_map(|i| storage::get_media(i)).map(|x| json!({ "id": x.id, "type": x.kind, "name": x.name, "duration": x.dur, "path": x.path })).collect::<Vec<_>>()));
                }
            }
            out(Value::Object(m))
        }
        "list_effects" => {
            let group = s(a, "group");
            let list: Vec<Value> = effects()
                .iter()
                .filter(|e| group.map(|g| format!("{:?}", e.group).eq_ignore_ascii_case(g)).unwrap_or(true))
                .map(|e| {
                    let params: Vec<Value> = e
                        .params
                        .iter()
                        .map(|p| match &p.kind {
                            ParamKind::Range { min, max, step, unit } => json!({ "key": p.key, "label": p.label, "type": "number", "min": min, "max": max, "step": step, "unit": unit, "default": e.defaults.get(p.key) }),
                            ParamKind::Select(o) => json!({ "key": p.key, "label": p.label, "type": "select", "options": o.iter().map(|(v, _)| *v).collect::<Vec<_>>(), "default": e.defaults.get(p.key) }),
                            ParamKind::Color => json!({ "key": p.key, "label": p.label, "type": "color", "default": e.defaults.get(p.key) }),
                            ParamKind::Toggle => json!({ "key": p.key, "label": p.label, "type": "boolean", "default": e.defaults.get(p.key) }),
                            ParamKind::File(ext) => json!({ "key": p.key, "label": p.label, "type": "file", "extension": ext, "description": format!("Absolute path to a .{ext} file") }),
                        })
                        .collect();
                    let group = EFFECT_GROUPS.iter().find(|(g, _)| *g == e.group).map(|(_, l)| *l).unwrap_or("");
                    let mut v = json!({ "id": e.id, "name": e.name, "group": group });
                    if !params.is_empty() {
                        v["params"] = json!(params);
                    }
                    if look_def(e.id).is_some() || e.css.is_some() {
                        v["amount"] = json!("strength 0–1");
                    }
                    v
                })
                .collect();
            out(json!({ "effects": list }))
        }
        "list_transitions" => out(json!({
            "transitions": TRANSITIONS.iter().map(|t| json!({ "id": t.id, "name": t.name, "group": format!("{:?}", t.group).to_lowercase() })).collect::<Vec<_>>(),
            "fades": [{ "id": "fadein", "name": "Fade in" }, { "id": "fadeout", "name": "Fade out" }, { "id": "both", "name": "Fade both" }],
        })),
        "list_formats" => out(json!({
            "formats": FORMATS.iter().map(|x| json!({ "id": x.id, "name": x.name, "aspect": x.aspect, "canvas": [x.w, x.h], "uploadSize": x.pixels, "bestFor": x.best_for, "length": x.length })).collect::<Vec<_>>(),
            "custom": "format \"custom\" with width and height in pixels",
        })),
        "import_media" => {
            let id = resolve(a)?;
            let paths: Vec<PathBuf> = a.get("paths").and_then(|v| v.as_array()).map(|v| v.iter().filter_map(|p| p.as_str()).map(PathBuf::from).collect()).unwrap_or_default();
            if paths.is_empty() {
                return Err("Give one or more absolute file paths".into());
            }
            let rec = ws.record(&id)?;
            let env = rec.env();
            let mut imported = vec![];
            let mut failed = vec![];
            // Folders bring in the media inside them (four levels deep, by name).
            let mut expanded = vec![];
            for p in paths {
                if p.is_dir() {
                    fn walk(d: &std::path::Path, depth: u32, out: &mut Vec<PathBuf>) {
                        let Ok(rd) = std::fs::read_dir(d) else { return };
                        let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| !p.file_name().and_then(|n| n.to_str()).unwrap_or(".").starts_with('.')).collect();
                        v.sort();
                        for p in v {
                            if p.is_dir() {
                                if depth < 4 {
                                    walk(&p, depth + 1, out);
                                }
                            } else if kind_of(&p).is_some() {
                                out.push(p);
                            }
                        }
                    }
                    walk(&p, 0, &mut expanded);
                } else {
                    expanded.push(p);
                }
            }
            for p in expanded {
                if kind_of(&p).is_none() || !p.exists() {
                    failed.push(json!({ "path": p, "error": if p.exists() { "not a video, audio or image file" } else { "file not found" } }));
                    continue;
                }
                match import::read_file(&p, new_media_id()) {
                    Ok(imp) => {
                        if let Some(t) = &imp.thumb {
                            let _ = storage::save_jpeg(t, &storage::media_thumb_path(&imp.record.id));
                        }
                        let _ = storage::put_media(&imp.record);
                        import::register_media(&imp.record, imp.thumb, imp.bitmap);
                        if imp.record.kind != AssetType::Photo {
                            let _ = import::load_payload(&imp.record);
                        }
                        imported.push(imp.record);
                    }
                    Err(e) => failed.push(json!({ "path": p, "error": e.to_string() })),
                }
            }
            let place = b(a, "addToTimeline").unwrap_or(true);
            let start = f(a, "start");
            let track = s(a, "track").map(String::from);
            let recs = imported.clone();
            let v = ws.edit(&id, &mut |st| {
                let mut cursor = start.unwrap_or_else(|| st.clips.iter().filter(|c| c.track == "V1").fold(0.0, |m, c| m.max(c.start + c.dur)));
                let mut added = vec![];
                for r in &recs {
                    if !st.pool.contains(&r.id) {
                        st.pool.push(r.id.clone());
                        st.changed();
                    }
                    if place {
                        if let Some(c) = st.add_clip_exact(&r.id, track.as_deref(), cursor) {
                            cursor = c.start + c.dur;
                            added.push(clip_json(&c));
                        }
                    }
                }
                Ok(json!({ "media": recs.iter().map(|r| json!({ "id": r.id, "type": r.kind, "name": r.name, "duration": r.dur })).collect::<Vec<_>>(), "clips": added }))
            });
            let _ = env;
            let mut v = v?;
            if !failed.is_empty() {
                v["failed"] = json!(failed);
            }
            out(v)
        }
        "add_clip" => {
            let id = resolve(a)?;
            let asset_id = req(a, "asset")?.to_string();
            let (track, start, dur, inp) = (s(a, "track").map(String::from), f(a, "start"), f(a, "duration"), f(a, "in"));
            out(ws.edit(&id, &mut |st| {
                if asset(&asset_id).is_none() {
                    return Err(format!("No asset {asset_id}; see list_library"));
                }
                let c = st.add_clip_exact(&asset_id, track.as_deref(), start.unwrap_or(0.0)).ok_or("The track is locked")?;
                if dur.is_some() || inp.is_some() {
                    st.update_clip(&c.id, |k| {
                        if let Some(d) = dur {
                            k.dur = d.max(crate::model::store::min_clip());
                        }
                        if let Some(i) = inp {
                            k.in_ = i.max(0.0);
                        }
                    });
                    st.settle(&c.id, false);
                }
                for n in overlaps(st, &c.id) {
                    st.toast(n);
                }
                Ok(json!({ "clip": clip_json(st.clip(&c.id).unwrap_or(&c)) }))
            })?)
        }
        "add_text" => {
            let id = resolve(a)?;
            let style = s(a, "style").unwrap_or("tx-title").to_string();
            let args = a.clone();
            out(ws.edit(&id, &mut |st| {
                if asset(&style).and_then(|x| x.text.clone()).is_none() {
                    return Err(format!("No text style {style}"));
                }
                let c = st.add_clip_exact(&style, Some("T1"), f(&args, "start").unwrap_or(0.0)).ok_or("The Titles track is locked")?;
                if let Some(d) = f(&args, "duration") {
                    st.update_clip(&c.id, |k| k.dur = d.max(0.1));
                }
                apply_text(st, &c.id, &args)?;
                let content = s(&args, "content").unwrap_or("").to_string();
                let first = content.lines().next().unwrap_or("").chars().take(40).collect::<String>();
                if !first.is_empty() {
                    st.update_clip(&c.id, |k| k.name = first);
                }
                st.settle(&c.id, false);
                for n in overlaps(st, &c.id) {
                    st.toast(n);
                }
                Ok(json!({ "clip": clip_json(st.clip(&c.id).unwrap()) }))
            })?)
        }
        "update_clip" => {
            let id = resolve(a)?;
            if b(a, "normalize") == Some(true) {
                ensure_media(&ws.record(&id)?, true);
            }
            let cid = req(a, "clip")?.to_string();
            let args = a.clone();
            out(ws.edit(&id, &mut |st| {
                let c = st.clip(&cid).cloned().ok_or_else(|| format!("No clip {cid}"))?;
                if st.track(&c.track).locked {
                    return Err(format!("{} is locked", st.track(&c.track).name));
                }
                st.commit();
                let mut moved = false;
                if let Some(t) = s(&args, "track") {
                    if !crate::model::store::accepts(&st.tracks, t, c.kind) {
                        return Err(format!("Track {t} doesn't take {:?} clips", c.kind));
                    }
                }
                st.update_clip(&cid, |k| {
                    if let Some(v) = f(&args, "start") {
                        k.start = crate::model::time::snap_frame(v.max(0.0));
                        moved = true;
                    }
                    if let Some(v) = f(&args, "duration") {
                        k.dur = crate::model::time::snap_frame(v.max(crate::model::store::min_clip()));
                        moved = true;
                    }
                    if let Some(v) = f(&args, "in") {
                        k.in_ = v.max(0.0);
                    }
                    if let Some(v) = s(&args, "track") {
                        k.track = v.into();
                        moved = true;
                    }
                    if let Some(v) = s(&args, "name") {
                        k.name = v.into();
                    }
                    if let Some(v) = f(&args, "x") {
                        k.x = v;
                    }
                    if let Some(v) = f(&args, "y") {
                        k.y = v;
                    }
                    if let Some(v) = f(&args, "scale") {
                        k.scale = v.max(0.01);
                    }
                    if let Some(v) = f(&args, "rotation") {
                        k.rot = v;
                    }
                    if let Some(v) = f(&args, "opacity") {
                        k.opacity = v.clamp(0.0, 1.0);
                    }
                    if let Some(v) = f(&args, "volume") {
                        k.volume = v.clamp(0.0, MAX_VOLUME);
                    }
                    // Text clips take blend as their text style (normal / invert / exclusion).
                    if let Some(v) = s(&args, "blend").filter(|_| k.kind != AssetType::Text) {
                        k.blend = (v != "normal").then(|| v.to_string());
                    }
                    if let Some(v) = f(&args, "duck") {
                        k.duck = v.clamp(0.0, 1.0);
                    }
                    match args.get("mask") {
                        Some(Value::Null) => {
                            k.mask = None;
                            k.keys.retain(|p, _| !p.starts_with("mask."));
                        }
                        Some(m @ Value::Object(_)) => {
                            let mut cur = serde_json::to_value(k.mask.clone().unwrap_or_default()).unwrap_or(Value::Null);
                            if let (Value::Object(c), Value::Object(n)) = (&mut cur, m) {
                                for (key, v) in n {
                                    c.insert(key.clone(), v.clone());
                                }
                            }
                            if let Ok(mm) = serde_json::from_value::<Mask>(cur) {
                                k.mask = Some(mm);
                            }
                        }
                        _ => {}
                    }
                    if let Some(v) = f(&args, "fadeIn") {
                        k.fade_in = v.max(0.0);
                    }
                    if let Some(v) = f(&args, "fadeOut") {
                        k.fade_out = v.max(0.0);
                    }
                    if let Some(v) = s(&args, "effectsOn") {
                        k.fx_scope = Some(if v == "clip" { FxScope::Clip } else { FxScope::Frame });
                    }
                    // Motion: speed, direction, flips, entrance and exit.
                    let motion_keys = ["speed", "reverse", "freeze", "flipH", "flipV", "animIn", "animInDur", "animOut", "animOutDur", "animLoop", "animLoopSpeed"];
                    if k.kind != AssetType::Audio && motion_keys.iter().any(|key| args.get(*key).is_some()) {
                        let mut m = k.motion.clone().unwrap_or_default();
                        if let Some(v) = f(&args, "speed") {
                            let v = v.clamp(0.25, 4.0);
                            // The same stretch of source, played faster or slower, unless a length was given.
                            if args.get("duration").is_none() {
                                k.dur = crate::model::time::snap_frame((k.dur * m.speed / v).max(crate::model::store::min_clip()));
                                moved = true;
                            }
                            m.speed = v;
                        }
                        if let Some(v) = b(&args, "reverse") {
                            m.reverse = v;
                        }
                        if let Some(v) = b(&args, "freeze") {
                            m.freeze = v;
                        }
                        if let Some(v) = b(&args, "flipH") {
                            m.flip_h = v;
                        }
                        if let Some(v) = b(&args, "flipV") {
                            m.flip_v = v;
                        }
                        if let Some(v) = s(&args, "animIn") {
                            m.anim_in = if v == "none" { String::new() } else { v.into() };
                        }
                        if let Some(v) = f(&args, "animInDur") {
                            m.anim_in_dur = v.clamp(0.1, 3.0);
                        }
                        if let Some(v) = s(&args, "animOut") {
                            m.anim_out = if v == "none" { String::new() } else { v.into() };
                        }
                        if let Some(v) = f(&args, "animOutDur") {
                            m.anim_out_dur = v.clamp(0.1, 3.0);
                        }
                        if let Some(v) = s(&args, "animLoop") {
                            m.anim_loop = if v == "none" { String::new() } else { v.into() };
                        }
                        if let Some(v) = f(&args, "animLoopSpeed") {
                            m.anim_loop_speed = v.clamp(0.25, 4.0);
                        }
                        k.motion = if m == crate::model::types::Motion::default() { None } else { Some(m) };
                    }
                    k.fade_in = k.fade_in.min(k.dur / 2.0);
                    k.fade_out = k.fade_out.min(k.dur / 2.0);
                });
                if b(&args, "normalize") == Some(true) {
                    let c = st.clip(&cid).cloned().ok_or("clip gone")?;
                    let v = crate::audio::mix::normalize_volume(&c).ok_or("This clip's sound isn't loaded, or it's silent")?;
                    st.update_clip(&cid, |k| {
                        k.volume = v;
                        k.keys.remove("volume");
                    });
                }
                if let Some(snd) = args.get("sound").filter(|v| v.is_object()) {
                    if c.kind != AssetType::Audio {
                        return Err("Sound effects work on audio clips (imported audio and recordings)".into());
                    }
                    let preset = match s(snd, "preset") {
                        Some(p) => Some(crate::audio::fx::PRESETS.iter().find(|(k, _, _)| *k == p).map(|(_, _, f)| f.clone()).ok_or_else(|| format!("No sound preset {p}"))?),
                        None => None,
                    };
                    let snd = snd.clone();
                    st.set_sound(&cid, move |x| {
                        if let Some(p) = preset {
                            *x = p;
                        }
                        let g = |k: &str| snd.get(k).and_then(|v| v.as_f64());
                        if let Some(v) = g("speed") {
                            x.speed = v.clamp(0.5, 2.0);
                        }
                        if let Some(v) = snd.get("keepPitch").and_then(|v| v.as_bool()) {
                            x.keep_pitch = v;
                        }
                        for (k, field) in [("reverb", &mut x.reverb), ("room", &mut x.room), ("bass", &mut x.bass), ("muffle", &mut x.muffle), ("pan8d", &mut x.pan8d)] {
                            if let Some(v) = g(k) {
                                *field = v.clamp(0.0, 1.0);
                            }
                        }
                        for (k, field) in [("eqLow", &mut x.eq_low), ("eqMid", &mut x.eq_mid), ("eqHigh", &mut x.eq_high)] {
                            if let Some(v) = g(k) {
                                *field = v.clamp(-12.0, 12.0);
                            }
                        }
                    });
                    moved = true;
                }
                if let Some(t) = args.get("text") {
                    apply_text(st, &cid, t)?;
                } else if c.kind == AssetType::Text && ["content", "font", "size", "weight", "color", "align", "animation", "blend", "box", "outline", "outlineWidth", "shadow", "glow", "highlight", "tiltX", "tiltY"].iter().any(|k| args.get(*k).is_some()) {
                    // Text fields given flat, as add_text takes them (y stays the clip's offset).
                    let mut t = args.clone();
                    if let Some(o) = t.as_object_mut() {
                        o.remove("y");
                    }
                    apply_text(st, &cid, &t)?;
                }
                let old_end = c.start + c.dur;
                if moved {
                    st.settle(&cid, false);
                    // A speed-up shortens the clip: pull what came after it back, so no gap opens
                    // (a slow-down already pushes them along).
                    if args.get("speed").is_some() && args.get("duration").is_none() {
                        let end = st.clip(&cid).map(|k| k.start + k.dur).unwrap_or(old_end);
                        let gap = old_end - end;
                        if gap > 1e-6 {
                            let after: Vec<String> = st.clips.iter().filter(|k| k.track == c.track && k.id != cid && k.start >= old_end - 1e-6).map(|k| k.id.clone()).collect();
                            for k in after {
                                st.update_clip(&k, |k| k.start = crate::model::time::snap_frame((k.start - gap).max(0.0)));
                            }
                        }
                    }
                }
                Ok(json!({ "clip": clip_json(st.clip(&cid).unwrap()) }))
            })?)
        }
        "set_keyframes" => {
            let id = resolve(a)?;
            let cid = req(a, "clip")?.to_string();
            let prop = match req(a, "property")? {
                "rotation" => "rot".to_string(),
                p => p.to_string(),
            };
            let list = a.get("keys").and_then(|v| v.as_array()).ok_or("Missing \"keys\" (an array, empty to remove)")?.clone();
            out(ws.edit(&id, &mut |st| {
                let c = st.clip(&cid).cloned().ok_or_else(|| format!("No clip {cid}"))?;
                if c.base(&prop).is_none() && prop != "speed" {
                    return Err(format!("{cid} has no numeric property \"{prop}\". Use x, y, scale, rotation, opacity, volume, speed, or fx.<uid>.amount / fx.<uid>.<param>."));
                }
                if prop == "speed" && (c.kind.is_audio() || c.kind == AssetType::Text) {
                    return Err("Speed ramps are for video and animated picture clips. Audio clips change speed with update_clip sound.".into());
                }
                let mut keys = vec![];
                for k in &list {
                    let t = f(k, "time").ok_or("Each key needs a time")?;
                    let mut v = f(k, "value").ok_or("Each key needs a value")?;
                    if prop == "speed" {
                        v = v.clamp(0.25, 4.0);
                    }
                    let ease = match s(k, "ease") {
                        Some("smooth") => crate::model::keys::Ease::Smooth,
                        Some("hold") => crate::model::keys::Ease::Hold,
                        _ => crate::model::keys::Ease::Linear,
                    };
                    keys.push(crate::model::keys::Key { t: (t - c.start).clamp(0.0, c.dur), v, ease });
                }
                keys.sort_by(|a, b| a.t.total_cmp(&b.t));
                keys.dedup_by(|a, b| (a.t - b.t).abs() < crate::model::keys::SAME);
                st.commit();
                let p2 = prop.clone();
                st.update_clip(&cid, move |k| {
                    if keys.is_empty() {
                        if let Some(v) = k.value(&p2, 0.0) {
                            if p2 != "speed" {
                                k.set_base(&p2, v);
                            }
                        }
                        k.keys.remove(&p2);
                    } else {
                        k.keys.insert(p2.clone(), keys);
                    }
                });
                let c = st.clip(&cid).cloned().unwrap();
                let mut notes = vec![];
                if prop == "speed" {
                    if let Some(a) = asset(&c.asset).filter(|a| a.media.is_some()) {
                        if c.src_span() > a.dur - c.in_ + 1e-3 {
                            notes.push(format!("The ramp runs past the end of the footage ({:.2} s of {:.2} s); the last frame holds. Shorten the clip or lower the speeds.", c.src_span(), a.dur - c.in_));
                        }
                    }
                }
                Ok(with_notes(clip_json(&c), notes))
            })?)
        }
        "transcribe" => {
            let id = resolve(a)?;
            let cues = auto_cues(&ws.record(&id)?)?;
            out(json!({ "cues": cues.iter().map(|c| json!({ "start": (c.start * 100.0).round() / 100.0, "end": (c.end * 100.0).round() / 100.0, "text": c.text })).collect::<Vec<_>>() }))
        }
        "add_captions" => {
            let id = resolve(a)?;
            let mut cues: Vec<crate::model::captions::Cue> = vec![];
            if let Some(p) = s(a, "path") {
                let text = std::fs::read(p).map_err(|e| format!("Couldn't read {p}: {e}"))?;
                cues = crate::model::captions::parse(&String::from_utf8_lossy(&text));
                if cues.is_empty() {
                    return Err(format!("{p} has no SRT or WebVTT captions in it"));
                }
            }
            if let Some(list) = a.get("cues").and_then(|v| v.as_array()) {
                for c in list {
                    let (Some(start), Some(end), Some(text)) = (f(c, "start"), f(c, "end"), s(c, "text")) else { return Err("Each cue needs start, end and text".into()) };
                    cues.push(crate::model::captions::Cue { start, end: end.max(start + 0.1), text: text.to_string() });
                }
            }
            if cues.is_empty() {
                return Err("Give a path or cues (transcribe makes cues from the sound)".into());
            }
            let style = s(a, "style").unwrap_or("subtitle").to_string();
            out(ws.edit(&id, &mut |st| {
                let n = st.add_captions(&cues, &style);
                if n == 0 {
                    return Err("The Titles track is locked. Unlock it with set_track.".into());
                }
                let added: Vec<Value> = st.selected_clips().iter().map(clip_json).collect();
                Ok(json!({ "added": n, "clips": added }))
            })?)
        }
        "export_captions" => {
            let id = resolve(a)?;
            let rec = ws.record(&id)?;
            let cues = crate::model::captions::cues_from_clips(&rec.clips);
            if cues.is_empty() {
                return Err("This project has no titles or captions to export".into());
            }
            let path = match s(a, "path") {
                Some(p) => std::path::PathBuf::from(p),
                None => dirs::download_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default()).join(format!("{}.srt", rec.name.replace(['/', ':'], "-"))),
            };
            let vtt = path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("vtt")).unwrap_or(false);
            let body = if vtt { crate::model::captions::to_vtt(&cues) } else { crate::model::captions::to_srt(&cues) };
            std::fs::write(&path, body).map_err(|e| format!("Couldn't write {}: {e}", path.display()))?;
            out(json!({ "path": path, "captions": cues.len(), "format": if vtt { "vtt" } else { "srt" } }))
        }
        "remove_clip" => {
            let id = resolve(a)?;
            let cid = req(a, "clip")?.to_string();
            out(ws.edit(&id, &mut |st| {
                st.clip(&cid).ok_or_else(|| format!("No clip {cid}"))?;
                st.select(Some(cid.clone()));
                let n = st.clips.len();
                st.remove();
                Ok(json!({ "removed": st.clips.len() < n }))
            })?)
        }
        "split_clip" => {
            let id = resolve(a)?;
            let t = f(a, "time").ok_or("Missing \"time\"")?;
            let cid = s(a, "clip").map(String::from);
            out(ws.edit(&id, &mut |st| {
                st.select(cid.clone());
                st.time = t;
                let before: Vec<String> = st.clips.iter().map(|c| c.id.clone()).collect();
                st.split();
                let new: Vec<Value> = st.clips.iter().filter(|c| !before.contains(&c.id)).map(clip_json).collect();
                if new.is_empty() {
                    return Err("Nothing to split there: the time must be inside the clip".into());
                }
                Ok(json!({ "newClip": new[0] }))
            })?)
        }
        "duplicate_clip" => {
            let id = resolve(a)?;
            let cid = req(a, "clip")?.to_string();
            out(ws.edit(&id, &mut |st| {
                st.clip(&cid).ok_or_else(|| format!("No clip {cid}"))?;
                st.select(Some(cid.clone()));
                st.duplicate();
                let sel = st.selected.clone().unwrap_or_default();
                Ok(json!({ "clip": st.clip(&sel).map(clip_json) }))
            })?)
        }
        "apply_effect" => {
            let id = resolve(a)?;
            let cid = req(a, "clip")?.to_string();
            let eid = req(a, "effect")?.to_string();
            let amount = f(a, "amount");
            let params = a.get("params").cloned();
            out(ws.edit(&id, &mut |st| {
                effect_def(&eid).ok_or_else(|| format!("No effect {eid}; see list_effects"))?;
                let n = st.clip(&cid).ok_or_else(|| format!("No clip {cid}"))?.effects.len();
                st.apply_effect(&eid, Some(&cid));
                let c = st.clip(&cid).unwrap();
                if c.effects.len() == n {
                    return Err("Effects go on video, image, overlay and text clips".into());
                }
                let uid = c.effects.last().unwrap().uid.clone();
                st.update_effect(&cid, &uid, |e| {
                    if let Some(v) = amount {
                        e.amount = v.clamp(0.0, 1.0);
                    }
                    if let Some(Value::Object(p)) = &params {
                        for (k, v) in p {
                            if let Ok(pv) = serde_json::from_value::<ParamValue>(v.clone()) {
                                e.params.insert(k.clone(), pv);
                            }
                        }
                    }
                });
                Ok(json!({ "uid": uid, "effects": st.clip(&cid).unwrap().effects }))
            })?)
        }
        "update_effect" => {
            let id = resolve(a)?;
            let (cid, uid) = (req(a, "clip")?.to_string(), req(a, "uid")?.to_string());
            let args = a.clone();
            out(ws.edit(&id, &mut |st| {
                let c = st.clip(&cid).ok_or_else(|| format!("No clip {cid}"))?;
                if !c.effects.iter().any(|e| e.uid == uid) {
                    return Err(format!("Clip {cid} has no effect {uid}"));
                }
                st.commit();
                st.update_effect(&cid, &uid, |e| {
                    if let Some(v) = f(&args, "amount") {
                        e.amount = v.clamp(0.0, 1.0);
                    }
                    if let Some(v) = b(&args, "enabled") {
                        e.enabled = v;
                    }
                    if let Some(Value::Object(p)) = args.get("params") {
                        for (k, v) in p {
                            if let Ok(pv) = serde_json::from_value::<ParamValue>(v.clone()) {
                                e.params.insert(k.clone(), pv);
                            }
                        }
                    }
                });
                Ok(json!({ "effects": st.clip(&cid).unwrap().effects }))
            })?)
        }
        "remove_effect" => {
            let id = resolve(a)?;
            let (cid, uid) = (req(a, "clip")?.to_string(), req(a, "uid")?.to_string());
            out(ws.edit(&id, &mut |st| {
                st.clip(&cid).ok_or_else(|| format!("No clip {cid}"))?;
                st.remove_effect(&cid, &uid);
                Ok(json!({ "effects": st.clip(&cid).unwrap().effects }))
            })?)
        }
        "set_transition" => {
            let id = resolve(a)?;
            let cid = req(a, "clip")?.to_string();
            let tid = req(a, "transition")?.to_string();
            let dur = f(a, "duration");
            out(ws.edit(&id, &mut |st| {
                st.clip(&cid).ok_or_else(|| format!("No clip {cid}"))?;
                if tid == "none" {
                    st.commit();
                    st.update_clip(&cid, |c| c.transition = None);
                } else {
                    if !matches!(tid.as_str(), "fadein" | "fadeout" | "both") && transition_def(&tid).is_none() {
                        return Err(format!("No transition {tid}; see list_transitions"));
                    }
                    st.apply_transition(&tid, Some(&cid));
                    if let Some(d) = dur {
                        st.update_clip(&cid, |c| {
                            if let Some(t) = c.transition.as_mut() {
                                t.dur = d.clamp(0.05, c.dur);
                            }
                        });
                    }
                }
                let c = st.clip(&cid).unwrap();
                Ok(json!({ "transition": c.transition, "fadeIn": c.fade_in, "fadeOut": c.fade_out }))
            })?)
        }
        "set_track" => {
            let id = resolve(a)?;
            let tid = req(a, "track")?.to_string();
            let args = a.clone();
            out(ws.edit(&id, &mut |st| {
                let t = st.tracks.iter().find(|t| t.id == tid).cloned().ok_or_else(|| format!("No track {tid}"))?;
                for (key, cur) in [("hidden", t.hidden), ("muted", t.muted), ("locked", t.locked)] {
                    if let Some(v) = b(&args, key) {
                        if v != cur {
                            st.toggle_track(&tid, key);
                        }
                    }
                }
                if let Some(n) = s(&args, "name") {
                    st.rename_track(&tid, n);
                }
                if let Some(m) = s(&args, "move") {
                    st.move_track(&tid, if m == "up" { -1 } else { 1 });
                }
                Ok(json!({ "track": st.tracks.iter().find(|t| t.id == tid), "order": st.tracks.iter().map(|t| t.id.clone()).collect::<Vec<_>>() }))
            })?)
        }
        "add_track" => {
            let id = resolve(a)?;
            let kind = match req(a, "kind")? {
                "titles" | "text" => TrackKind::Text,
                "audio" => TrackKind::Audio,
                _ => TrackKind::Overlay,
            };
            let name = s(a, "name").map(String::from);
            out(ws.edit(&id, &mut |st| {
                let tid = st.add_track(kind, name.clone());
                Ok(json!({ "track": st.tracks.iter().find(|t| t.id == tid), "order": st.tracks.iter().map(|t| t.id.clone()).collect::<Vec<_>>() }))
            })?)
        }
        "remove_track" => {
            let id = resolve(a)?;
            let tid = req(a, "track")?.to_string();
            out(ws.edit(&id, &mut |st| {
                if !st.tracks.iter().any(|t| t.id == tid) {
                    return Err(format!("No track {tid}"));
                }
                if !st.remove_track(&tid) {
                    return Err(st.toasts.pop().unwrap_or_else(|| "That track stays".into()));
                }
                Ok(json!({ "removed": tid, "order": st.tracks.iter().map(|t| t.id.clone()).collect::<Vec<_>>() }))
            })?)
        }
        "set_markers" => {
            let id = resolve(a)?;
            let args = a.clone();
            out(ws.edit(&id, &mut |st| {
                st.commit();
                if let Some(list) = args.get("markers").and_then(|v| v.as_array()) {
                    let mut ms: Vec<Marker> = list
                        .iter()
                        .filter_map(|m| f(m, "time").map(|t| Marker { t: crate::model::time::snap_frame(t.max(0.0)), name: s(m, "name").unwrap_or("").to_string(), color: s(m, "color").unwrap_or("#ffb020").to_string() }))
                        .collect();
                    ms.sort_by(|a, b| a.t.total_cmp(&b.t));
                    st.markers = ms;
                }
                for (key, is_in) in [("in", true), ("out", false)] {
                    match args.get(key) {
                        Some(Value::Null) => if is_in { st.in_out.0 = None } else { st.in_out.1 = None },
                        Some(v) => if let Some(t) = v.as_f64() {
                            let t = crate::model::time::snap_frame(t.max(0.0));
                            if is_in { st.in_out.0 = Some(t) } else { st.in_out.1 = Some(t) }
                        },
                        None => {}
                    }
                }
                st.changed();
                Ok(json!({ "markers": st.markers, "in": st.in_out.0, "out": st.in_out.1, "range": st.in_out_range() }))
            })?)
        }
        "detach_audio" => {
            let id = resolve(a)?;
            let cid = req(a, "clip")?.to_string();
            out(ws.edit(&id, &mut |st| {
                if st.clip(&cid).map(|c| c.kind != AssetType::Video).unwrap_or(true) {
                    return Err(format!("{cid} isn't a video clip"));
                }
                match st.detach_audio(&cid) {
                    Some(aid) => Ok(json!({ "audio": st.clip(&aid).map(clip_json), "video": st.clip(&cid).map(clip_json) })),
                    None => Err(st.toasts.pop().unwrap_or_else(|| "Couldn't detach the audio".into())),
                }
            })?)
        }
        "record_audio" => {
            let id = resolve(a)?;
            let action = req(a, "action")?.to_string();
            out(ws.record_audio(&id, &action, a)?)
        }
        "undo" | "redo" => {
            let id = resolve(a)?;
            if !ws.live() {
                return Err("Undo needs Liemo running with the project open (history lives in the editor).".into());
            }
            let undo = name == "undo";
            out(ws.edit(&id, &mut |st| {
                let can = if undo { st.can_undo() } else { st.can_redo() };
                if !can {
                    return Err(format!("Nothing to {name}"));
                }
                if undo { st.undo() } else { st.redo() }
                Ok(json!({ "done": name }))
            })?)
        }
        "render_frame" => {
            let id = resolve(a)?;
            let rec = ws.record(&id)?;
            let t = f(a, "time").unwrap_or(0.0);
            let long = f(a, "width").unwrap_or(768.0).clamp(64.0, 2048.0) as u32;
            let png = render_png(&rec, t, long)?;
            Ok(ToolOut { text: serde_json::to_string(&json!({ "project": rec.id, "time": t, "duration": project_end(&rec.clips) })).unwrap(), png: Some(png) })
        }
        #[cfg(not(liemo_mobile))]
        "export" => {
            let id = resolve(a)?;
            let rec = ws.record(&id)?;
            out(run_export(&rec, a)?)
        }
        _ => Err(format!("Unknown tool {name}")),
    }
}

/// Clips on the same track that `id` now overlaps (they keep playing underneath it).
fn overlaps(st: &Store, id: &str) -> Vec<String> {
    let Some(c) = st.clip(id) else { return vec![] };
    st.clips
        .iter()
        .filter(|k| k.id != c.id && k.track == c.track && k.start < c.start + c.dur - 1e-6 && c.start < k.start + k.dur - 1e-6)
        .map(|k| format!("Overlaps {} \"{}\" ({:.2}–{:.2} s) on {}; shorten one with update_clip if that's not intended.", k.id, k.name, k.start, k.start + k.dur, k.track))
        .collect()
}

fn apply_text(st: &mut Store, cid: &str, a: &Value) -> R<()> {
    if st.clip(cid).and_then(|c| c.text.as_ref()).is_none() {
        return Err("That clip isn't a text clip".into());
    }
    if let Some(v) = s(a, "font") {
        if !FONTS.iter().any(|f| f.family == v) && !crate::render::text::is_installed(v) {
            return Err(format!("\"{v}\" isn't a bundled font or installed on this Mac. list_library with kind \"font\" lists them.").into());
        }
    }
    let a = a.clone();
    st.update_text(cid, |t| {
        if let Some(v) = s(&a, "content") {
            t.content = v.into();
        }
        if let Some(v) = s(&a, "font") {
            t.font = font_def(v).family.into();
            if a.get("weight").is_none() {
                t.weight = font_def(v).weight;
            }
        }
        if let Some(v) = f(&a, "size") {
            t.size = v.clamp(8.0, 400.0);
        }
        if let Some(v) = f(&a, "weight") {
            t.weight = v;
        }
        if let Some(v) = s(&a, "color") {
            t.color = v.into();
        }
        if let Some(v) = s(&a, "align") {
            t.align = match v {
                "left" => TextAlign::Left,
                "right" => TextAlign::Right,
                _ => TextAlign::Center,
            };
        }
        if let Some(v) = f(&a, "y") {
            t.y = v;
        }
        if let Some(v) = s(&a, "animation") {
            t.anim = match v {
                "fade" => TextAnim::Fade,
                "rise" => TextAnim::Rise,
                "slide" => TextAnim::Slide,
                "type" => TextAnim::Type,
                "pop" => TextAnim::Pop,
                "bounce" => TextAnim::Bounce,
                "zoom" => TextAnim::Zoom,
                "blur" => TextAnim::Blur,
                "wave" => TextAnim::Wave,
                "glitch" => TextAnim::Glitch,
                "flicker" => TextAnim::Flicker,
                _ => TextAnim::None,
            };
        }
        if let Some(v) = s(&a, "blend") {
            t.blend = match v {
                "invert" => TextBlend::Invert,
                "exclusion" => TextBlend::Exclusion,
                _ => TextBlend::Normal,
            };
        }
        if let Some(v) = b(&a, "box") {
            t.boxed = v;
        }
        // Colours for the extra styles; "" or "none" takes the style off.
        let colour = |k: &str| s(&a, k).map(|v| if v.is_empty() || v == "none" { None } else { Some(v.to_string()) });
        if let Some(v) = colour("outline") {
            t.outline = v;
        }
        if let Some(v) = f(&a, "outlineWidth") {
            t.outline_width = v.clamp(1.0, 24.0);
        }
        if let Some(v) = colour("shadow") {
            t.shadow = v;
        }
        if let Some(v) = colour("glow") {
            t.glow = v;
        }
        if let Some(v) = colour("highlight") {
            t.highlight = v;
        }
        if let Some(v) = f(&a, "tiltX") {
            t.tilt_x = v.clamp(-80.0, 80.0);
        }
        if let Some(v) = f(&a, "tiltY") {
            t.tilt_y = v.clamp(-80.0, 80.0);
        }
    });
    Ok(())
}

/// Render one frame of a project to PNG bytes, long side `long` px.
pub fn render_png(rec: &ProjectRecord, t: f64, long: u32) -> R<Vec<u8>> {
    let old = crate::model::time::env();
    let env = rec.env();
    crate::model::time::set_env(env);
    ensure_media(rec, true);
    let mut c = crate::render::canvas::Canvas::new(env.w as u32, env.h as u32);
    let mut pool = crate::render::frames::DecoderPool::new(1920, 1920);
    crate::render::compositor::render_frame(&mut c, &crate::render::compositor::Scene { clips: &rec.clips, tracks: &rec.tracks }, t, &mut pool);
    crate::model::time::set_env(old);
    let pm = c.into_pixmap();
    let k = long as f32 / env.w.max(env.h);
    let pm = crate::render::canvas::resample_area(&pm, ((env.w * k).round() as u32).max(1), ((env.h * k).round() as u32).max(1));
    let rgba = crate::render::canvas::to_straight(&pm);
    let mut bytes = vec![];
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(&rgba, pm.width(), pm.height(), image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Listen to a project's sound and turn what's said into caption cues.
#[cfg(all(not(liemo_mobile), target_os = "macos"))]
fn auto_cues(rec: &ProjectRecord) -> R<Vec<crate::model::captions::Cue>> {
    ensure_media(rec, true);
    let old = crate::model::time::env();
    crate::model::time::set_env(rec.env());
    let mix = crate::audio::mix::voice_range(&rec.clips, &rec.tracks, 0.0, project_end(&rec.clips));
    crate::model::time::set_env(old);
    let mix = mix.ok_or("There's no voice or video sound in this project to caption (music is left out)")?;
    let words = crate::speech_mac::transcribe_mix(&mix)?;
    if words.is_empty() {
        return Err("No speech found in this project's sound".into());
    }
    Ok(crate::model::captions::group_words(&words))
}
#[cfg(not(all(not(liemo_mobile), target_os = "macos")))]
fn auto_cues(_: &ProjectRecord) -> R<Vec<crate::model::captions::Cue>> {
    Err("Auto captions run in the Mac app. On iPhone and iPad use the Captions tool; elsewhere pass cues or an SRT/VTT path.".into())
}

#[cfg(not(liemo_mobile))]
fn run_export(rec: &ProjectRecord, a: &Value) -> R<Value> {
    use crate::export::*;
    let mut st = Settings { aspect: rec.aspect_key(), resolution: rec.resolution, fps: rec.fps, ..Default::default() };
    st.kind = match s(a, "kind").unwrap_or("video") {
        "gif" => Kind::Gif,
        "image" => Kind::Image,
        "audio" => Kind::Audio,
        _ => Kind::Video,
    };
    if let Some(v) = s(a, "container") {
        st.container = if v == "webm" { Container::Webm } else { Container::Mp4 };
    }
    if let Some(v) = f(a, "resolution") {
        st.resolution = v as u32;
    }
    if let Some(v) = s(a, "aspect") {
        st.aspect = v.into();
    }
    if let Some(v) = s(a, "framing") {
        st.framing = match v {
            "crop" => Framing::Crop,
            "bars" => Framing::Bars,
            _ => Framing::Blur,
        };
    }
    if let Some(v) = f(a, "fps") {
        st.fps = v as u32;
    }
    if let Some(v) = s(a, "quality") {
        st.quality = match v {
            "low" => Quality::Low,
            "medium" => Quality::Medium,
            "best" => Quality::Best,
            _ => Quality::High,
        };
    }
    if let Some(v) = b(a, "audio") {
        st.audio = v;
    }
    if let Some(v) = s(a, "imageFormat") {
        st.image_png = v != "jpeg";
    }
    if let Some(v) = s(a, "audioFormat") {
        st.audio_wav = v != "m4a";
    }
    if let Some(v) = f(a, "gifWidth") {
        st.gif_width = v as u32;
    }
    if let Some(v) = f(a, "gifFps") {
        st.gif_fps = v as u32;
    }
    let end = project_end(&rec.clips);
    let ext = extension(&st);
    let path = match s(a, "path") {
        Some(p) => {
            let p = PathBuf::from(p);
            if p.extension().is_none() { p.with_extension(ext) } else { p }
        }
        None => {
            let dir = dirs::download_dir().unwrap_or_else(|| PathBuf::from("."));
            let stem: String = rec.name.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '-' } else { c }).collect();
            unique_path(&dir, if stem.trim().is_empty() { "Liemo export" } else { stem.trim() }, ext)
        }
    };
    let old = crate::model::time::env();
    crate::model::time::set_env(rec.env());
    ensure_media(rec, true);
    let job = Job {
        settings: st,
        start: f(a, "start").unwrap_or(0.0),
        end: f(a, "end").unwrap_or(end),
        image_time: f(a, "time").unwrap_or(0.0),
        clips: rec.clips.clone(),
        tracks: rec.tracks.clone(),
        path,
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let report = |_: f64, _: String| {};
    let r = run(&job, &Progress { cancel: &cancel, report: &report });
    crate::model::time::set_env(old);
    let o = r.map_err(|e| e.to_string())?;
    Ok(json!({ "path": o.path, "bytes": o.bytes, "seconds": o.seconds, "size": o.size, "withAudio": o.with_audio }))
}

use image::ImageEncoder;
