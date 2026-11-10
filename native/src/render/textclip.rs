// Text clips: drawing with their entrance animation, and the box they occupy (for hit-testing
// and handles on the preview). Both lay lines out identically.

use super::canvas::{hex, pixmap, rgba, Canvas};
use super::text::{fill_text, measure, stroke_text, Align, Baseline, TextStyle};
use crate::model::assets::font_def;
use crate::model::time::{H, W};
use crate::model::types::{TextAlign, TextAnim, TextBlend, TextProps};
use tiny_skia::Pixmap;

fn ease(p: f32) -> f32 {
    1.0 - (1.0 - p).powi(3)
}

struct Layout {
    lines: Vec<String>,
    sizes: Vec<f32>,
    lh: Vec<f32>,
    total: f32,
    base_x: f32,
}

fn layout(t: &TextProps, lines: Vec<String>) -> Layout {
    let n = lines.len();
    let sizes: Vec<f32> = (0..n).map(|i| if i > 0 && t.boxed && n > 1 { t.size as f32 * 0.6 } else { t.size as f32 }).collect();
    let k = if t.font == "Bebas Neue" { 0.95 } else { 1.12 };
    let lh: Vec<f32> = sizes.iter().map(|s| s * k).collect();
    let total = lh.iter().sum();
    let base_x = match t.align {
        TextAlign::Left => 120.0,
        TextAlign::Right => W() - 120.0,
        TextAlign::Center => W() / 2.0,
    };
    Layout { lines, sizes, lh, total, base_x }
}

fn style<'a>(t: &'a TextProps, i: usize, size: f32) -> TextStyle<'a> {
    let f = font_def(&t.font);
    TextStyle { family: f.family, weight: if i > 0 { 500.0 } else { t.weight as f32 }, size, spacing: f.track * size }
}

fn align(a: TextAlign) -> Align {
    match a {
        TextAlign::Left => Align::Left,
        TextAlign::Center => Align::Center,
        TextAlign::Right => Align::Right,
    }
}

/// The blend a glyph layer needs for a text blend style, or source-over for normal text.
pub fn text_blend(b: TextBlend) -> tiny_skia::BlendMode {
    match b {
        TextBlend::Normal => tiny_skia::BlendMode::SourceOver,
        TextBlend::Invert => tiny_skia::BlendMode::Difference,
        TextBlend::Exclusion => tiny_skia::BlendMode::Exclusion,
    }
}

