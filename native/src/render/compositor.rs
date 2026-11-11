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
use std::f32::consts::PI;
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
    let st = clip.in_ + clip.src_offset(lt);
    place(c, clip, a.kind, lt);
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
                // The main track fills the frame; on other tracks (stickers, picture-in-picture)
                // the whole picture shows.
                if clip.track == "V1" {
                    draw_cover(c, b, W(), H());
                } else {
                    let (sw, sh) = (b.width() as f32, b.height() as f32);
                    let k = (W() / sw).min(H() / sh);
                    c.draw_image_scaled(b, (W() - sw * k) / 2.0, (H() - sh * k) / 2.0, sw * k, sh * k);
                }
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

/// Move the canvas into a clip's own space: its position, turn, scale and flips, with its motion
/// at `lt` seconds in.
fn place(c: &mut Canvas, clip: &Clip, kind: AssetType, lt: f64) {
    let m = motion_at(clip, lt);
    c.translate(W() / 2.0 + clip.x as f32 + m.dx, H() / 2.0 + clip.y as f32 + m.dy);
    c.rotate((clip.rot as f32 + m.rot).to_radians());
    let (fx, fy) = match &clip.motion {
        Some(mo) if kind != AssetType::Text => (if mo.flip_h { -1.0 } else { 1.0 }, if mo.flip_v { -1.0 } else { 1.0 }),
        _ => (1.0, 1.0),
    };
    let k = clip.scale as f32 * m.scale;
    c.scale(k * fx * m.sx, k * fy * m.sy);
    c.translate(-W() / 2.0, -H() / 2.0);
}

/// The clip's mask as alpha on a `cw`×`ch` frame: its shape in the clip's own space, softened by
/// its feather, flipped when inverted.
fn mask_alpha(clip: &Clip, kind: AssetType, m: &Mask, lt: f64, cw: u32, ch: u32) -> Pixmap {
    let s = cw as f32 / W();
    let mut mc = Canvas::from_pixmap(pixmap(cw, ch));
    mc.set_transform(Transform::from_scale(s, s));
    place(&mut mc, clip, kind, lt);
    mc.translate(W() / 2.0 + m.x as f32, H() / 2.0 + m.y as f32);
    mc.rotate((m.rot as f32).to_radians());
    let (w, h) = (m.w.max(1.0) as f32, m.h.max(1.0) as f32);
    mc.set_fill_color(super::canvas::rgb(255, 255, 255));
    mc.begin_path();
    if m.shape == "rect" {
        mc.round_rect(-w / 2.0, -h / 2.0, w, h, (m.radius as f32).min(w.min(h) / 2.0));
    } else {
        mc.ellipse(0.0, 0.0, w / 2.0, h / 2.0, 0.0, 2.0 * PI, false);
    }
    mc.fill();
    let mut pm = mc.into_pixmap();
    if m.feather > 0.5 {
        pm = crate::fx::engines::blur_of(&pm, (m.feather as f32 * s * clip.scale as f32 / 2.0).max(0.5));
    }
    if m.invert {
        for p in pm.data_mut().chunks_exact_mut(4) {
            let a = 255 - p[3];
            p.copy_from_slice(&[a, a, a, a]);
        }
    }
    pm
}

fn bounce_out(x: f32) -> f32 {
    let (n1, d1) = (7.5625f32, 2.75f32);
    if x < 1.0 / d1 {
        n1 * x * x
    } else if x < 2.0 / d1 {
        let x = x - 1.5 / d1;
        n1 * x * x + 0.75
    } else if x < 2.5 / d1 {
        let x = x - 2.25 / d1;
        n1 * x * x + 0.9375
    } else {
        let x = x - 2.625 / d1;
        n1 * x * x + 0.984375
    }
}

/// Where a clip's entrance or exit animation has it at `lt` seconds in.
pub struct MotionNow {
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
    pub rot: f32,
    pub alpha: f32,
    /// Squash and stretch on top of `scale`.
    pub sx: f32,
    pub sy: f32,
}

