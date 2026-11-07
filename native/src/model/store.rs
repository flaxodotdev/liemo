// The editor's state and edit actions: clips, tracks, selection, undo/redo, insert/overwrite
// placement. Actions that want to tell the user something push to `toasts`; the UI shows them.

use super::assets::{asset, has_asset};
use super::time::{snap_frame, tc, FPS, H, W};
use super::types::*;
use crate::fx::registry::{effect_def, make_clip_effect};
use crate::fx::transitions::transition_def;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMode {
    Insert,
    Overwrite,
}

#[derive(Clone)]
struct Snapshot {
    clips: Vec<Clip>,
    selected: Option<String>,
}

/// The shortest a clip can be trimmed to: three frames.
pub fn min_clip() -> f64 {
    3.0 / FPS()
}
const EPS: f64 = 1e-6;

pub fn track_kinds(k: TrackKind) -> &'static [AssetType] {
    use AssetType::*;
    match k {
        TrackKind::Text => &[Text],
        TrackKind::Overlay => &[Overlay, Image, Photo, Video, Scene],
        TrackKind::Video => &[Scene, Video, Image, Photo],
        TrackKind::Audio => &[Music, Audio],
    }
}
pub fn default_track(t: AssetType) -> &'static str {
    use AssetType::*;
    match t {
        Text => "T1",
        Overlay => "V2",
        Scene | Video | Image | Photo => "V1",
        Music => "A1",
        Audio => "A2",
    }
}
pub fn track_height(k: TrackKind) -> f32 {
    match k {
        TrackKind::Text => 44.0,
        TrackKind::Overlay => 48.0,
        TrackKind::Video => 64.0,
        TrackKind::Audio => 52.0,
    }
}

pub fn default_tracks() -> Vec<Track> {
    let t = |id: &str, name: &str, kind| Track { id: id.into(), name: name.into(), kind, hidden: false, muted: false, locked: false };
    vec![
        t("T1", "Titles", TrackKind::Text),
        t("V2", "Overlays", TrackKind::Overlay),
        t("V1", "Main video", TrackKind::Video),
        t("A1", "Music", TrackKind::Audio),
        t("A2", "Voice & SFX", TrackKind::Audio),
    ]
}

static SEQ: AtomicU64 = AtomicU64::new(1);
pub fn next_id() -> String {
    format!("c{}", SEQ.fetch_add(1, Ordering::Relaxed))
}
/// Keep new clip ids above the ones in a loaded project.
pub fn sync_clip_seq(clips: &[Clip]) {
    for c in clips {
        if let Ok(n) = c.id[1..].parse::<u64>() {
            SEQ.fetch_max(n + 1, Ordering::Relaxed);
        }
    }
}

pub fn project_end(clips: &[Clip]) -> f64 {
    clips.iter().fold(0.0, |m, c| m.max(c.start + c.dur))
}

fn fit_fades(mut c: Clip) -> Clip {
    c.fade_in = c.fade_in.min(c.dur / 2.0);
    c.fade_out = c.fade_out.min(c.dur / 2.0);
    c
}

/// Overwrite edit: clips on the same track under `id` are trimmed, split or removed so nothing
/// overlaps it. This is what NLEs do when you drop a clip onto another.
pub fn resolve_overlaps(clips: &[Clip], id: &str) -> Vec<Clip> {
    let Some(m) = clips.iter().find(|c| c.id == id) else { return clips.to_vec() };
    let (s, e, track) = (m.start, m.start + m.dur, m.track.clone());
    let mut out = vec![];
    for c in clips {
        let (cs, ce) = (c.start, c.start + c.dur);
        if c.id == id || c.track != track || ce <= s + EPS || cs >= e - EPS {
            out.push(c.clone());
            continue;
        }
        if cs >= s - EPS && ce <= e + EPS {
            continue; // fully covered
        }
        if cs < s && ce > e {
            // covered in the middle: keep both ends
            out.push(fit_fades(Clip { dur: s - cs, fade_out: 0.0, ..c.clone() }));
            out.push(fit_fades(Clip { id: next_id(), start: e, in_: c.in_ + (e - cs), dur: ce - e, fade_in: 0.0, ..c.clone() }));
        } else if cs < s {
            out.push(fit_fades(Clip { dur: s - cs, ..c.clone() }));
        } else {
            out.push(fit_fades(Clip { start: e, in_: c.in_ + (e - cs), dur: ce - e, ..c.clone() }));
        }
    }
    out
}

