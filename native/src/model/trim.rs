// Trim-mode edits, as in Resolve's Trim tool. Each one starts from the timeline as it was when
// the drag began (`base`) and the distance dragged so far, and returns the timeline after it:
//
// - Ripple: an edge moves and everything after it on the track moves with it, so no gap opens
//   and nothing is covered. The head stays where it is on the timeline; trimming it changes which
//   footage the clip starts on.
// - Roll: the cut between two touching clips moves; one gets longer and the other shorter by the
//   same amount, and nothing else moves.
// - Slip: the clip stays put and shows an earlier or later part of its footage.
// - Slide: the clip moves between its neighbours; the one before gets longer or shorter at its
//   end and the one after at its start, so the total length stays the same.

use crate::model::assets::asset;
use crate::model::keys::{self, SAME};
use crate::model::types::{AssetType, Clip};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrimOp {
    RippleHead,
    RippleTail,
    /// Roll the cut at the clip's head (`false`) or tail (`true`).
    Roll { tail: bool },
    Slip,
    Slide,
}

/// Seconds of footage a clip's file has, when the clip plays a file and can run out of it.
fn source_len(c: &Clip) -> Option<f64> {
    if !matches!(c.kind, AssetType::Video | AssetType::Audio | AssetType::Music) || c.frozen() {
        return None;
    }
    asset(&c.asset).map(|a| a.dur)
}

/// Whether a clip's footage can be moved under it (a speed ramp ties footage to time).
fn reframes(c: &Clip) -> bool {
    !c.ramped()
}

/// The longest a clip can be, from its in point, before its footage runs out.
fn max_dur(c: &Clip) -> f64 {
    match source_len(c) {
        Some(len) if !c.ramped() => ((len - c.in_) / c.speed()).max(0.0),
        _ => f64::INFINITY,
    }
}

/// The clip touching this one's head or tail on the same track.
fn neighbour(clips: &[Clip], c: &Clip, tail: bool) -> Option<usize> {
    let edge = if tail { c.start + c.dur } else { c.start };
    clips.iter().position(|k| k.id != c.id && k.track == c.track && if tail { (k.start - edge).abs() < SAME } else { (k.start + k.dur - edge).abs() < SAME })
}

/// The end of the clip before this one on its track (0 if none), and the start of the one after.
fn room(clips: &[Clip], c: &Clip) -> (f64, f64) {
    let before = clips.iter().filter(|k| k.id != c.id && k.track == c.track && k.start + k.dur <= c.start + SAME).fold(0.0f64, |m, k| m.max(k.start + k.dur));
    let after = clips.iter().filter(|k| k.id != c.id && k.track == c.track && k.start >= c.start + c.dur - SAME).fold(f64::INFINITY, |m, k| m.min(k.start));
    (before, after)
}

fn fit_fades(c: &mut Clip) {
    c.fade_in = c.fade_in.min(c.dur / 2.0);
    c.fade_out = c.fade_out.min(c.dur / 2.0);
}