pub fn motion_at(clip: &Clip, lt: f64) -> MotionNow {
    let mut n = MotionNow { dx: 0.0, dy: 0.0, scale: 1.0, rot: 0.0, alpha: 1.0, sx: 1.0, sy: 1.0 };
    let Some(m) = &clip.motion else { return n };
    // `p` runs 0→1 over the animation: in from the start, out toward the end (reversed).
    let mut apply = |kind: &str, p: f32| {
        if kind.is_empty() || p >= 1.0 {
            return;
        }
        let p = p.clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - p).powi(3);
        match kind {
            "fade" => n.alpha *= e,
            "zoom-in" => {
                n.scale *= 0.6 + 0.4 * e;
                n.alpha *= e;
            }
            "zoom-out" => {
                n.scale *= 1.5 - 0.5 * e;
                n.alpha *= e;
            }
            "slide-left" => n.dx += (1.0 - e) * W(),
            "slide-right" => n.dx -= (1.0 - e) * W(),
            "slide-up" => n.dy += (1.0 - e) * H(),
            "slide-down" => n.dy -= (1.0 - e) * H(),
            "spin" => {
                n.rot -= (1.0 - e) * 180.0;
                n.scale *= 0.4 + 0.6 * e;
                n.alpha *= e;
            }
            "pop" => {
                let (c1, c3) = (2.4f32, 3.4f32);
                n.scale *= 0.3 + 0.7 * (1.0 + c3 * (p - 1.0).powi(3) + c1 * (p - 1.0).powi(2));
                n.alpha *= (p * 4.0).min(1.0);
            }
            "whip-left" | "whip-right" => {
                // Much faster than a slide: most of the travel happens in the first instant.
                let e = 1.0 - (1.0 - p).powi(6);
                n.dx += (1.0 - e) * W() * if kind == "whip-left" { 1.2 } else { -1.2 };
            }
            "drop" => {
                n.dy -= (1.0 - bounce_out(p)) * H();
                n.alpha *= (p * 6.0).min(1.0);
            }
            "rise" => {
                n.dy += (1.0 - e) * H() * 0.25;
                n.alpha *= e;
            }
            "swing" => {
                n.rot += (1.0 - p) * 24.0 * (p * 14.0).cos();
                n.alpha *= (p * 5.0).min(1.0);
            }
            "flip" => {
                n.sx *= (e * PI / 2.0).sin().max(0.02);
            }
            "stretch" => {
                let (c1, c3) = (1.8f32, 2.8f32);
                let back = 1.0 + c3 * (p - 1.0).powi(3) + c1 * (p - 1.0).powi(2);
                n.sx *= 1.6 - 0.6 * back;
                n.sy *= 0.4 + 0.6 * back;
                n.alpha *= (p * 4.0).min(1.0);
            }
            "shake" => {
                let a = (1.0 - p) * 40.0;
                n.dx += a * (p * 70.0).sin();
                n.dy += a * 0.5 * (p * 53.0).cos();
            }
            _ => {}
        }
    };
    if m.anim_in_dur > 0.0 {
        apply(&m.anim_in, (lt / m.anim_in_dur) as f32);
    }
    if m.anim_out_dur > 0.0 {
        apply(&m.anim_out, ((clip.dur - lt) / m.anim_out_dur) as f32);
    }
    // All the time, for the length of the clip.
    let sp = m.anim_loop_speed.clamp(0.25, 4.0) as f32;
    let l = lt as f32 * sp;
    match m.anim_loop.as_str() {
        "breathe" => n.scale *= 1.0 + 0.045 * (l * PI).sin(),
        "float" => n.dy += 16.0 * (l * 1.7).sin(),
        "wiggle" => n.rot += 4.0 * (l * 7.0).sin(),
        "pulse" => n.scale *= 1.0 + 0.09 * (-(l * 2.0).fract() * 6.0).exp(),
        "spin" => n.rot += l * 90.0,
        "shake" => {
            n.dx += 7.0 * ((l * 47.0).sin() + (l * 31.0).cos()) / 2.0;
            n.dy += 7.0 * ((l * 41.0).cos() + (l * 29.0).sin()) / 2.0;
        }
        "sway" => {
            n.rot += 5.0 * (l * 1.5).sin();
            n.dx += 12.0 * (l * 1.5).sin();
        }
        _ => {}
    }
    n
}

