// Where each clip sits on the 1280×720 canvas, so the preview can be clicked, dragged, scaled and
// rotated directly. Mirrors the transforms in the compositor.

use crate::model::time::{H, W};
use crate::model::types::*;
use crate::render::textclip::{text_rect, Rect};

/// A clip's box on the canvas: centre, size after scaling, rotation in degrees.
#[derive(Clone, Copy, Debug)]
pub struct BoxR {
    pub cx: f32,
    pub cy: f32,
    pub w: f32,
    pub h: f32,
    pub rot: f32,
}

pub fn local_rect(c: &Clip) -> Rect {
    match &c.text {
        Some(t) => text_rect(t),
        None => Rect { x: 0.0, y: 0.0, w: W(), h: H() },
    }
}

/// Map a local point through the clip's transform: translate to centre + offset, rotate, scale.
pub fn to_canvas(c: &Clip, px: f32, py: f32) -> (f32, f32) {
    let a = (c.rot as f32).to_radians();
    let s = c.scale as f32;
    let (dx, dy) = ((px - W() / 2.0) * s, (py - H() / 2.0) * s);
    (W() / 2.0 + c.x as f32 + dx * a.cos() - dy * a.sin(), H() / 2.0 + c.y as f32 + dx * a.sin() + dy * a.cos())
}
pub fn to_local(c: &Clip, px: f32, py: f32) -> (f32, f32) {
    let a = (-c.rot as f32).to_radians();
    let s = if c.scale == 0.0 { 1.0 } else { c.scale as f32 };
    let (dx, dy) = (px - W() / 2.0 - c.x as f32, py - H() / 2.0 - c.y as f32);
    (W() / 2.0 + (dx * a.cos() - dy * a.sin()) / s, H() / 2.0 + (dx * a.sin() + dy * a.cos()) / s)
}

pub fn clip_box(c: &Clip) -> BoxR {
    let r = local_rect(c);
    let (cx, cy) = to_canvas(c, r.x + r.w / 2.0, r.y + r.h / 2.0);
    BoxR { cx, cy, w: r.w * c.scale as f32, h: r.h * c.scale as f32, rot: c.rot as f32 }
}

/// Corners of a box (nw, ne, se, sw) and the top-centre point, in canvas units.
pub fn corners(b: &BoxR) -> [(f32, f32); 5] {
    let a = b.rot.to_radians();
    let (c, s) = (a.cos(), a.sin());
    let p = |x: f32, y: f32| (b.cx + x * c - y * s, b.cy + x * s + y * c);
    let (hw, hh) = (b.w / 2.0, b.h / 2.0);
    [p(-hw, -hh), p(hw, -hh), p(hw, hh), p(-hw, hh), p(0.0, -hh)]
}

/// Visual clips on screen at `t`, front-most first: tracks from the top of the list down
/// (titles, then picture-in-picture, then footage); blended overlays (light leaks, grain) last,
/// so clicking through them picks what they cover.
pub fn visible_clips<'a>(clips: &'a [Clip], tracks: &[Track], t: f64) -> Vec<&'a Clip> {
    let at = |track: &str| {
        let mut v: Vec<&Clip> = clips.iter().filter(|c| c.track == track && t >= c.start && t < c.start + c.dur).collect();
        v.sort_by(|a, b| b.start.total_cmp(&a.start));
        v
    };
    let mut out = vec![];
    let mut overlays = vec![];
    for tr in tracks.iter().filter(|x| x.kind != TrackKind::Audio && !x.hidden) {
        for c in at(&tr.id) {
            if c.kind == AssetType::Overlay {
                overlays.push(c);
            } else {
                out.push(c);
            }
        }
    }
    out.extend(overlays);
    out
}

pub fn inside(c: &Clip, px: f32, py: f32) -> bool {
    let r = local_rect(c);
    let (x, y) = to_local(c, px, py);
    x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h
}

/// The clip under a canvas point, front-most first.
pub fn hit_test<'a>(clips: &'a [Clip], tracks: &[Track], px: f32, py: f32, t: f64) -> Option<&'a Clip> {
    visible_clips(clips, tracks, t).into_iter().find(|c| inside(c, px, py))
}