/// Insert edit: make room for `id` without cutting anything. With `snap_into_place`, a clip
/// dropped inside another lands before it (first half) or after it (second half). Clips it would
/// overlap shift right, as little as needed; gaps absorb the shift.
pub fn push_overlaps(clips: &[Clip], id: &str, snap_into_place: bool) -> (Vec<Clip>, usize) {
    let Some(m0) = clips.iter().find(|c| c.id == id) else { return (clips.to_vec(), 0) };
    let mut m = m0.clone();
    let mut others: Vec<&Clip> = clips.iter().filter(|c| c.track == m.track && c.id != id).collect();
    others.sort_by(|a, b| a.start.total_cmp(&b.start));
    if snap_into_place {
        if let Some(host) = others.iter().find(|k| m.start > k.start + EPS && m.start < k.start + k.dur - EPS) {
            m.start = if m.start - host.start < host.dur / 2.0 { host.start } else { host.start + host.dur };
        }
    }
    let mut shifted: Vec<(String, f64)> = vec![];
    let mut cursor = m.start + m.dur;
    for k in &others {
        if k.start + k.dur <= m.start + EPS {
            continue;
        }
        if k.start < m.start - EPS {
            cursor = cursor.max(k.start + k.dur);
            continue;
        }
        if k.start < cursor - EPS {
            shifted.push((k.id.clone(), cursor));
            cursor += k.dur;
        } else {
            cursor = k.start + k.dur;
        }
    }
    let n = shifted.len();
    let out = clips
        .iter()
        .map(|c| {
            if c.id == id {
                m.clone()
            } else if let Some((_, s)) = shifted.iter().find(|(i, _)| *i == c.id) {
                Clip { start: snap_frame(*s), ..c.clone() }
            } else {
                c.clone()
            }
        })
        .collect();
    (out, n)
}

/// First start time at or after `from` where a clip of `dur` fits on `track` without overlapping.
pub fn free_slot(clips: &[Clip], track: &str, from: f64, dur: f64) -> f64 {
    let mut start = from;
    let mut moved = true;
    while moved {
        moved = false;
        for k in clips {
            if k.track == track && k.start < start + dur - EPS && k.start + k.dur > start + EPS {
                start = k.start + k.dur;
                moved = true;
            }
        }
    }
    start
}

pub fn make_clip(tracks: &[Track], time: f64, asset_id: &str, track_id: Option<&str>, start: Option<f64>) -> Option<Clip> {
    let a = asset(asset_id)?;
    let track = match track_id {
        Some(t) if accepts(tracks, t, a.kind) => t.to_string(),
        _ => default_track(a.kind).to_string(),
    };
    Some(Clip {
        id: next_id(),
        asset: asset_id.into(),
        kind: a.kind,
        name: a.name.clone(),
        track,
        start: snap_frame(start.unwrap_or(time).max(0.0)),
        dur: if a.kind == AssetType::Music { a.dur.min(24.0) } else { a.dur },
        in_: 0.0,
        x: 0.0,
        y: 0.0,
        scale: 1.0,
        rot: 0.0,
        opacity: if a.kind == AssetType::Overlay && a.blend != Some("source-over") { 0.7 } else { 1.0 },
        transition: None,
        fx_scope: None,
        effects: vec![],
        fade_in: 0.0,
        fade_out: 0.0,
        volume: if a.kind == AssetType::Music { 0.7 } else { 1.0 },
        text: a.text.clone().map(fit_text),
    })
}