/// Draw one clip onto `c` (any 16:9 size).
pub fn draw_clip(c: &mut Canvas, clip: &Clip, t: f64, frames: &mut dyn Frames) {
    let Some(a) = asset(&clip.asset) else { return };
    // Keyframed properties take their value at this moment.
    let posed;
    let clip = if clip.keys.is_empty() {
        clip
    } else {
        posed = clip.posed(t);
        &posed
    };
    let alpha = (clip.opacity * clip_fade(clip, t)) as f32 * motion_at(clip, t - clip.start).alpha;
    let (cw, ch) = (c.width(), c.height());
    let s = cw as f32 / W();
    // The asset's blend, with the text style's own blend winning for text clips.
    let text = clip.text.as_ref().filter(|_| a.kind == AssetType::Text);
    let blend = match (&clip.blend, text) {
        (Some(b), _) => super::canvas::blend_named(b),
        (None, Some(tx)) => super::textclip::text_blend(tx.blend),
        (None, None) => a.blend.map(super::canvas::blend_named).unwrap_or(tiny_skia::BlendMode::SourceOver),
    };
    // A layer whenever effects run: the clip renders alone first, takes its effect stack, then
    // composites like any other clip. Blended text goes the same way even without effects, so the
    // blend applies to the composite (the picture on the canvas) rather than a transparent layer.
    let layered = clip.effects.iter().any(|e| e.enabled) || clip.mask.is_some() || (text.is_some() || clip.blend.is_some()) && blend != tiny_skia::BlendMode::SourceOver;

    if layered {
        // Render the clip alone, run its effect stack, then composite the result like any other clip.
        let mut layer = Canvas::from_pixmap(pixmap(cw, ch));
        layer.set_transform(Transform::from_scale(s, s));
        draw_content(&mut layer, clip, &a, t, 1.0, frames);
        let mut out = if clip.effects.iter().any(|e| e.enabled) {
            run_effects(&layer.into_pixmap(), &clip.effects, (t - clip.start) as f32, clip.dur as f32, effects_on_object(clip))
        } else {
            layer.into_pixmap()
        };
        if let Some(m) = &clip.mask {
            let mask = mask_alpha(clip, a.kind, m, t - clip.start, cw, ch);
            let mut mc = Canvas::from_pixmap(out);
            mc.composite(&mask, 1.0, tiny_skia::BlendMode::DestinationIn);
            out = mc.into_pixmap();
        }
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

// ---------- Text zoom: the next clip seen through the title's letters, which then grow ----------

/// The title a Text zoom cut opens through: the text clip on screen as the first clip ends.
fn zoom_title<'a>(clips: &'a [Clip], w: &TransitionWindow) -> Option<&'a Clip> {
    if w.def.id != "text-zoom" {
        return None;
    }
    clips
        .iter()
        .filter(|k| k.track == "T1" && k.text.is_some() && k.start <= w.from + 1e-3 && k.start + k.dur > w.from - 0.5)
        .max_by(|a, b| a.start.total_cmp(&b.start))
}

/// The title's words alone, white and plain, as a mask.
fn mask_props(title: &Clip) -> TextProps {
    let mut p = title.text.clone().unwrap_or_else(|| crate::model::assets::text_presets()[0].text.clone().unwrap());
    p.color = "#ffffff".into();
    p.outline = None;
    p.shadow = None;
    p.glow = None;
    p.highlight = None;
    p.boxed = false;
    p.blend = TextBlend::Normal;
    p
}

