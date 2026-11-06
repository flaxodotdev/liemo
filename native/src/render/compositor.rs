// The compositor: every visible clip at time `t`, track by track, with effect stacks and
// transitions. Everything is laid out in 1280×720 units and scaled, so text and graphics stay
// sharp at any output size.

use super::canvas::{pixmap, Canvas};
use super::textclip::draw_text;
use crate::fx::engines::run_effects;
use crate::fx::transitions::{transition_def, TransitionDef};
use crate::model::assets::{asset, bitmap_of, Asset};
use crate::model::time::{H, W};
use crate::model::types::*;
use std::sync::Arc;
use tiny_skia::{Pixmap, Transform};

/// Where decoded video pictures come from (per clip, so clips can sit at different source times).
pub trait Frames {
    /// The picture for `clip` at source time `src_t` (seconds into the media file).
    fn video(&mut self, clip: &Clip, src_t: f64) -> Option<Arc<Pixmap>>;
}
pub struct NoFrames;
impl Frames for NoFrames {
    fn video(&mut self, _: &Clip, _: f64) -> Option<Arc<Pixmap>> {
        None
    }
}

pub struct Scene<'a> {
    pub clips: &'a [Clip],
    pub tracks: &'a [Track],
}
impl Scene<'_> {
    fn hidden(&self, track: &str) -> bool {
        self.tracks.iter().find(|t| t.id == track).map(|t| t.hidden).unwrap_or(false)
    }
}

pub fn draw_cover(c: &mut Canvas, src: &Pixmap, w: f32, h: f32) {
    let (sw, sh) = (src.width() as f32, src.height() as f32);
    if sw == 0.0 || sh == 0.0 {
        return;
    }
    let k = (w / sw).max(h / sh);
    c.draw_image_scaled(src, (w - sw * k) / 2.0, (h - sh * k) / 2.0, sw * k, sh * k);
}

/// Draw a clip's content with its transform, in 1280×720 units.
fn draw_content(c: &mut Canvas, clip: &Clip, a: &Asset, t: f64, alpha: f32, frames: &mut dyn Frames) {
    let lt = t - clip.start;
    let st = lt + clip.in_;
    c.translate(W() / 2.0 + clip.x as f32, H() / 2.0 + clip.y as f32);
    c.rotate((clip.rot as f32).to_radians());
    c.scale(clip.scale as f32, clip.scale as f32);
    c.translate(-W() / 2.0, -H() / 2.0);
    match a.kind {
        AssetType::Scene if a.shader.is_some() => {
            // Rendered at the layer's pixel size, then placed like any full frame.
            if let Some(pm) = crate::fx::paper::render(a.shader.unwrap(), st as f32, c.width(), c.height()) {
                c.draw_image_scaled(&pm, 0.0, 0.0, W(), H());
            }
        }
        AssetType::Scene | AssetType::Overlay => {
            if let Some(d) = a.draw {
                d(c, st as f32)
            }
        }
        AssetType::Image => {
            if let Some(b) = bitmap_of(a) {
                c.draw_image_scaled(&b, 0.0, 0.0, W(), H());
            }
        }
        AssetType::Photo => {
            if let Some(b) = &a.bitmap {
                draw_cover(c, b, W(), H());
            }
        }
        AssetType::Video => {
            if let Some(f) = frames.video(clip, st) {
                draw_cover(c, &f, W(), H());
            }
        }
        AssetType::Text => {
            if let Some(tx) = &clip.text {
                draw_text(c, tx, lt as f32, alpha);
            }
        }
        _ => {}
    }
}