/// Text styles are designed for a 1280×720 canvas. On narrower canvases they shrink a little and
/// keep their place relative to the frame's height.
fn fit_text(mut t: TextProps) -> TextProps {
    let k = (W() / 1280.0).sqrt().min(1.0) as f64;
    t.size = (t.size * k).round();
    t.y = (t.y * H() as f64 / 720.0).round();
    t
}

pub fn accepts(tracks: &[Track], track_id: &str, t: AssetType) -> bool {
    tracks.iter().find(|x| x.id == track_id).map(|tr| track_kinds(tr.kind).contains(&t)).unwrap_or(false)
}

/// The "Nature reel" sample, used as a project template.
pub fn sample_clips() -> Vec<Clip> {
    let tracks = default_tracks();
    let mut clips = vec![];
    let mut put = |a: &str, track: &str, start: f64, dur: f64, f: &dyn Fn(&mut Clip)| {
        let mut c = make_clip(&tracks, 0.0, a, Some(track), Some(start)).unwrap();
        c.dur = dur;
        f(&mut c);
        clips.push(c);
    };
    let tr = |id: &str, dur: f64| Some(TransitionRef { id: id.into(), dur });
    put("sc-coast", "V1", 0.0, 6.0, &|c| c.fade_in = 0.5);
    put("sc-drive", "V1", 6.0, 5.0, &|c| {
        c.effects = vec![make_clip_effect("cool", 0.8).unwrap()];
        c.transition = tr("whip", 0.6);
    });
    put("sc-aurora", "V1", 11.0, 6.0, &|c| {
        c.effects = vec![make_clip_effect("vivid", 0.6).unwrap()];
        c.transition = tr("dissolve", 1.0);
    });
    put("sc-studio", "V1", 17.0, 4.0, &|c| {
        c.fade_out = 0.8;
        c.effects = vec![make_clip_effect("ascii-color", 1.0).unwrap()];
        c.transition = tr("glitch", 0.5);
    });
    put("ov-leak", "V2", 0.0, 6.0, &|c| {
        c.opacity = 0.55;
        c.fade_out = 1.0;
    });
    put("ov-grain", "V2", 6.0, 15.0, &|c| c.opacity = 0.45);
    put("tx-title", "T1", 0.5, 4.0, &|c| c.fade_out = 0.4);
    put("tx-lower", "T1", 6.5, 4.0, &|c| c.fade_out = 0.3);
    put("tx-quote", "T1", 12.0, 4.0, &|c| c.fade_out = 0.5);
    put("tx-poster", "T1", 17.5, 3.0, &|c| c.fade_out = 0.3);
    put("mu-golden", "A1", 0.0, 21.0, &|c| c.fade_out = 1.5);
    clips
}

pub struct Store {
    pub name: String,
    pub time: f64,
    pub playing: bool,
    pub zoom: f64,
    pub snap: bool,
    pub looping: bool,
    pub guides: bool,
    pub edit_mode: EditMode,
    pub selected: Option<String>,
    pub tracks: Vec<Track>,
    pub clips: Vec<Clip>,
    pub pool: Vec<String>,
    past: Vec<Snapshot>,
    future: Vec<Snapshot>,
    pub editing: bool,
    pub clipboard: Option<Clip>,
    pub toasts: Vec<String>,
    /// Bumped whenever something that is saved changes (clips, tracks, name, pool).
    pub rev: u64,
}

