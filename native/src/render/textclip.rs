// Text clips: drawing with their entrance animation, and the box they occupy (for hit-testing
// and handles on the preview). Both lay lines out identically.

use super::canvas::{hex, pixmap, rgba, Canvas};
use super::text::{fill_text, measure, stroke_text, Align, Baseline, TextStyle};
use crate::model::assets::font_def;
use crate::model::time::{H, W};
use crate::model::types::{TextAlign, TextAnim, TextBlend, TextProps};

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
/// Draw a text clip at local time `lt` (seconds into the clip), in 1280×720 units: its
/// entrance animation, backing (box or per-line highlight), soft shadow and glow, outline, fill.
pub fn draw_text(c: &mut Canvas, t: &TextProps, lt: f32, alpha: f32) {
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