/// Trim `id` by `dx` seconds (positive is later). The result is the timeline after the edit and
/// the distance actually applied, which is `dx` held back wherever footage or clips run out.
pub fn trim(base: &[Clip], id: &str, op: TrimOp, dx: f64, min: f64) -> Result<(Vec<Clip>, f64), &'static str> {
    let i = base.iter().position(|c| c.id == id).ok_or("That clip is gone")?;
    let c = base[i].clone();
    let mut out = base.to_vec();
    let s = c.speed();
    let clamp = |dx: f64, lo: f64, hi: f64| if lo > hi { 0.0 } else { dx.clamp(lo, hi) };
    let after = |k: &Clip| k.id != c.id && k.track == c.track && k.start >= c.start + c.dur - SAME;
    match op {
        TrimOp::RippleTail => {
            let dx = clamp(dx, min - c.dur, max_dur(&c) - c.dur);
            for k in out.iter_mut().filter(|k| after(k)) {
                k.start += dx;
            }
            out[i].dur = c.dur + dx;
            fit_fades(&mut out[i]);
            Ok((out, dx))
        }
        TrimOp::RippleHead => {
            if !reframes(&c) {
                return Err("This clip has a speed ramp. Trim its head with the Select tool.");
            }
            // Dragging the head in (positive) skips footage; out (negative) brings back footage
            // from before the in point, as far as there is any.
            let back = if source_len(&c).is_some() { c.in_ / s } else if c.frozen() || c.kind.is_audio() { 0.0 } else { f64::INFINITY };
            let dx = clamp(dx, -back, c.dur - min);
            for k in out.iter_mut().filter(|k| after(k)) {
                k.start -= dx;
            }
            let k = &mut out[i];
            k.dur = c.dur - dx;
            if !c.frozen() && matches!(c.kind, AssetType::Video | AssetType::Audio | AssetType::Music) {
                k.in_ = (c.in_ + dx * s).max(0.0);
            }
            // Keys stay on the same moment of the footage.
            keys::shift(&mut k.keys, dx);
            fit_fades(k);
            Ok((out, dx))
        }
        TrimOp::Roll { tail } => {
            let Some(j) = neighbour(base, &c, tail) else { return Err("Roll moves a cut between two clips that touch") };
            let (a, b) = if tail { (i, j) } else { (j, i) };
            let (ca, cb) = (base[a].clone(), base[b].clone());
            if !reframes(&cb) {
                return Err("The clip after the cut has a speed ramp, so its start can't roll");
            }
            let back_b = if source_len(&cb).is_some() { cb.in_ / cb.speed() } else if cb.frozen() || cb.kind.is_audio() { 0.0 } else { f64::INFINITY };
            let lo = (min - ca.dur).max(-back_b);
            let hi = (cb.dur - min).min(max_dur(&ca) - ca.dur);
            let dx = clamp(dx, lo, hi);
            out[a].dur = ca.dur + dx;
            fit_fades(&mut out[a]);
            let k = &mut out[b];
            k.start = cb.start + dx;
            k.dur = cb.dur - dx;
            if !cb.frozen() && matches!(cb.kind, AssetType::Video | AssetType::Audio | AssetType::Music) {
                k.in_ = (cb.in_ + dx * cb.speed()).max(0.0);
            }
            keys::shift(&mut k.keys, dx);
            fit_fades(k);
            Ok((out, dx))
        }
        TrimOp::Slip => {
            if !matches!(c.kind, AssetType::Video | AssetType::Audio | AssetType::Music) || c.frozen() {
                return Err("Slip shows a different part of a video or audio clip's footage");
            }
            if !reframes(&c) {
                return Err("This clip has a speed ramp, so its footage is tied to it");
            }
            // Dragging right pulls earlier footage into view, like sliding film through a gate.
            let hi = c.in_ / s;
            let lo = match source_len(&c) {
                Some(len) => -((len - c.in_) / s - c.dur).max(0.0),
                None => f64::NEG_INFINITY,
            };
            let dx = clamp(dx, lo, hi);
            out[i].in_ = (c.in_ - dx * s).max(0.0);
            Ok((out, dx))
        }
        TrimOp::Slide => {
            let (prev, next) = (neighbour(base, &c, false), neighbour(base, &c, true));
            let (gap_lo, gap_hi) = room(base, &c);
            let mut lo = match prev {
                Some(p) => min - base[p].dur,
                None => gap_lo - c.start,
            };
            let mut hi = match prev {
                Some(p) => max_dur(&base[p]) - base[p].dur,
                None => f64::INFINITY,
            };
            match next {
                Some(n) => {
                    let cn = &base[n];
                    if !reframes(cn) {
                        return Err("The clip after this one has a speed ramp, so its start can't move");
                    }
                    hi = hi.min(cn.dur - min);
                    let back = if source_len(cn).is_some() { cn.in_ / cn.speed() } else if cn.frozen() || cn.kind.is_audio() { 0.0 } else { f64::INFINITY };
                    lo = lo.max(-back);
                }
                None => hi = hi.min(gap_hi - (c.start + c.dur)),
            }
            let dx = clamp(dx, lo, hi);
            out[i].start = c.start + dx;
            if let Some(p) = prev {
                out[p].dur = base[p].dur + dx;
                fit_fades(&mut out[p]);
            }
            if let Some(n) = next {
                let cn = base[n].clone();
                let k = &mut out[n];
                k.start = cn.start + dx;
                k.dur = cn.dur - dx;
                if !cn.frozen() && matches!(cn.kind, AssetType::Video | AssetType::Audio | AssetType::Music) {
                    k.in_ = (cn.in_ + dx * cn.speed()).max(0.0);
                }
                keys::shift(&mut k.keys, dx);
                fit_fades(k);
            }
            Ok((out, dx))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::store::{default_tracks, make_clip};

    /// Three image clips back to back on V1 (images never run out of footage), 0–2, 2–5, 5–6.
    fn row() -> Vec<Clip> {
        let tracks = default_tracks();
        let img = crate::model::assets::images().into_iter().next().unwrap().id;
        [(0.0, 2.0), (2.0, 3.0), (5.0, 1.0)]
            .iter()
            .enumerate()
            .map(|(n, (s, d))| {
                let mut c = make_clip(&tracks, 0.0, &img, Some("V1"), Some(*s)).unwrap();
                c.id = format!("c{n}");
                c.dur = *d;
                c
            })
            .collect()
    }
    fn get<'a>(cs: &'a [Clip], id: &str) -> &'a Clip {
        cs.iter().find(|c| c.id == id).unwrap()
    }

    #[test]
    fn ripple_tail_moves_everything_after() {
        let (cs, dx) = trim(&row(), "c1", TrimOp::RippleTail, 1.0, 0.1).unwrap();
        assert_eq!(dx, 1.0);
        assert_eq!((get(&cs, "c1").start, get(&cs, "c1").dur), (2.0, 4.0));
        assert_eq!(get(&cs, "c2").start, 6.0);
        assert_eq!(get(&cs, "c0").start, 0.0);
        // Shortening pulls the rest in, and stops at the shortest a clip can be.
        let (cs, dx) = trim(&row(), "c1", TrimOp::RippleTail, -10.0, 0.1).unwrap();
        assert!((dx + 2.9).abs() < 1e-9 && (get(&cs, "c2").start - 2.1).abs() < 1e-9);
    }

    #[test]
    fn ripple_head_keeps_the_start_and_pulls_the_rest() {
        let (cs, _) = trim(&row(), "c1", TrimOp::RippleHead, 1.0, 0.1).unwrap();
        assert_eq!((get(&cs, "c1").start, get(&cs, "c1").dur), (2.0, 2.0));
        assert_eq!(get(&cs, "c2").start, 4.0);
    }

    #[test]
    fn roll_moves_only_the_cut() {
        let (cs, _) = trim(&row(), "c0", TrimOp::Roll { tail: true }, 0.5, 0.1).unwrap();
        assert_eq!(get(&cs, "c0").dur, 2.5);
        assert_eq!((get(&cs, "c1").start, get(&cs, "c1").dur), (2.5, 2.5));
        assert_eq!(get(&cs, "c2").start, 5.0);
        // The same cut from the other side.
        let (cs2, _) = trim(&row(), "c1", TrimOp::Roll { tail: false }, 0.5, 0.1).unwrap();
        assert_eq!(cs, cs2);
        // No neighbour at the first clip's head.
        assert!(trim(&row(), "c0", TrimOp::Roll { tail: false }, 0.5, 0.1).is_err());
    }

    #[test]
    fn slide_keeps_the_total_length() {
        let (cs, dx) = trim(&row(), "c1", TrimOp::Slide, 0.5, 0.1).unwrap();
        assert_eq!(dx, 0.5);
        assert_eq!(get(&cs, "c0").dur, 2.5);
        assert_eq!((get(&cs, "c1").start, get(&cs, "c1").dur), (2.5, 3.0));
        assert_eq!((get(&cs, "c2").start, get(&cs, "c2").dur), (5.5, 0.5));
        // The clip after can't get shorter than the minimum.
        let (cs, _) = trim(&row(), "c1", TrimOp::Slide, 5.0, 0.1).unwrap();
        assert!((get(&cs, "c2").dur - 0.1).abs() < 1e-9);
        let end: f64 = cs.iter().map(|c| c.start + c.dur).fold(0.0, f64::max);
        assert!((end - 6.0).abs() < 1e-9);
    }

    #[test]
    fn slip_needs_footage() {
        assert!(trim(&row(), "c1", TrimOp::Slip, 0.5, 0.1).is_err());
    }
}