impl Store {
    pub fn new(name: String, clips: Vec<Clip>, tracks: Vec<Track>, pool: Vec<String>) -> Self {
        sync_clip_seq(&clips);
        Store {
            name, time: 0.0, playing: false, zoom: 60.0, snap: true, looping: false, guides: false, edit_mode: EditMode::Insert,
            selected: None, tracks: if tracks.is_empty() { default_tracks() } else { tracks }, clips, pool,
            past: vec![], future: vec![], editing: false, clipboard: None, toasts: vec![], rev: 0,
        }
    }
    pub fn toast(&mut self, s: impl Into<String>) {
        self.toasts.push(s.into());
    }
    pub fn changed(&mut self) {
        self.rev += 1;
    }
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
    pub fn track(&self, id: &str) -> &Track {
        self.tracks.iter().find(|t| t.id == id).unwrap_or(&self.tracks[0])
    }
    pub fn clip(&self, id: &str) -> Option<&Clip> {
        self.clips.iter().find(|c| c.id == id)
    }
    pub fn selected_clip(&self) -> Option<&Clip> {
        self.selected.as_deref().and_then(|id| self.clip(id))
    }
    pub fn end(&self) -> f64 {
        project_end(&self.clips)
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot { clips: self.clips.clone(), selected: self.selected.clone() }
    }
    fn push_past(&mut self) {
        let s = self.snapshot();
        self.past.push(s);
        if self.past.len() > 120 {
            self.past.remove(0);
        }
        self.future.clear();
    }
    /// Fit a clip into its track according to the edit mode, and say what moved.
    fn place(&mut self, clips: Vec<Clip>, id: &str, snap_into_place: bool, announce: bool) -> Vec<Clip> {
        if self.edit_mode == EditMode::Overwrite {
            return resolve_overlaps(&clips, id);
        }
        let (out, moved) = push_overlaps(&clips, id, snap_into_place);
        if announce && moved > 0 {
            self.toast(if moved == 1 { "Moved 1 clip along to make room".to_string() } else { format!("Moved {moved} clips along to make room") });
        }
        out
    }
    fn locked(&mut self, track_id: &str) -> bool {
        let t = self.track(track_id).clone();
        if t.locked {
            self.toast(format!("{} is locked. Unlock it to edit.", t.name));
        }
        t.locked
    }

    pub fn commit(&mut self) {
        self.push_past();
    }
    pub fn begin_edit(&mut self) {
        if !self.editing {
            self.editing = true;
            self.push_past();
        }
    }
    pub fn end_edit(&mut self) {
        self.editing = false;
    }
    pub fn undo(&mut self) {
        let Some(prev) = self.past.pop() else { return };
        let cur = self.snapshot();
        self.future.push(cur);
        self.clips = prev.clips;
        self.selected = prev.selected;
        self.changed();
        self.toast("Undone");
    }
    pub fn redo(&mut self) {
        let Some(next) = self.future.pop() else { return };
        let cur = self.snapshot();
        self.past.push(cur);
        self.clips = next.clips;
        self.selected = next.selected;
        self.changed();
        self.toast("Redone");
    }
    pub fn select(&mut self, id: Option<String>) {
        self.selected = id;
    }

    pub fn add_clip(&mut self, asset_id: &str, track_id: Option<&str>, start: Option<f64>) -> Option<Clip> {
        self.add_clip_with(asset_id, track_id, start, true)
    }
    /// Like add_clip, but the clip keeps exactly the start it was given (later clips move along
    /// if they'd overlap). For tools that ask for a precise time.
    pub fn add_clip_exact(&mut self, asset_id: &str, track_id: Option<&str>, start: f64) -> Option<Clip> {
        self.add_clip_with(asset_id, track_id, Some(start), false)
    }
    fn add_clip_with(&mut self, asset_id: &str, track_id: Option<&str>, start: Option<f64>, snap_into_place: bool) -> Option<Clip> {
        let c = make_clip(&self.tracks, self.time, asset_id, track_id, start)?;
        if self.locked(&c.track) {
            return None;
        }
        self.push_past();
        let mut clips = self.clips.clone();
        clips.push(c.clone());
        self.clips = self.place(clips, &c.id, snap_into_place, true);
        self.selected = Some(c.id.clone());
        self.changed();
        self.clip(&c.id).cloned().or(Some(c))
    }
    pub fn update_clip(&mut self, id: &str, f: impl FnOnce(&mut Clip)) {
        if let Some(c) = self.clips.iter_mut().find(|c| c.id == id) {
            f(c);
            self.rev += 1;
        }
    }
    pub fn update_text(&mut self, id: &str, f: impl FnOnce(&mut TextProps)) {
        if let Some(t) = self.clips.iter_mut().find(|c| c.id == id).and_then(|c| c.text.as_mut()) {
            f(t);
            self.rev += 1;
        }
    }

