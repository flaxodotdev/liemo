// The editor's state and edit actions: clips, tracks, selection, undo/redo, insert/overwrite
// placement. Actions that want to tell the user something push to `toasts`; the UI shows them.

use super::assets::{asset, has_asset};
use super::captions::Cue;
use super::keys;
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
    multi: Vec<String>,
    tracks: Vec<Track>,
    markers: Vec<Marker>,
    in_out: (Option<f64>, Option<f64>),
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
/// The track a new clip of this type goes on: its usual one, or the first that takes it.
pub fn home_track(tracks: &[Track], t: AssetType) -> String {
    let d = default_track(t);
    if accepts(tracks, d, t) {
        return d.to_string();
    }
    tracks.iter().find(|x| track_kinds(x.kind).contains(&t)).map(|x| x.id.clone()).unwrap_or_else(|| d.to_string())
}

/// Rows for clips that share moments on one track (titles stack instead of pushing each other
/// along): each clip's row, first fit by start time, and how many rows there are (at least one).
pub fn stack_rows(clips: &[Clip], track: &str) -> (std::collections::HashMap<String, usize>, usize) {
    let mut mine: Vec<&Clip> = clips.iter().filter(|c| c.track == track).collect();
    mine.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut ends: Vec<f64> = vec![];
    let mut rows = std::collections::HashMap::new();
    for c in mine {
        let row = match ends.iter().position(|e| *e <= c.start + EPS) {
            Some(r) => r,
            None => {
                ends.push(0.0);
                ends.len() - 1
            }
        };
        ends[row] = c.start + c.dur;
        rows.insert(c.id.clone(), row);
    }
    let n = ends.len().max(1);
    (rows, n)
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

/// `c` from timeline time `at` to its end, as `base` (which keeps keys on the same footage).
fn later_part(c: &Clip, at: f64, mut base: Clip) -> Clip {
    let cut = at - c.start;
    base.start = at;
    base.in_ = c.in_ + c.src_offset(cut);
    base.dur = c.dur - cut;
    keys::shift(&mut base.keys, cut);
    base
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
            out.push(fit_fades(later_part(c, e, Clip { id: next_id(), fade_in: 0.0, ..c.clone() })));
        } else if cs < s {
            out.push(fit_fades(Clip { dur: s - cs, ..c.clone() }));
        } else {
            out.push(fit_fades(later_part(c, e, c.clone())));
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
        _ => home_track(tracks, a.kind),
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
        sound: None,
        motion: None,
        keys: Default::default(),
        blend: None,
        mask: None,
        duck: 0.0,
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
    /// The clip the inspector shows; with `multi`, the rest of a multiple selection.
    pub selected: Option<String>,
    pub multi: Vec<String>,
    pub tracks: Vec<Track>,
    /// Named moments on the ruler, in time order.
    pub markers: Vec<Marker>,
    /// In and out points: the part of the edit to play in a loop and to export.
    pub in_out: (Option<f64>, Option<f64>),
    pub clips: Vec<Clip>,
    pub pool: Vec<String>,
    past: Vec<Snapshot>,
    future: Vec<Snapshot>,
    pub editing: bool,
    /// The last copied clips, in time order.
    pub clipboard: Vec<Clip>,
    pub toasts: Vec<String>,
    /// Bumped whenever something that is saved changes (clips, tracks, name, pool).
    pub rev: u64,
}

impl Store {
    pub fn new(name: String, clips: Vec<Clip>, tracks: Vec<Track>, pool: Vec<String>) -> Self {
        sync_clip_seq(&clips);
        Store {
            name, time: 0.0, playing: false, zoom: 60.0, snap: true, looping: false, guides: false, edit_mode: EditMode::Insert,
            selected: None, multi: vec![], markers: vec![], in_out: (None, None), tracks: if tracks.is_empty() { default_tracks() } else { tracks }, clips, pool,
            past: vec![], future: vec![], editing: false, clipboard: vec![], toasts: vec![], rev: 0,
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
        Snapshot { clips: self.clips.clone(), selected: self.selected.clone(), multi: self.multi.clone(), tracks: self.tracks.clone(), markers: self.markers.clone(), in_out: self.in_out }
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
        // Titles may overlap: they stack on the Titles track (each in its own row) and all show.
        if clips.iter().any(|c| c.id == id && c.kind == AssetType::Text) {
            return clips;
        }
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

    /// One undo step for the edit about to happen; during a gesture (begin_edit…end_edit) the
    /// whole gesture is already one step.
    pub fn commit(&mut self) {
        if !self.editing {
            self.push_past();
        }
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
        self.multi = prev.multi;
        self.tracks = prev.tracks;
        self.markers = prev.markers;
        self.in_out = prev.in_out;
        self.changed();
        self.toast("Undone");
    }
    pub fn redo(&mut self) {
        let Some(next) = self.future.pop() else { return };
        let cur = self.snapshot();
        self.past.push(cur);
        self.clips = next.clips;
        self.selected = next.selected;
        self.multi = next.multi;
        self.tracks = next.tracks;
        self.markers = next.markers;
        self.in_out = next.in_out;
        self.changed();
        self.toast("Redone");
    }
    pub fn select(&mut self, id: Option<String>) {
        self.selected = id;
        self.multi.clear();
    }

    // ---------- Multiple selection ----------
    /// Every selected clip id, the inspector's one first.
    pub fn selection(&self) -> Vec<String> {
        self.selected.iter().chain(self.multi.iter()).filter(|id| self.clip(id).is_some()).cloned().collect()
    }
    pub fn is_selected(&self, id: &str) -> bool {
        self.selected.as_deref() == Some(id) || self.multi.iter().any(|m| m == id)
    }
    pub fn many_selected(&self) -> bool {
        self.selection().len() > 1
    }
    /// Shift- or ⌘-click: add a clip to the selection, or take it out.
    pub fn toggle_select(&mut self, id: &str) {
        if self.selected.as_deref() == Some(id) {
            self.selected = self.multi.pop();
        } else if let Some(i) = self.multi.iter().position(|m| m == id) {
            self.multi.remove(i);
        } else {
            if let Some(p) = self.selected.take() {
                self.multi.push(p);
            }
            self.selected = Some(id.to_string());
        }
    }
    /// Select exactly these clips; the first is the one the inspector shows.
    pub fn select_many(&mut self, ids: Vec<String>) {
        let mut ids = ids.into_iter();
        self.selected = ids.next();
        self.multi = ids.collect();
    }
    /// Make an already-selected clip the inspector's one, keeping the rest selected.
    pub fn focus_in_selection(&mut self, id: &str) {
        if let Some(i) = self.multi.iter().position(|m| m == id) {
            let old = self.selected.replace(self.multi.remove(i));
            if let Some(o) = old {
                self.multi.push(o);
            }
        }
    }
    pub fn select_all(&mut self, track: Option<&str>) {
        let mut v: Vec<&Clip> = self.clips.iter().filter(|c| track.map(|t| c.track == t).unwrap_or(true) && !self.track(&c.track).locked).collect();
        v.sort_by(|a, b| a.start.total_cmp(&b.start));
        let ids = v.into_iter().map(|c| c.id.clone()).collect();
        self.select_many(ids);
    }
    /// The selected clips, in time order.
    pub fn selected_clips(&self) -> Vec<Clip> {
        let ids = self.selection();
        let mut v: Vec<Clip> = self.clips.iter().filter(|c| ids.contains(&c.id)).cloned().collect();
        v.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.track.cmp(&b.track)));
        v
    }
    /// The same change to every selected clip (on unlocked tracks), as one undo step.
    pub fn update_selection(&mut self, f: impl Fn(&mut Clip)) {
        let ids = self.selection();
        let locked: Vec<String> = self.tracks.iter().filter(|t| t.locked).map(|t| t.id.clone()).collect();
        for c in self.clips.iter_mut().filter(|c| ids.contains(&c.id) && !locked.contains(&c.track)) {
            f(c);
            c.fade_in = c.fade_in.clamp(0.0, c.dur / 2.0);
            c.fade_out = c.fade_out.clamp(0.0, c.dur / 2.0);
        }
        self.changed();
    }
    /// After moving several clips together: fit each into its track, earliest first.
    pub fn settle_many(&mut self, ids: &[String]) {
        let mut order: Vec<(f64, String)> = ids.iter().filter_map(|id| self.clip(id).map(|c| (c.start, id.clone()))).collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, id) in order {
            let clips = self.clips.clone();
            self.clips = self.place(clips, &id, false, true);
        }
        self.changed();
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
        self.select(Some(c.id.clone()));
        self.changed();
        self.clip(&c.id).cloned().or(Some(c))
    }
    pub fn update_clip(&mut self, id: &str, f: impl FnOnce(&mut Clip)) {
        if let Some(c) = self.clips.iter_mut().find(|c| c.id == id) {
            f(c);
            self.rev += 1;
        }
    }
    /// Change an audio clip's sound effects. A new speed stretches or shrinks the clip so it
    /// still covers the same stretch of the source (within what the file has).
    pub fn set_sound(&mut self, id: &str, f: impl FnOnce(&mut AudioFx)) {
        self.update_clip(id, |c| {
            let old = c.speed();
            let mut fx = c.sound.clone().unwrap_or_default();
            f(&mut fx);
            fx.speed = fx.speed.clamp(0.25, 4.0);
            let new = fx.speed;
            c.sound = (!fx.is_identity() || fx.keep_pitch).then_some(fx);
            if (new - old).abs() > 1e-9 {
                let mut d = c.dur * old / new;
                if let Some(a) = asset(&c.asset) {
                    d = d.min((a.dur - c.in_) / new);
                }
                c.dur = snap_frame(d.max(min_clip()));
                c.fade_in = c.fade_in.min(c.dur / 2.0);
                c.fade_out = c.fade_out.min(c.dur / 2.0);
            }
        });
    }
    pub fn update_text(&mut self, id: &str, f: impl FnOnce(&mut TextProps)) {
        if let Some(t) = self.clips.iter_mut().find(|c| c.id == id).and_then(|c| c.text.as_mut()) {
            f(t);
            self.rev += 1;
        }
    }

    pub fn split(&mut self) {
        let t = snap_frame(self.time);
        let inside = |c: &Clip| t > c.start + EPS && t < c.start + c.dur - EPS;
        let mut targets: Vec<Clip> = self.selected_clips().into_iter().filter(|c| inside(c)).collect();
        if targets.is_empty() {
            targets = self.clips.iter().find(|k| k.track == "V1" && inside(k)).cloned().into_iter().collect();
        }
        if targets.is_empty() {
            self.toast("Move the playhead over a clip to split it");
            return;
        }
        targets.retain(|c| !self.track(&c.track).locked);
        if targets.is_empty() {
            let name = self.track(&self.selected_clip().map(|c| c.track.clone()).unwrap_or_default()).name.clone();
            self.toast(format!("{name} is locked. Unlock it to edit."));
            return;
        }
        self.push_past();
        let mut seconds = vec![];
        for c in targets {
            let cut = t - c.start;
            let mut b = Clip { id: next_id(), start: t, in_: c.in_ + c.src_offset(cut), dur: c.dur - cut, fade_in: 0.0, transition: None, ..c.clone() };
            if c.motion.as_ref().map(|m| m.reverse).unwrap_or(false) && !c.ramped() {
                // Backwards: the second half shows the earlier footage.
                b.in_ = c.in_;
            }
            keys::shift(&mut b.keys, cut);
            if let Some(tx) = b.text.as_mut() {
                tx.anim = TextAnim::None;
            }
            for k in self.clips.iter_mut() {
                if k.id == c.id {
                    k.dur = cut;
                    k.fade_out = 0.0;
                    if c.motion.as_ref().map(|m| m.reverse).unwrap_or(false) && !c.ramped() {
                        k.in_ = c.in_ + c.src_offset(cut);
                    }
                }
            }
            seconds.push(b.id.clone());
            self.clips.push(b);
        }
        self.select_many(seconds);
        self.changed();
        self.toast(format!("Split at {}", tc(t)));
    }
    pub fn remove(&mut self) {
        let sel = self.selected_clips();
        if sel.is_empty() {
            return;
        }
        let ok: Vec<String> = sel.iter().filter(|c| !self.track(&c.track).locked).map(|c| c.id.clone()).collect();
        if ok.is_empty() {
            let _ = self.locked(&sel[0].track);
            return;
        }
        self.push_past();
        self.clips.retain(|k| !ok.contains(&k.id));
        self.select(None);
        self.changed();
        if ok.len() > 1 {
            self.toast(format!("Deleted {} clips", ok.len()));
        }
    }
    pub fn duplicate(&mut self) {
        let sel = self.selected_clips();
        if sel.len() > 1 {
            // The whole block again, straight after itself.
            let first = sel.iter().map(|c| c.start).fold(f64::INFINITY, f64::min);
            let last = sel.iter().map(|c| c.start + c.dur).fold(0.0, f64::max);
            self.push_past();
            self.paste_clips(sel, last, false);
            self.toast(format!("Duplicated {} clips to {}", self.selection().len(), tc(last)));
            let _ = first;
            return;
        }
        let Some(c) = self.selected_clip().cloned() else { return };
        let start = if self.edit_mode == EditMode::Insert { c.start + c.dur } else { free_slot(&self.clips, &c.track, c.start + c.dur, c.dur) };
        let d = Clip { id: next_id(), start, ..c.clone() };
        self.push_past();
        let mut clips = self.clips.clone();
        clips.push(d.clone());
        self.clips = self.place(clips, &d.id, false, false);
        self.select(Some(d.id.clone()));
        self.changed();
        self.toast(format!("Duplicated to {}", tc(d.start)));
    }
    /// Overwrite whatever the clip now covers on its track (no new history entry).
    pub fn settle(&mut self, id: &str, snap_into_place: bool) {
        let clips = self.clips.clone();
        self.clips = self.place(clips, id, snap_into_place, true);
        self.changed();
    }
    pub fn copy(&mut self) -> Option<Vec<Clip>> {
        let sel = self.selected_clips();
        if sel.is_empty() {
            self.toast("Select a clip to copy");
            return None;
        }
        self.toast(if sel.len() == 1 { format!("Copied {}", sel[0].name) } else { format!("Copied {} clips", sel.len()) });
        self.clipboard = sel.clone();
        Some(sel)
    }
    pub fn cut(&mut self) {
        let sel: Vec<Clip> = self.selected_clips().into_iter().filter(|c| !self.track(&c.track).locked).collect();
        if sel.is_empty() {
            if let Some(c) = self.selected_clip().cloned() {
                let _ = self.locked(&c.track);
            }
            return;
        }
        let ids: Vec<String> = sel.iter().map(|c| c.id.clone()).collect();
        self.toast(if sel.len() == 1 { format!("Cut {}", sel[0].name) } else { format!("Cut {} clips", sel.len()) });
        self.clipboard = sel;
        self.push_past();
        self.clips.retain(|k| !ids.contains(&k.id));
        self.select(None);
        self.changed();
    }
    /// Paste clips (the clipboard's when None) with the earliest at `at` (default the playhead),
    /// keeping how they sit relative to one another.
    pub fn paste(&mut self, clips: Option<Vec<Clip>>, at: Option<f64>) {
        let src = clips.unwrap_or_else(|| self.clipboard.clone());
        if src.is_empty() {
            self.toast("Nothing to paste. Copy a clip first.");
            return;
        }
        if src.iter().any(|c| !has_asset(&c.asset)) {
            self.toast("That clip's media isn't in this project. Import the file, then paste again.");
            return;
        }
        if let Some(c) = src.iter().find(|c| self.track(&c.track).locked) {
            let t = c.track.clone();
            let _ = self.locked(&t);
            return;
        }
        let at = snap_frame(at.unwrap_or(self.time).max(0.0));
        self.push_past();
        let n = src.len();
        let name = src[0].name.clone();
        self.paste_clips(src, at, n == 1);
        let start = self.selected_clip().map(|c| c.start).unwrap_or(at);
        self.toast(if n == 1 { format!("Pasted {name} at {}", tc(start)) } else { format!("Pasted {n} clips at {}", tc(at)) });
    }
    /// New copies of `src` from `at` on, fitted into their tracks and selected. No history entry.
    fn paste_clips(&mut self, mut src: Vec<Clip>, at: f64, snap_into_place: bool) {
        src.sort_by(|a, b| a.start.total_cmp(&b.start));
        let first = src[0].start;
        let mut ids = vec![];
        for c in src {
            let n = Clip { id: next_id(), start: snap_frame(at + c.start - first), ..c };
            let mut clips = self.clips.clone();
            clips.push(n.clone());
            self.clips = self.place(clips, &n.id, snap_into_place, true);
            ids.push(n.id);
        }
        self.select_many(ids);
        self.changed();
    }

    // ---------- Keyframes ----------
    /// Set a property at `lt` seconds into the clip: a key there when the property is animated,
    /// otherwise its own value.
    pub fn set_prop(&mut self, id: &str, prop: &str, v: f64, lt: f64) {
        let lt = clamp_lt(self.clip(id), lt);
        self.update_clip(id, |c| match c.keys.get_mut(prop).filter(|k| !k.is_empty()) {
            Some(ks) => keys::upsert(ks, lt, v),
            None => c.set_base(prop, v),
        });
    }
    /// The keyframe button: add a key at `lt` with the value there, or take away the one there.
    /// Removing the last key leaves the property at that value.
    pub fn toggle_key(&mut self, id: &str, prop: &str, lt: f64) {
        let Some(c) = self.clip(id).cloned() else { return };
        if self.locked(&c.track) {
            return;
        }
        let lt = clamp_lt(Some(&c), lt);
        let Some(now) = c.value(prop, lt) else { return };
        self.push_past();
        let mut removed = false;
        self.update_clip(id, |c| {
            let ks = c.keys.entry(prop.to_string()).or_default();
            if let Some(i) = ks.iter().position(|k| (k.t - lt).abs() < keys::SAME) {
                ks.remove(i);
                removed = true;
            } else {
                keys::upsert(ks, lt, now);
            }
            if ks.is_empty() {
                c.keys.remove(prop);
                c.set_base(prop, now);
            }
        });
        if prop == "speed" {
            self.fit_ramp(id);
        }
        self.toast(if removed { format!("Removed the {} keyframe", keys::prop_label(prop).to_lowercase()) } else { format!("{} keyframe at {}", keys::prop_label(prop), tc(c.start + lt)) });
    }
    /// Every key at a moment of the clip (the timeline's diamonds): change its ease or remove it.
    pub fn edit_keys_at(&mut self, id: &str, lt: f64, ease: Option<keys::Ease>) {
        self.push_past();
        self.update_clip(id, |c| {
            let now: Vec<(String, f64)> = c.keys.keys().filter_map(|p| c.value(p, lt).map(|v| (p.clone(), v))).collect();
            for ks in c.keys.values_mut() {
                match ease {
                    Some(e) => ks.iter_mut().filter(|k| (k.t - lt).abs() < keys::SAME).for_each(|k| k.ease = e),
                    None => ks.retain(|k| (k.t - lt).abs() >= keys::SAME),
                }
            }
            // Properties left with no keys keep the value they had at that moment.
            let empty: Vec<String> = c.keys.iter().filter(|(_, k)| k.is_empty()).map(|(p, _)| p.clone()).collect();
            for p in empty {
                c.keys.remove(&p);
                if let Some((_, v)) = now.iter().find(|(q, _)| *q == p) {
                    c.set_base(&p, *v);
                }
            }
        });
        self.fit_ramp(id);
    }
    /// Drop all of a clip's keys (or one property's), leaving values as they are at `lt`.
    pub fn clear_keys(&mut self, id: &str, prop: Option<&str>, lt: f64) {
        self.push_past();
        self.update_clip(id, |c| {
            let props: Vec<String> = c.keys.keys().filter(|p| prop.map(|q| q == p.as_str()).unwrap_or(true)).cloned().collect();
            for p in props {
                if let Some(v) = c.value(&p, lt) {
                    if p != "speed" {
                        c.set_base(&p, v);
                    }
                }
                c.keys.remove(&p);
            }
        });
    }

    // ---------- Speed ----------
    /// A new constant speed: the clip keeps the same footage, so it gets longer or shorter. For
    /// audio clips this is the sound's speed (pitch as set in Sound).
    pub fn set_speed(&mut self, id: &str, v: f64) {
        let Some(c) = self.clip(id).cloned() else { return };
        if c.kind == AssetType::Audio {
            self.set_sound(id, |f| f.speed = v.clamp(0.5, 2.0));
            return;
        }
        let v = v.clamp(0.25, 4.0);
        self.update_clip(id, |k| {
            k.keys.remove("speed");
            let span = k.src_span();
            let m = k.motion.get_or_insert_with(Motion::default);
            m.freeze = false;
            m.speed = v;
            let mut d = span / v;
            if let Some(a) = asset(&k.asset).filter(|a| a.media.is_some()) {
                d = d.min(((a.dur - k.in_) / v).max(min_clip()));
            }
            k.dur = snap_frame(d.max(min_clip()));
            k.fade_in = k.fade_in.min(k.dur / 2.0);
            k.fade_out = k.fade_out.min(k.dur / 2.0);
            if k.motion.as_ref() == Some(&Motion::default()) {
                k.motion = None;
            }
        });
    }
    /// Rate stretch: make the clip `dur` long by playing the same footage faster or slower.
    pub fn stretch_to(&mut self, id: &str, o_dur: f64, o_speed: f64, dur: f64) {
        let Some(c) = self.clip(id).cloned() else { return };
        let (lo, hi) = if c.kind == AssetType::Audio { (0.5, 2.0) } else { (0.25, 4.0) };
        let speed = (o_speed * o_dur / dur.max(min_clip())).clamp(lo, hi);
        let dur = snap_frame(o_dur * o_speed / speed);
        if c.kind == AssetType::Audio {
            self.update_clip(id, |k| {
                let mut fx = k.sound.clone().unwrap_or_default();
                fx.speed = speed;
                k.sound = (!fx.is_identity() || fx.keep_pitch).then_some(fx);
            });
        } else {
            self.update_clip(id, |k| k.motion.get_or_insert_with(Motion::default).speed = speed);
        }
        self.update_clip(id, |k| {
            k.dur = dur.max(min_clip());
            k.fade_in = k.fade_in.min(k.dur / 2.0);
            k.fade_out = k.fade_out.min(k.dur / 2.0);
        });
    }
    pub fn set_reverse(&mut self, id: &str, on: bool) {
        self.commit();
        self.update_clip(id, |k| {
            let m = k.motion.get_or_insert_with(Motion::default);
            m.reverse = on;
            if k.motion.as_ref() == Some(&Motion::default()) {
                k.motion = None;
            }
        });
    }
    /// Hold the frame under the playhead for `hold` seconds: the clip splits there, the held
    /// frame goes between the halves, and what follows on the track moves along to make room.
    pub fn freeze_frame(&mut self, id: &str, hold: f64) {
        let Some(c) = self.clip(id).cloned() else { return };
        if c.kind.is_audio() || c.kind == AssetType::Text {
            self.toast("Freeze frame is for video and animated clips");
            return;
        }
        if self.locked(&c.track) {
            return;
        }
        let t = snap_frame(self.time.clamp(c.start, c.start + c.dur));
        let lt = t - c.start;
        let src = c.in_ + c.src_offset(lt.min(c.dur - 1e-3));
        let hold = snap_frame(hold.max(min_clip()));
        self.push_past();
        // Everything at or after the playhead on this track moves along by the hold.
        for k in self.clips.iter_mut().filter(|k| k.track == c.track && k.id != c.id && k.start >= t - EPS) {
            k.start = snap_frame(k.start + hold);
        }
        let mut frozen = Clip {
            id: next_id(),
            start: t,
            dur: hold,
            in_: src,
            fade_in: 0.0,
            fade_out: 0.0,
            transition: None,
            keys: Default::default(),
            name: format!("{} (freeze)", c.name),
            ..c.clone()
        };
        let mut m = c.motion.clone().unwrap_or_default();
        m.freeze = true;
        m.reverse = false;
        m.anim_in.clear();
        m.anim_out.clear();
        frozen.motion = Some(m);
        // Keyed looks hold at their value on that frame.
        let posed = c.posed(t);
        frozen.x = posed.x;
        frozen.y = posed.y;
        frozen.scale = posed.scale;
        frozen.rot = posed.rot;
        frozen.opacity = posed.opacity;
        frozen.effects = posed.effects.clone();
        if lt > min_clip() && lt < c.dur - min_clip() {
            let mut b = Clip { id: next_id(), start: t + hold, in_: c.in_ + c.src_offset(lt), dur: c.dur - lt, fade_in: 0.0, transition: None, ..c.clone() };
            keys::shift(&mut b.keys, lt);
            for k in self.clips.iter_mut().filter(|k| k.id == c.id) {
                k.dur = lt;
                k.fade_out = 0.0;
            }
            self.clips.push(b);
        } else if lt >= c.dur - min_clip() {
            // At the very end: hold the last frame after the clip.
        } else {
            // At the very start: the hold comes first and the clip follows it.
            for k in self.clips.iter_mut().filter(|k| k.id == c.id) {
                k.start = snap_frame(k.start + hold);
            }
        }
        self.select(Some(frozen.id.clone()));
        self.clips.push(frozen);
        self.changed();
        self.toast(format!("Held the frame at {} for {:.1}s", tc(t), hold));
    }
    /// A speed ramp preset, keyed over the clip's length.
    pub fn ramp_preset(&mut self, id: &str, preset: &str) {
        let Some(c) = self.clip(id).cloned() else { return };
        if c.kind.is_audio() {
            return;
        }
        let d = c.dur;
        let k = |t: f64, v: f64| keys::Key { t, v, ease: keys::Ease::Smooth };
        let ks = match preset {
            "slowmo" => vec![k(0.0, 1.0), k(d * 0.35, 0.3), k(d * 0.65, 0.3), k(d, 1.0)],
            "up" => vec![k(0.0, 1.0), k(d, 3.0)],
            "down" => vec![k(0.0, 2.5), k(d, 0.5)],
            "pulse" => vec![k(0.0, 3.0), k(d * 0.4, 3.0), k(d * 0.5, 0.25), k(d * 0.7, 0.25), k(d, 3.0)],
            _ => vec![],
        };
        self.push_past();
        self.update_clip(id, |c| {
            if ks.is_empty() {
                c.keys.remove("speed");
            } else {
                c.keys.insert("speed".into(), ks);
                if let Some(m) = c.motion.as_mut() {
                    m.freeze = false;
                }
            }
        });
        self.fit_ramp(id);
        self.toast(if preset == "none" { "Speed ramp removed".to_string() } else { "Speed ramp added. Change it with the Speed keyframes.".to_string() });
    }
    /// Keep a ramped clip inside its media: shorten it if the ramp would run past the file's end.
    fn fit_ramp(&mut self, id: &str) {
        let Some(c) = self.clip(id).cloned() else { return };
        let Some(a) = asset(&c.asset).filter(|a| a.media.is_some()) else { return };
        if !c.ramped() || c.src_span() <= a.dur - c.in_ + 1e-3 {
            return;
        }
        // Binary search the longest length whose footage fits.
        let (mut lo, mut hi) = (min_clip(), c.dur);
        for _ in 0..40 {
            let mid = (lo + hi) / 2.0;
            if keys::integral(&c.keys["speed"], mid) <= a.dur - c.in_ { lo = mid } else { hi = mid }
        }
        self.update_clip(id, |k| k.dur = snap_frame(lo).max(min_clip()));
    }

    // ---------- Captions ----------
    /// Put cues on the Titles track as text clips in a caption style: "subtitle" (the Caption
    /// text style as it is), "pop", "clean" or "boxed". Returns how many were added.
    pub fn add_captions(&mut self, cues: &[Cue], style: &str) -> usize {
        if cues.is_empty() || self.locked("T1") {
            return 0;
        }
        self.push_past();
        let mut ids = vec![];
        for cue in cues {
            let Some(mut c) = make_clip(&self.tracks, cue.start, "tx-caption", Some("T1"), Some(cue.start)) else { continue };
            c.dur = snap_frame((cue.end - cue.start).max(min_clip()));
            c.name = cue.text.lines().next().unwrap_or("Caption").chars().take(32).collect();
            if let Some(t) = c.text.as_mut() {
                t.content = cue.text.clone();
                let k = (W() / 1280.0).sqrt().min(1.0) as f64;
                let y = (260.0 * H() as f64 / 720.0).round();
                match style {
                    "pop" => {
                        t.font = "Mona Sans".into();
                        t.weight = 800.0;
                        t.size = (76.0 * k).round();
                        t.color = "#ffffff".into();
                        t.anim = TextAnim::Pop;
                        t.boxed = false;
                        t.outline = Some("#000000".into());
                        t.y = y;
                    }
                    "clean" => {
                        t.font = "Inter".into();
                        t.weight = 600.0;
                        t.size = (52.0 * k).round();
                        t.color = "#ffffff".into();
                        t.anim = TextAnim::Fade;
                        t.boxed = false;
                        t.shadow = Some("#000000".into());
                        t.y = y;
                    }
                    "boxed" => {
                        t.font = "Inter".into();
                        t.weight = 700.0;
                        t.size = (48.0 * k).round();
                        t.color = "#ffffff".into();
                        t.anim = TextAnim::None;
                        t.boxed = true;
                        t.y = y;
                    }
                    _ => {}
                }
                t.align = TextAlign::Center;
            }
            ids.push(c.id.clone());
            self.clips.push(c);
        }
        self.select_many(ids.clone());
        self.changed();
        ids.len()
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
        // Onto a clip that's part of a multiple selection: every selected picture clip gets it.
        let group: Vec<Clip> = if self.is_selected(&c.id) && self.many_selected() { self.selected_clips().into_iter().filter(|k| k.visual()).collect() } else { vec![] };
        if group.len() > 1 {
            for k in &group {
                let fx = make_clip_effect(effect_id, 1.0).unwrap_or_else(|| fx.clone());
                self.update_clip(&k.id, |k| k.effects.push(fx));
            }
            self.changed();
            self.toast(format!("{} added to {} clips", effect_def(effect_id).map(|d| d.name).unwrap_or(""), group.len()));
            return;
        }
        self.select(Some(c.id.clone()));
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
        if matches!(id, "fadein" | "fadeout" | "both" | "nofade") {
            if self.is_selected(&c.id) && self.many_selected() {
                self.push_past();
                self.update_selection(|k| {
                    let d = 0.5f64.min(k.dur / 2.0);
                    if id == "nofade" {
                        k.fade_in = 0.0;
                        k.fade_out = 0.0;
                    }
                    if id == "fadein" || id == "both" {
                        k.fade_in = d;
                    }
                    if id == "fadeout" || id == "both" {
                        k.fade_out = d;
                    }
                });
                self.toast(format!("Fades set on {} clips", self.selection().len()));
                return;
            }
            let d = 0.5f64.min(c.dur / 2.0);
            self.push_past();
            self.select(Some(c.id.clone()));
            if id == "nofade" {
                self.update_clip(&c.id, |k| {
                    k.fade_in = 0.0;
                    k.fade_out = 0.0;
                });
                return;
            }
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
        if self.is_selected(&c.id) && self.many_selected() {
            let n = self.selected_clips().iter().filter(|k| k.visual()).count();
            self.update_selection(|k| {
                if k.visual() {
                    k.transition = Some(TransitionRef { id: id.into(), dur: dur.min(k.dur) });
                }
            });
            self.toast(format!("{} transition added to {n} clips", def.name));
            return;
        }
        self.select(Some(c.id.clone()));
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
    /// Split a video's sound off onto an audio track as its own clip; the video goes silent.
    pub fn detach_audio(&mut self, id: &str) -> Option<String> {
        let c = self.clip(id).cloned()?;
        if c.kind != AssetType::Video {
            return None;
        }
        if c.ramped() || c.frozen() || c.motion.as_ref().map(|m| m.reverse).unwrap_or(false) {
            self.toast("Detach audio works on clips playing forwards at a steady speed");
            return None;
        }
        if !asset(&c.asset).and_then(|a| a.media.as_ref().map(|m| m.has_audio)).unwrap_or(false) {
            self.toast(format!("{} has no sound to detach", c.name));
            return None;
        }
        self.push_past();
        let (s, e) = (c.start, c.start + c.dur);
        let free = |st: &Store, t: &str| !st.clips.iter().any(|k| k.track == t && k.start < e - EPS && k.start + k.dur > s + EPS);
        let audio: Vec<String> = self.tracks.iter().filter(|t| t.kind == TrackKind::Audio && !t.locked).map(|t| t.id.clone()).collect();
        // Voice & SFX first, then any free audio track, else a new one.
        let track = ["A2".to_string()].iter().chain(audio.iter()).find(|t| audio.contains(t) && free(self, t)).cloned();
        let track = match track {
            Some(t) => t,
            None => {
                let editing = self.editing;
                self.editing = true; // one undo step with the rest
                let t = self.add_track(TrackKind::Audio, None);
                self.editing = editing;
                t
            }
        };
        let speed = c.speed();
        let mut a = Clip {
            id: next_id(),
            kind: AssetType::Audio,
            track,
            name: format!("{} (audio)", c.name),
            motion: None,
            effects: vec![],
            transition: None,
            text: None,
            fx_scope: None,
            mask: None,
            blend: None,
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            rot: 0.0,
            opacity: 1.0,
            sound: ((speed - 1.0).abs() > 1e-3).then(|| AudioFx { speed, keep_pitch: true, ..Default::default() }),
            ..c.clone()
        };
        a.keys.retain(|p, _| p == "volume");
        let aid = a.id.clone();
        self.clips.push(a);
        self.update_clip(id, |v| {
            v.volume = 0.0;
            v.keys.remove("volume");
        });
        self.select(Some(aid.clone()));
        self.changed();
        self.toast("Audio detached. The video is now silent.");
        Some(aid)
    }

    // ---------- Tracks ----------
    /// A new track of `kind`, next to the others of its kind (titles above, audio below).
    /// Returns its id.
    pub fn add_track(&mut self, kind: TrackKind, name: Option<String>) -> String {
        let prefix = match kind {
            TrackKind::Text => "T",
            TrackKind::Audio => "A",
            _ => "V",
        };
        let n = (1..).find(|i| !self.tracks.iter().any(|t| t.id == format!("{prefix}{i}"))).unwrap();
        let id = format!("{prefix}{n}");
        let kind = if kind == TrackKind::Video { TrackKind::Overlay } else { kind };
        let name = name.unwrap_or_else(|| match kind {
            TrackKind::Text => format!("Titles {n}"),
            TrackKind::Audio => format!("Audio {n}"),
            _ => format!("Video {n}"),
        });
        self.commit();
        let t = Track { id: id.clone(), name, kind, hidden: false, muted: false, locked: false };
        // Picture tracks stack on top of the other picture tracks (in front); titles go first;
        // audio goes last.
        let at = match kind {
            TrackKind::Text => 0,
            TrackKind::Audio => self.tracks.len(),
            _ => self.tracks.iter().position(|t| t.kind != TrackKind::Text).unwrap_or(0),
        };
        self.tracks.insert(at, t);
        self.changed();
        id
    }
    /// Remove a track and its clips. The main video track and the last track of a kind stay.
    pub fn remove_track(&mut self, id: &str) -> bool {
        let Some(t) = self.tracks.iter().find(|t| t.id == id).cloned() else { return false };
        let same = |k: TrackKind| matches!((k, t.kind), (TrackKind::Overlay | TrackKind::Video, TrackKind::Overlay | TrackKind::Video)) || k == t.kind;
        if id == "V1" || self.tracks.iter().filter(|x| same(x.kind)).count() <= 1 {
            self.toast(format!("{} can't be removed: the edit needs it", t.name));
            return false;
        }
        let n = self.clips.iter().filter(|c| c.track == id).count();
        self.push_past();
        self.clips.retain(|c| c.track != id);
        self.tracks.retain(|x| x.id != id);
        self.select(None);
        self.changed();
        self.toast(if n > 0 { format!("Removed {} and its {n} clip{}. Undo brings them back.", t.name, if n == 1 { "" } else { "s" }) } else { format!("Removed {}", t.name) });
        true
    }
    pub fn rename_track(&mut self, id: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        self.push_past();
        if let Some(t) = self.tracks.iter_mut().find(|t| t.id == id) {
            t.name = name.chars().take(40).collect();
        }
        self.changed();
    }
    /// Move a track up or down among the tracks of its kind (picture tracks higher are in front).
    pub fn move_track(&mut self, id: &str, dir: i32) {
        let Some(i) = self.tracks.iter().position(|t| t.id == id) else { return };
        let j = i as i32 + dir;
        if j < 0 || j as usize >= self.tracks.len() {
            return;
        }
        let audio = |k: TrackKind| k == TrackKind::Audio;
        let (a, b) = (self.tracks[i].kind, self.tracks[j as usize].kind);
        if audio(a) != audio(b) || (a == TrackKind::Text) != (b == TrackKind::Text) {
            return;
        }
        self.push_past();
        self.tracks.swap(i, j as usize);
        self.changed();
    }

    // ---------- Markers and in/out ----------
    /// M: a marker at the playhead, or take away the one there.
    pub fn toggle_marker(&mut self) {
        let t = snap_frame(self.time);
        self.push_past();
        if let Some(i) = self.markers.iter().position(|m| (m.t - t).abs() < 0.5 / FPS()) {
            self.markers.remove(i);
            self.toast("Marker removed");
        } else {
            let n = self.markers.len() + 1;
            self.markers.push(Marker { t, name: format!("Marker {n}"), color: MARKER_COLORS[0].0.into() });
            self.markers.sort_by(|a, b| a.t.total_cmp(&b.t));
            self.toast(format!("Marker at {}", tc(t)));
        }
        self.changed();
    }
    pub fn add_markers(&mut self, list: Vec<Marker>) {
        if list.is_empty() {
            return;
        }
        self.push_past();
        for m in list {
            if !self.markers.iter().any(|x| (x.t - m.t).abs() < 0.5 / FPS()) {
                self.markers.push(m);
            }
        }
        self.markers.sort_by(|a, b| a.t.total_cmp(&b.t));
        self.changed();
    }
    pub fn update_marker(&mut self, i: usize, f: impl FnOnce(&mut Marker)) {
        if i < self.markers.len() {
            self.push_past();
            f(&mut self.markers[i]);
            self.markers.sort_by(|a, b| a.t.total_cmp(&b.t));
            self.changed();
        }
    }
    pub fn remove_marker(&mut self, i: usize) {
        if i < self.markers.len() {
            self.push_past();
            self.markers.remove(i);
            self.changed();
        }
    }
    /// The next (dir 1) or previous (dir −1) marker from the playhead.
    pub fn marker_from(&self, dir: i32) -> Option<f64> {
        let t = self.time;
        if dir > 0 {
            self.markers.iter().map(|m| m.t).find(|m| *m > t + 1e-6)
        } else {
            self.markers.iter().rev().map(|m| m.t).find(|m| *m < t - 1e-6)
        }
    }
    pub fn set_in(&mut self, t: Option<f64>) {
        self.push_past();
        self.in_out.0 = t.map(snap_frame);
        if let (Some(i), Some(o)) = self.in_out {
            if o <= i {
                self.in_out.1 = None;
            }
        }
        self.changed();
    }
    pub fn set_out(&mut self, t: Option<f64>) {
        self.push_past();
        self.in_out.1 = t.map(snap_frame);
        if let (Some(i), Some(o)) = self.in_out {
            if o <= i {
                self.in_out.0 = None;
            }
        }
        self.changed();
    }
    /// The in-to-out range, when either is set (the edit's start or end stands in for the other).
    pub fn in_out_range(&self) -> Option<(f64, f64)> {
        match self.in_out {
            (None, None) => None,
            (i, o) => {
                let (a, b) = (i.unwrap_or(0.0), o.unwrap_or_else(|| self.end()));
                (b > a).then_some((a, b))
            }
        }
    }

    pub fn set_zoom(&mut self, z: f64) {
        self.zoom = z.clamp(12.0, 240.0);
    }
}

/// A time inside the clip (keys outside it would never show).
fn clamp_lt(c: Option<&Clip>, lt: f64) -> f64 {
    let d = c.map(|c| c.dur).unwrap_or(0.0);
    snap_frame(lt.clamp(0.0, d.max(0.0)))
}