/// Draw a text clip at local time `lt` (seconds into the clip), in 1280×720 units.
/// Draw a text clip at local time `lt` (seconds into the clip), in 1280×720 units. Tilted text is
/// drawn flat on a layer, then laid onto its turned plane in perspective.
pub fn draw_text(c: &mut Canvas, t: &TextProps, lt: f32, alpha: f32) {
    if t.tilt_x.abs() < 0.05 && t.tilt_y.abs() < 0.05 {
        return draw_flat(c, t, lt, alpha);
    }
    let ts = c.transform();
    let mut layer = Canvas::from_pixmap(pixmap(c.width(), c.height()));
    layer.set_transform(ts);
    draw_flat(&mut layer, t, lt, alpha);
    let r = text_rect(t);
    // The text's middle, and how far its look (glow, shadow) reaches, in device pixels.
    let map = |x: f32, y: f32| (ts.sx * x + ts.kx * y + ts.tx, ts.ky * x + ts.sy * y + ts.ty);
    let (cx, cy) = map(r.x + r.w / 2.0, r.y + r.h / 2.0);
    let k = (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt();
    let (hw, hh) = (r.w * k * 0.5 + r.h * k * 0.6, r.h * k * 0.5 + r.h * k * 0.6);
    let warped = tilt(&layer.into_pixmap(), cx, cy, hw, hh, t.tilt_x as f32, t.tilt_y as f32);
    c.save();
    c.reset_transform();
    c.composite(&warped, 1.0, text_blend(t.blend));
    c.restore();
}

/// `src` turned in 3D about (`cx`, `cy`) by `rx` (tip, around the horizontal axis) and `ry`
/// (swing, around the vertical one), in perspective. Only the region within `hw`×`hh` of the
/// centre (where the text and its glow are) is carried over.
fn tilt(src: &Pixmap, cx: f32, cy: f32, hw: f32, hh: f32, rx: f32, ry: f32) -> Pixmap {
    let (w, h) = (src.width() as i32, src.height() as i32);
    let mut out = pixmap(src.width(), src.height());
    let (ax, ay) = (rx.to_radians(), ry.to_radians());
    let (sx, cxr, sy, cyr) = (ax.sin(), ax.cos(), ay.sin(), ay.cos());
    // Rotation R = Rx · Ry: where the plane's own axes and its normal end up.
    let ex = [cyr, sx * sy, -cxr * sy];
    let ey = [0.0, cxr, sx];
    let n = [sy, -sx * cyr, cxr * cyr];
    let f = (w.max(h) as f32) * 1.4; // the eye's distance: a natural, not fish-eyed, perspective
    // Where the region's corners land on screen bounds the pixels to fill.
    let project = |x: f32, y: f32| {
        let p = [ex[0] * x + ey[0] * y, ex[1] * x + ey[1] * y, ex[2] * x + ey[2] * y];
        let d = (f + p[2]).max(f * 0.05);
        (cx + f * p[0] / d, cy + f * p[1] / d)
    };
    let corners = [project(-hw, -hh), project(hw, -hh), project(-hw, hh), project(hw, hh)];
    let x0 = corners.iter().map(|p| p.0).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
    let x1 = corners.iter().map(|p| p.0).fold(f32::MIN, f32::max).ceil().min(w as f32) as i32;
    let y0 = corners.iter().map(|p| p.1).fold(f32::MAX, f32::min).floor().max(0.0) as i32;
    let y1 = corners.iter().map(|p| p.1).fold(f32::MIN, f32::max).ceil().min(h as f32) as i32;
    let s = src.data();
    let d = out.data_mut();
    let sample = |x: f32, y: f32, ch: usize| -> f32 {
        let (x, y) = (x - 0.5, y - 0.5);
        let (ix, iy) = (x.floor() as i32, y.floor() as i32);
        let (fx, fy) = (x - ix as f32, y - iy as f32);
        let at = |px: i32, py: i32| if px >= 0 && py >= 0 && px < w && py < h { s[((py * w + px) * 4) as usize + ch] as f32 } else { 0.0 };
        let top = at(ix, iy) * (1.0 - fx) + at(ix + 1, iy) * fx;
        let bottom = at(ix, iy + 1) * (1.0 - fx) + at(ix + 1, iy + 1) * fx;
        top * (1.0 - fy) + bottom * fy
    };
    for py in y0..y1 {
        for px in x0..x1 {
            // The ray from the eye through this pixel, met with the turned plane.
            let dir = [px as f32 + 0.5 - cx, py as f32 + 0.5 - cy, f];
            let nd = n[0] * dir[0] + n[1] * dir[1] + n[2] * dir[2];
            if nd.abs() < 1e-4 {
                continue;
            }
            let t = n[2] * f / nd;
            if t <= 0.0 {
                continue;
            }
            let p = [dir[0] * t, dir[1] * t, dir[2] * t - f];
            let u = p[0] * ex[0] + p[1] * ex[1] + p[2] * ex[2];
            let v = p[0] * ey[0] + p[1] * ey[1] + p[2] * ey[2];
            if u.abs() > hw || v.abs() > hh {
                continue;
            }
            let o = ((py * w + px) * 4) as usize;
            let a = sample(cx + u, cy + v, 3);
            if a < 0.5 {
                continue;
            }
            for ch in 0..4 {
                d[o + ch] = sample(cx + u, cy + v, ch).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// Flat text: its entrance animation, backing (box or per-line highlight), soft shadow and glow,
/// outline, fill.
fn draw_flat(c: &mut Canvas, t: &TextProps, lt: f32, alpha: f32) {
    let e = ease((lt / 0.6).min(1.0));
    let (mut ox, mut oy, mut a, mut k, mut blur) = (0.0, 0.0, 1.0, 1.0f32, 0.0f32);
    match t.anim {
        TextAnim::Fade => a = e,
        TextAnim::Rise => {
            a = e;
            oy = (1.0 - e) * 40.0;
        }
        TextAnim::Slide => {
            a = e;
            ox = -(1.0 - e) * 80.0;
        }
        // Scale from 40% with a springy overshoot.
        TextAnim::Pop => {
            let p = (lt / 0.28).clamp(0.0, 1.0);
            let (c1, c3) = (2.4f32, 3.4f32);
            k = 0.4 + 0.6 * (1.0 + c3 * (p - 1.0).powi(3) + c1 * (p - 1.0).powi(2));
            a = (lt / 0.06).min(1.0);
        }
        TextAnim::Bounce => {
            oy = -(1.0 - bounce_out((lt / 0.75).clamp(0.0, 1.0))) * 260.0;
            a = (lt / 0.08).min(1.0);
        }
        TextAnim::Zoom => {
            let p = ease((lt / 0.4).clamp(0.0, 1.0));
            k = 2.0 - p;
            a = p;
        }
        TextAnim::Blur => {
            let p = ease((lt / 0.5).clamp(0.0, 1.0));
            blur = (1.0 - p) * 26.0;
            a = p;
        }
        // On, off, on: a sign warming up.
        TextAnim::Flicker => {
            let k = (lt * 22.0) as u32;
            a = if lt > 0.7 { 1.0 } else if (k.wrapping_mul(2_654_435_761) >> 28) % 3 == 0 { 0.08 } else { 0.9 };
        }
        TextAnim::Glitch => {
            a = (lt / 0.04).min(1.0);
            if lt < 0.5 {
                ox += (lt * 61.0).sin() * 12.0 * (1.0 - lt / 0.5);
            }
        }
        _ => {}
    }
    let glitch = if t.anim == TextAnim::Glitch && lt < 0.5 { 1.0 - lt / 0.5 } else { 0.0 };
    let mut lines: Vec<String> = t.content.split('\n').map(String::from).collect();
    if t.anim == TextAnim::Type {
        let mut n = (lt / 0.045).floor() as i64;
        lines = lines
            .into_iter()
            .map(|l| {
                let len = l.chars().count() as i64;
                let s: String = l.chars().take(n.max(0) as usize).collect();
                n -= len;
                s
            })
            .collect();
    }
    let lay = layout(t, lines);
    let top = H() / 2.0 + t.y as f32 - lay.total / 2.0 + oy;
    let widths: Vec<f32> = lay.lines.iter().enumerate().map(|(i, l)| measure(l, &style(t, i, lay.sizes[i]))).collect();
    let color = hex(&t.color);
    let left_of = |w: f32| match t.align {
        TextAlign::Left => lay.base_x,
        TextAlign::Right => lay.base_x - w,
        TextAlign::Center => lay.base_x - w / 2.0,
    };

    // Scale animations work around the text's centre.
    let scaled = k != 1.0;
    if scaled {
        let cy = H() / 2.0 + t.y as f32;
        c.save();
        c.translate(lay.base_x, cy);
        c.scale(k, k);
        c.translate(-lay.base_x, -cy);
    }

    if t.boxed {
        let bw = widths.iter().cloned().fold(f32::MIN, f32::max) + 48.0;
        let bx = left_of(bw - 48.0) - 24.0;
        c.set_alpha(alpha * a);
        c.set_fill_color(rgba(0, 0, 0, 0.55));
        c.begin_path();
        c.round_rect(bx + ox, top - 14.0, bw, lay.total + 28.0, 12.0);
        c.fill();
        if t.align == TextAlign::Left {
            let accent = if t.color == "#ffffff" { hex("#ff7a3d") } else { color };
            c.fill_rect_color(bx + ox, top - 14.0, 6.0, lay.total + 28.0, accent);
        }
    }
    if let Some(hl) = &t.highlight {
        // A rounded block behind each line, hugging its width.
        c.set_alpha(alpha * a);
        c.set_fill_color(hex(hl));
        let mut y = top;
        for (i, w) in widths.iter().enumerate() {
            if *w > 0.0 {
                let pad = lay.sizes[i] * 0.22;
                c.begin_path();
                c.round_rect(left_of(*w) - pad + ox, y + lay.lh[i] * 0.06, w + pad * 2.0, lay.lh[i] * 0.92, lay.sizes[i] * 0.18);
                c.fill();
            }
            y += lay.lh[i];
        }
    }

    let outline = t.outline.as_ref().map(|o| (hex(o), t.outline_width as f32));
    // The glyphs (outline under fill) in one colour, or their own colours.
    let paint = |c: &mut Canvas, only: Option<tiny_skia::Color>, a_lines: f32, dx: f32| {
        let mut y = top;
        let mut letters = 0usize;
        for (i, l) in lay.lines.iter().enumerate() {
            c.set_alpha((if i > 0 { 0.75 } else { 1.0 }) * a_lines);
            let st = style(t, i, lay.sizes[i]);
            let at = (lay.base_x + ox + dx, y + lay.lh[i] / 2.0);
            if t.anim == TextAnim::Wave {
                // Letter by letter, each rising in a beat after the one before.
                let x0 = left_of(widths[i]) + ox + dx;
                for (bi, ch) in l.char_indices() {
                    let e = ease(((lt - letters as f32 * 0.035) / 0.35).clamp(0.0, 1.0));
                    letters += 1;
                    if e <= 0.0 || ch.is_whitespace() {
                        continue;
                    }
                    let letter = &l[bi..bi + ch.len_utf8()];
                    let (px, py) = (x0 + measure(&l[..bi], &st), at.1 + (1.0 - e) * 34.0);
                    c.set_alpha((if i > 0 { 0.75 } else { 1.0 }) * a_lines * e);
                    if let Some((oc, ow)) = outline {
                        stroke_text(c, letter, px, py, &st, Align::Left, Baseline::Middle, only.unwrap_or(oc), ow * 2.0);
                    }
                    fill_text(c, letter, px, py, &st, Align::Left, Baseline::Middle, only.unwrap_or(color));
                }
                y += lay.lh[i];
                continue;
            }
            if let Some((oc, ow)) = outline {
                stroke_text(c, l, at.0, at.1, &st, align(t.align), Baseline::Middle, only.unwrap_or(oc), ow * 2.0);
            }
            // Blend styles need the glyph fill to invert against everything already on this canvas.
            let saved = c.blend();
            let blend = text_blend(t.blend);
            if only.is_none() && blend != tiny_skia::BlendMode::SourceOver {
                c.set_blend(blend);
            }
            fill_text(c, l, at.0, at.1, &st, align(t.align), Baseline::Middle, only.unwrap_or(color));
            c.set_blend(saved);
            y += lay.lh[i];
        }
    };
    // Soft passes: the glyphs alone on a layer, blurred, then laid on the canvas.
    let soft = |c: &mut Canvas, col: tiny_skia::Color, dx: f32, dy: f32, radius: f32, strength: f32| {
        let ts = c.transform();
        let dev = (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt();
        let mut layer = Canvas::from_pixmap(pixmap(c.width(), c.height()));
        layer.set_transform(ts.pre_translate(dx, dy));
        paint(&mut layer, Some(col), 1.0, 0.0);
        // blur_of sizes its radius to the frame itself (1280-long-side units, like ours).
        let pm = if radius * dev >= 0.5 { crate::fx::engines::blur_of(&layer.into_pixmap(), radius) } else { layer.into_pixmap() };
        c.save();
        c.reset_transform();
        c.composite(&pm, (alpha * a * strength).min(1.0), tiny_skia::BlendMode::SourceOver);
        c.restore();
    };
    let size = lay.sizes.first().cloned().unwrap_or(60.0);
    if let Some(sh) = &t.shadow {
        let mut col = hex(sh);
        col.apply_opacity(0.8);
        soft(c, col, 0.0, size * 0.06, size * 0.07, 1.0);
    }
    if let Some(g) = &t.glow {
        let col = hex(g);
        soft(c, col, 0.0, 0.0, size * 0.28, 1.0);
        soft(c, col, 0.0, 0.0, size * 0.09, 0.9);
    }
    if blur > 0.3 {
        // Coming into focus: the whole text as one blurred layer.
        let ts = c.transform();
        let dev = (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt();
        let mut layer = Canvas::from_pixmap(pixmap(c.width(), c.height()));
        layer.set_transform(ts);
        paint(&mut layer, None, 1.0, 0.0);
        let _ = dev;
        let pm = crate::fx::engines::blur_of(&layer.into_pixmap(), blur);
        c.save();
        c.reset_transform();
        c.composite(&pm, alpha * a, tiny_skia::BlendMode::SourceOver);
        c.restore();
    } else {
        if glitch > 0.0 {
            // Red and cyan ghosts pulling apart, then snapping back.
            let j = glitch * 14.0 * (0.6 + 0.4 * (lt * 47.0).sin());
            let saved = c.blend();
            c.set_blend(tiny_skia::BlendMode::Screen);
            paint(c, Some(hex("#ff2a4a")), a * alpha * 0.9, -j);
            paint(c, Some(hex("#20e3ff")), a * alpha * 0.9, j);
            c.set_blend(saved);
        }
        paint(c, None, a * alpha, 0.0);
    }
    if scaled {
        c.restore();
    }
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

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The untransformed rectangle a text clip draws into (matches draw_text's layout).
pub fn text_rect(t: &TextProps) -> Rect {
    let lay = layout(t, t.content.split('\n').map(String::from).collect());
    let widths: Vec<f32> = lay
        .lines
        .iter()
        .enumerate()
        .map(|(i, l)| measure(if l.is_empty() { " " } else { l }, &style(t, i, lay.sizes[i])))
        .collect();
    let top = H() / 2.0 + t.y as f32 - lay.total / 2.0;
    let max_w = widths.iter().cloned().fold(8.0, f32::max);
    if t.boxed {
        let bw = max_w + 48.0;
        let bx = match t.align {
            TextAlign::Left => lay.base_x - 24.0,
            TextAlign::Right => lay.base_x - bw + 24.0,
            TextAlign::Center => lay.base_x - bw / 2.0,
        };
        return Rect { x: bx, y: top - 14.0, w: bw, h: lay.total + 28.0 };
    }
    let x = match t.align {
        TextAlign::Left => lay.base_x,
        TextAlign::Right => lay.base_x - max_w,
        TextAlign::Center => lay.base_x - max_w / 2.0,
    };
    Rect { x: x - 8.0, y: top - 4.0, w: max_w + 16.0, h: lay.total + 8.0 }
}