    pub fn split(&mut self) {
        let t = snap_frame(self.time);
        let inside = |c: &Clip| t > c.start && t < c.start + c.dur;
        let c = self
            .selected_clip()
            .filter(|c| inside(c))
            .or_else(|| self.clips.iter().find(|k| k.track == "V1" && inside(k)))
            .cloned();
        let Some(c) = c else {
            self.toast("Move the playhead over a clip to split it");
            return;
        };
        if self.locked(&c.track) {
            return;
        }
        let cut = t - c.start;
        let mut b = Clip { id: next_id(), start: t, in_: c.in_ + cut, dur: c.dur - cut, fade_in: 0.0, transition: None, ..c.clone() };
        if let Some(tx) = b.text.as_mut() {
            tx.anim = TextAnim::None;
        }
        self.push_past();
        for k in self.clips.iter_mut() {
            if k.id == c.id {
                k.dur = cut;
                k.fade_out = 0.0;
            }
        }
        self.selected = Some(b.id.clone());
        self.clips.push(b);
        self.changed();
        self.toast(format!("Split at {}", tc(t)));
    }
    pub fn remove(&mut self) {
        let Some(c) = self.selected_clip().cloned() else { return };
        if self.locked(&c.track) {
            return;
        }
        self.push_past();
        self.clips.retain(|k| k.id != c.id);
        self.selected = None;
        self.changed();
    }
    pub fn duplicate(&mut self) {
        let Some(c) = self.selected_clip().cloned() else { return };
        let start = if self.edit_mode == EditMode::Insert { c.start + c.dur } else { free_slot(&self.clips, &c.track, c.start + c.dur, c.dur) };
        let d = Clip { id: next_id(), start, ..c.clone() };
        self.push_past();
        let mut clips = self.clips.clone();
        clips.push(d.clone());
        self.clips = self.place(clips, &d.id, false, false);
        self.selected = Some(d.id.clone());
        self.changed();
        self.toast(format!("Duplicated to {}", tc(d.start)));
    }
    /// Overwrite whatever the clip now covers on its track (no new history entry).
    pub fn settle(&mut self, id: &str, snap_into_place: bool) {
        let clips = self.clips.clone();
        self.clips = self.place(clips, id, snap_into_place, true);
        self.changed();
    }
    pub fn copy(&mut self) -> Option<Clip> {
        let Some(c) = self.selected_clip().cloned() else {
            self.toast("Select a clip to copy");
            return None;
        };
        self.toast(format!("Copied {}", c.name));
        self.clipboard = Some(c.clone());
        Some(c)
    }
    pub fn cut(&mut self) {
        let Some(c) = self.selected_clip().cloned() else { return };
        if self.locked(&c.track) {
            return;
        }
        self.clipboard = Some(c.clone());
        self.push_past();
        self.clips.retain(|k| k.id != c.id);
        self.selected = None;
        self.changed();
        self.toast(format!("Cut {}", c.name));
    }
    pub fn paste(&mut self, clip: Option<Clip>, at: Option<f64>) {
        let Some(src) = clip.or_else(|| self.clipboard.clone()) else {
            self.toast("Nothing to paste. Copy a clip first.");
            return;
        };
        if !has_asset(&src.asset) {
            self.toast("That clip's media isn't in this project. Import the file, then paste again.");
            return;
        }
        if self.locked(&src.track) {
            return;
        }
        let n = Clip { id: next_id(), start: snap_frame(at.unwrap_or(self.time).max(0.0)), ..src };
        self.push_past();
        let mut clips = self.clips.clone();
        clips.push(n.clone());
        self.clips = self.place(clips, &n.id, true, true);
        self.selected = Some(n.id.clone());
        self.changed();
        let at = self.clip(&n.id).map(|c| c.start).unwrap_or(n.start);
        self.toast(format!("Pasted {} at {}", n.name, tc(at)));
    }