/// The title clip's own placement (offset and scale), in design units.
fn title_place(c: &mut Canvas, title: &Clip) {
    c.translate(W() / 2.0 + title.x as f32, H() / 2.0 + title.y as f32);
    c.scale(title.scale as f32, title.scale as f32);
    c.translate(-W() / 2.0, -H() / 2.0);
}

/// A point inside a letter, nearest the middle of the words: zooming in there fills the frame
/// with the inside of the letter rather than the gap between two.
fn zoom_pivot(title: &Clip, props: &TextProps, lt: f32) -> (f32, f32) {
    let gw = 320u32;
    let gh = ((gw as f32 * H() / W()).round() as u32).max(16);
    let k = gw as f32 / W();
    let mut m = Canvas::from_pixmap(pixmap(gw, gh));
    m.set_transform(Transform::from_scale(k, k));
    title_place(&mut m, title);
    draw_text(&mut m, props, lt, 1.0);
    let pm = m.into_pixmap();
    let (cx, cy) = ((W() / 2.0 + title.x as f32) * k, (H() / 2.0 + title.y as f32 + props.y as f32 * title.scale as f32) * k);
    let d = pm.data();
    let mut best = (cx, cy, f32::MAX);
    for y in 0..gh {
        for x in 0..gw {
            if d[((y * gw + x) * 4 + 3) as usize] > 230 {
                let dist = (x as f32 - cx).powi(2) + (y as f32 - cy).powi(2);
                if dist < best.2 {
                    best = (x as f32 + 0.5, y as f32 + 0.5, dist);
                }
            }
        }
    }
    (best.0 / k, best.1 / k)
}

/// `b` (at its time `tb`) showing only inside the title's letters, the letters grown `z` times
/// around a point inside them; `lt` is the title's own time (its entrance animation).
fn text_window(c: &mut Canvas, title: &Clip, b: &Clip, tb: f64, z: f32, lt: f32, alpha: f32, frames: &mut dyn Frames) {
    let (cw, ch) = (c.width(), c.height());
    let props = mask_props(title);
    let (px, py) = zoom_pivot(title, &props, lt.max(0.0));
    let mut m = Canvas::from_pixmap(pixmap(cw, ch));
    m.set_transform(Transform::from_scale(cw as f32 / W(), cw as f32 / W()));
    m.translate(px, py);
    m.scale(z, z);
    m.translate(-px, -py);
    title_place(&mut m, title);
    draw_text(&mut m, &props, lt, 1.0);
    let mut layer = Canvas::from_pixmap(clip_frame(b, tb, cw, ch, frames));
    layer.composite(&m.into_pixmap(), 1.0, tiny_skia::BlendMode::DestinationIn);
    c.save();
    c.reset_transform();
    c.composite(&layer.into_pixmap(), alpha, tiny_skia::BlendMode::SourceOver);
    c.restore();
}

/// The cut itself: the first clip, and over it the second through the letters as they swell
/// until the second fills the frame.
fn draw_text_zoom(c: &mut Canvas, w: &TransitionWindow, title: &Clip, p: f64, t: f64, frames: &mut dyn Frames) {
    if let Some(a) = w.a {
        draw_clip(c, a, t.min(a.start + a.dur - 1e-3), frames);
    }
    let p = p.clamp(0.0, 1.0) as f32;
    // Slow at first, then rushing in: about 90× by the end.
    let z = 90f32.powf(p.powf(1.7));
    text_window(c, title, w.b, t, z, (t - title.start) as f32, 1.0, frames);
    if p > 0.85 {
        let (cw, ch) = (c.width(), c.height());
        let b = clip_frame(w.b, t, cw, ch, frames);
        c.save();
        c.reset_transform();
        c.composite(&b, (p - 0.85) / 0.15, tiny_skia::BlendMode::SourceOver);
        c.restore();
    }
}