/// Draw one clip onto `c` (any 16:9 size).
pub fn draw_clip(c: &mut Canvas, clip: &Clip, t: f64, frames: &mut dyn Frames) {
    let Some(a) = asset(&clip.asset) else { return };
    let alpha = (clip.opacity * clip_fade(clip, t)) as f32;
    let (cw, ch) = (c.width(), c.height());
    let s = cw as f32 / W();
    let blend = a.blend.map(super::canvas::blend_named).unwrap_or(tiny_skia::BlendMode::SourceOver);

    if clip.effects.iter().any(|e| e.enabled) {
        // Render the clip alone, run its effect stack, then composite the result like any other clip.
        let mut layer = Canvas::from_pixmap(pixmap(cw, ch));
        layer.set_transform(Transform::from_scale(s, s));
        draw_content(&mut layer, clip, &a, t, 1.0, frames);
        let out = run_effects(&layer.into_pixmap(), &clip.effects, (t - clip.start) as f32, clip.dur as f32, effects_on_object(clip));
        c.save();
        c.reset_transform();
        c.composite(&out, alpha, blend);
        c.restore();
        return;
    }
    c.save();
    c.set_transform(Transform::from_scale(s, s));
    c.set_alpha(alpha);
    c.set_blend(blend);
    draw_content(c, clip, &a, t, alpha, frames);
    c.restore();
}

pub struct TransitionWindow<'a> {
    pub a: Option<&'a Clip>,
    pub b: &'a Clip,
    pub def: &'static TransitionDef,
    pub from: f64,
    pub to: f64,
}

/// Where each transition plays. Between two touching clips it straddles the cut; with nothing
/// right before the clip it plays over the clip's first moments, from whatever is below.
pub fn transition_windows<'a>(track: &[&'a Clip]) -> Vec<TransitionWindow<'a>> {
    let mut out = vec![];
    for (i, b) in track.iter().enumerate() {
        let Some(tr) = &b.transition else { continue };
        let Some(def) = transition_def(&tr.id) else { continue };
        let prev = if i > 0 { Some(track[i - 1]) } else { None };
        let a = prev.filter(|p| (p.start + p.dur - b.start).abs() < 1.5 / 30.0);
        let d = (1.0 / 30.0f64).max(tr.dur.min(b.dur).min(a.map(|a| a.dur).unwrap_or(f64::INFINITY)));
        let from = if a.is_some() { b.start - d / 2.0 } else { b.start };
        out.push(TransitionWindow { a, b, def, from, to: from + d });
    }
    out
}

/// Render one clip alone, at time `t` held inside the clip, onto a transparent frame.
fn clip_frame(clip: &Clip, t: f64, w: u32, h: u32, frames: &mut dyn Frames) -> Pixmap {
    let mut g = Canvas::from_pixmap(pixmap(w, h));
    // Outside its own range a clip holds its first or last frame.
    draw_clip(&mut g, clip, t.max(clip.start).min(clip.start + clip.dur - 1e-3), frames);
    g.into_pixmap()
}

fn draw_transition(c: &mut Canvas, w: &TransitionWindow, p: f64, t: f64, frames: &mut dyn Frames) {
    let (cw, ch) = (c.width(), c.height());
    let a = w.a.map(|a| clip_frame(a, t, cw, ch, frames));
    let b = clip_frame(w.b, t, cw, ch, frames);
    c.save();
    c.reset_transform();
    (w.def.draw)(c, a.as_ref(), &b, p.clamp(0.0, 1.0) as f32, t as f32);
    c.restore();
}

/// Composite every visible clip at time `t`.
pub fn render_frame(c: &mut Canvas, scene: &Scene, t: f64, frames: &mut dyn Frames) {
    c.reset_state();
    let [r, g, b] = crate::model::time::env().bg;
    c.fill_all(crate::render::canvas::rgb(r, g, b));
    for tid in ["V1", "V2", "T1"] {
        if scene.hidden(tid) {
            continue;
        }
        let mut track: Vec<&Clip> = scene.clips.iter().filter(|k| k.track == tid).collect();
        track.sort_by(|a, b| a.start.total_cmp(&b.start));
        let mut handled: Vec<&str> = vec![];
        for w in transition_windows(&track) {
            if t < w.from || t >= w.to {
                continue;
            }
            draw_transition(c, &w, (t - w.from) / (w.to - w.from), t, frames);
            if let Some(a) = w.a {
                handled.push(&a.id);
            }
            handled.push(&w.b.id);
        }
        for k in track {
            if !handled.contains(&k.id.as_str()) && t >= k.start && t < k.start + k.dur {
                draw_clip(c, k, t, frames);
            }
        }
    }
}