    pub fn apply_effect(&mut self, effect_id: &str, clip_id: Option<&str>) {
        let id = clip_id.map(String::from).or_else(|| self.selected.clone());
        let c = id.as_deref().and_then(|i| self.clip(i)).cloned();
        let Some(c) = c.filter(|c| c.visual()) else {
            self.toast("Select a video, image, overlay or text clip first");
            return;
        };
        let Some(fx) = make_clip_effect(effect_id, 1.0) else { return };
        self.push_past();
        self.selected = Some(c.id.clone());
        self.update_clip(&c.id, |k| k.effects.push(fx));
        self.changed();
        self.toast(format!("{} added to {}", effect_def(effect_id).map(|d| d.name).unwrap_or(""), c.name));
    }
    pub fn update_effect(&mut self, clip_id: &str, uid: &str, f: impl FnOnce(&mut ClipEffect)) {
        self.update_clip(clip_id, |c| {
            if let Some(e) = c.effects.iter_mut().find(|e| e.uid == uid) {
                f(e);
            }
        });
    }
    pub fn remove_effect(&mut self, clip_id: &str, uid: &str) {
        self.push_past();
        self.update_clip(clip_id, |c| c.effects.retain(|e| e.uid != uid));
    }
    pub fn move_effect(&mut self, clip_id: &str, uid: &str, dir: i32) {
        self.push_past();
        self.update_clip(clip_id, |c| {
            let Some(i) = c.effects.iter().position(|e| e.uid == uid) else { return };
            let j = i as i32 + dir;
            if j < 0 || j as usize >= c.effects.len() {
                return;
            }
            c.effects.swap(i, j as usize);
        });
    }
    /// "fadein" | "fadeout" | "both" set clip fades; any other id is a transition into the clip.
    pub fn apply_transition(&mut self, id: &str, clip_id: Option<&str>) {
        let cid = clip_id.map(String::from).or_else(|| self.selected.clone());
        let Some(c) = cid.as_deref().and_then(|i| self.clip(i)).cloned() else {
            self.toast("Select a clip first");
            return;
        };
        if matches!(id, "fadein" | "fadeout" | "both") {
            let d = 0.5f64.min(c.dur / 2.0);
            self.push_past();
            self.selected = Some(c.id.clone());
            self.update_clip(&c.id, |k| {
                if id != "fadeout" {
                    k.fade_in = d;
                }
                if id != "fadein" {
                    k.fade_out = d;
                }
            });
            self.toast(format!("Fade added to {}", c.name));
            return;
        }
        let Some(def) = transition_def(id) else { return };
        if c.kind.is_audio() {
            self.toast("Transitions are for video, image, overlay and text clips. Use a fade on audio.");
            return;
        }
        let dur = c.transition.as_ref().map(|t| t.dur).unwrap_or(0.8).min(c.dur);
        self.push_past();
        self.selected = Some(c.id.clone());
        self.update_clip(&c.id, |k| k.transition = Some(TransitionRef { id: id.into(), dur }));
        self.toast(format!("{} transition added to the start of {}", def.name, c.name));
    }
    pub fn toggle_track(&mut self, id: &str, key: &str) {
        if let Some(t) = self.tracks.iter_mut().find(|t| t.id == id) {
            match key {
                "hidden" => t.hidden = !t.hidden,
                "muted" => t.muted = !t.muted,
                _ => t.locked = !t.locked,
            }
        }
        self.changed();
    }
    pub fn set_zoom(&mut self, z: f64) {
        self.zoom = z.clamp(12.0, 240.0);
    }
}