fn draw_transition(c: &mut Canvas, w: &TransitionWindow, p: f64, t: f64, frames: &mut dyn Frames) {
    let (cw, ch) = (c.width(), c.height());
    let a = w.a.map(|a| clip_frame(a, t, cw, ch, frames));
    let b = clip_frame(w.b, t, cw, ch, frames);
    let p = p.clamp(0.0, 1.0) as f32;
    c.save();
    c.reset_transform();
    if w.b.track == "V1" {
        (w.def.draw)(c, a.as_ref(), &b, p, t as f32);
    } else {
        // Titles and overlays: the transition happens to the object, not the frame. On its own
        // layer, and (unless it moves the object) kept to where the object is, so a dip to black
        // darkens the letters rather than the whole picture.
        let mut layer = Canvas::from_pixmap(pixmap(cw, ch));
        (w.def.draw)(&mut layer, a.as_ref(), &b, p, t as f32);
        let mut out = layer.into_pixmap();
        if w.def.group != crate::fx::transitions::TransitionGroup::Motion {
            keep_within(&mut out, a.as_ref(), &b);
        }
        c.composite(&out, 1.0, tiny_skia::BlendMode::SourceOver);
    }
    c.restore();
}

/// Limit `out` to where `a` or `b` has something (their larger alpha), scaling premultiplied
/// pixels down where the transition painted beyond them.
fn keep_within(out: &mut Pixmap, a: Option<&Pixmap>, b: &Pixmap) {
    let (ad, bd) = (a.map(|a| a.data()), b.data());
    for (i, px) in out.data_mut().chunks_exact_mut(4).enumerate() {
        let cap = bd[i * 4 + 3].max(ad.map(|d| d[i * 4 + 3]).unwrap_or(0)) as u32;
        let alpha = px[3] as u32;
        if alpha > cap {
            for v in px.iter_mut() {
                *v = (*v as u32 * cap / alpha.max(1)) as u8;
            }
        }
    }
}

/// Composite every visible clip at time `t`.
pub fn render_frame(c: &mut Canvas, scene: &Scene, t: f64, frames: &mut dyn Frames) {
    c.reset_state();
    let [r, g, b] = crate::model::time::env().bg;
    c.fill_all(crate::render::canvas::rgb(r, g, b));
    // Text zoom cuts on the main track, with the title each one opens through.
    let mut main: Vec<&Clip> = scene.clips.iter().filter(|k| k.track == "V1").collect();
    main.sort_by(|a, b| a.start.total_cmp(&b.start));
    let zooms: Vec<(TransitionWindow, &Clip)> =
        if scene.hidden("V1") { vec![] } else { transition_windows(&main).into_iter().filter_map(|w| zoom_title(scene.clips, &w).map(|title| (w, title))).collect() };
    // Picture tracks from the bottom of the list up: lower tracks are further back.
    let order: Vec<&str> = scene.tracks.iter().rev().filter(|t| t.kind != TrackKind::Audio).map(|t| t.id.as_str()).collect();
    let order = if order.is_empty() { vec!["V1", "V2", "T1"] } else { order };
    for tid in order {
        if scene.hidden(tid) {
            continue;
        }
        let mut track: Vec<&Clip> = scene.clips.iter().filter(|k| k.track == tid).collect();
        track.sort_by(|a, b| a.start.total_cmp(&b.start));
        let mut handled: Vec<&str> = vec![];
        if tid == "T1" {
            // A title that a Text zoom opens through is a window onto the next clip until the cut,
            // then the cut draws it.
            for (w, title) in &zooms {
                if t >= title.start && t < w.to {
                    handled.push(&title.id);
                    if t < w.from && t < title.start + title.dur {
                        text_window(c, title, w.b, w.b.start, 1.0, (t - title.start) as f32, crate::model::types::clip_fade(title, t) as f32, frames);
                    }
                }
            }
        }
        for w in transition_windows(&track) {
            if t < w.from || t >= w.to {
                continue;
            }
            match zooms.iter().find(|(z, _)| tid == "V1" && z.b.id == w.b.id) {
                Some((_, title)) => draw_text_zoom(c, &w, title, (t - w.from) / (w.to - w.from), t, frames),
                None => draw_transition(c, &w, (t - w.from) / (w.to - w.from), t, frames),
            }
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

