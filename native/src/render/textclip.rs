// Text clips: drawing with their entrance animation, and the box they occupy (for hit-testing
// and handles on the preview). Both lay lines out identically.

use super::canvas::{hex, pixmap, rgba, with_alpha, Canvas};
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

/// Animations that move letters or words one at a time.
fn per_unit(a: TextAnim) -> Option<bool> {
    use TextAnim::*;
    match a {
        Wave | Drop | Tracking | Scramble | Flip | Spring => Some(false),
        Words | WordRise | WordPop => Some(true),
        _ => Option::None,
    }
}

/// Back-out spring from `from` to 1 over p in 0..1.
fn spring(p: f32, from: f32) -> f32 {
    let (c1, c3) = (2.4f32, 3.4f32);
    from + (1.0 - from) * (1.0 + c3 * (p - 1.0).powi(3) + c1 * (p - 1.0).powi(2))
}

/// Where letter or word `i` is at `lt`: offset, opacity and scale (x, y). `off` is how far the
/// unit's middle sits from its line's middle.
fn unit_pose(anim: TextAnim, i: usize, lt: f32, off: f32) -> (f32, f32, f32, f32, f32) {
    let i = i as f32;
    match anim {
        TextAnim::Wave => {
            let e = ease(((lt - i * 0.035) / 0.35).clamp(0.0, 1.0));
            (0.0, (1.0 - e) * 34.0, e, 1.0, 1.0)
        }
        TextAnim::Drop => {
            let e = ease(((lt - i * 0.04) / 0.4).clamp(0.0, 1.0));
            (0.0, -(1.0 - e) * 60.0, e, 1.0, 1.0)
        }
        TextAnim::Spring => {
            let p = ((lt - i * 0.04) / 0.32).clamp(0.0, 1.0);
            let k = spring(p, 0.0).max(0.0);
            (0.0, 0.0, (p * 5.0).min(1.0), k, k)
        }
        TextAnim::Flip => {
            let e = ease(((lt - i * 0.03) / 0.3).clamp(0.0, 1.0));
            (0.0, 0.0, (e * 2.0).min(1.0), 1.0, e.max(0.001))
        }
        TextAnim::Tracking => {
            let e = ease((lt / 0.9).clamp(0.0, 1.0));
            (off * 0.6 * (1.0 - e), 0.0, e, 1.0, 1.0)
        }
        TextAnim::Scramble => (0.0, 0.0, (lt / 0.1).min(1.0), 1.0, 1.0),
        TextAnim::Words => (0.0, 0.0, ((lt - i * 0.22) / 0.06).clamp(0.0, 1.0), 1.0, 1.0),
        TextAnim::WordRise => {
            let e = ease(((lt - i * 0.12) / 0.4).clamp(0.0, 1.0));
            (0.0, (1.0 - e) * 28.0, e, 1.0, 1.0)
        }
        TextAnim::WordPop => {
            let p = ((lt - i * 0.14) / 0.26).clamp(0.0, 1.0);
            let k = spring(p, 0.4);
            (0.0, 0.0, (p * 5.0).min(1.0), k, k)
        }
        _ => (0.0, 0.0, 1.0, 1.0, 1.0),
    }
}

/// Decode: letter `i` shows a random glyph until it settles, left to right.
fn scrambled(ch: char, i: usize, lt: f32) -> Option<char> {
    const GLYPHS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ0123456789#$%&*+=?";
    if ch.is_whitespace() || lt >= 0.25 + i as f32 * 0.04 {
        return None;
    }
    let h = ((i as u32).wrapping_mul(2_654_435_761) ^ ((lt * 20.0) as u32).wrapping_mul(40_503)).wrapping_mul(2_246_822_519);
    Some(GLYPHS[(h >> 16) as usize % GLYPHS.len()] as char)
}

/// Every number in `s` counted up to `p` (0..1) of its value, keeping its decimals and grouping.
fn count_up(s: &str, p: f32) -> String {
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < cs.len() {
        if !cs[i].is_ascii_digit() {
            out.push(cs[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < cs.len() && (cs[i].is_ascii_digit() || ((cs[i] == ',' || cs[i] == '.') && i + 1 < cs.len() && cs[i + 1].is_ascii_digit())) {
            i += 1;
        }
        let num: String = cs[start..i].iter().collect();
        let grouped = num.contains(',');
        let decimals = num.split_once('.').map(|(_, d)| d.len()).unwrap_or(0);
        let v: f64 = num.replace(',', "").parse().unwrap_or(0.0);
        let shown = format!("{:.*}", decimals, v * p as f64);
        if grouped {
            let (int, frac) = shown.split_once('.').map(|(a, b)| (a.to_string(), format!(".{b}"))).unwrap_or((shown.clone(), String::new()));
            let mut g = String::new();
            for (k, ch) in int.chars().enumerate() {
                if k > 0 && (int.len() - k) % 3 == 0 {
                    g.push(',');
                }
                g.push(ch);
            }
            out.push_str(&g);
            out.push_str(&frac);
        } else {
            out.push_str(&shown);
        }
    }
    out
}

/// The liquid glass panel: the backdrop (everything composited so far) frosted behind a
/// rounded translucent panel, tinted, with a specular sheen along the top and a fine light
/// border. The panel's path follows the canvas transform, so a placed, scaled or turned
/// text keeps its glass; the blurred backdrop is painted full-frame in device space, since
/// the clip is already in device space.
fn glass_panel(c: &mut Canvas, x: f32, y: f32, w: f32, h: f32, alpha: f32, tint: &str) {
    if alpha <= 0.01 || w <= 0.0 || h <= 0.0 {
        return;
    }
    let r = (h * 0.16).min(24.0).max(8.0);
    // Frost whatever is behind the text right now.
    let frost = crate::fx::engines::blur_of(&c.pm, 16.0);
    let panel = |c: &mut Canvas| {
        c.begin_path();
        c.round_rect(x, y, w, h, r);
    };
    c.save();
    panel(c);
    c.clip();
    // The frosted backdrop, full frame, device space (the clip already is).
    c.save();
    c.reset_transform();
    c.set_alpha(alpha);
    c.draw_image(&frost, 0.0, 0.0);
    c.restore();
    // Translucent white body over it, washed with the chosen tint.
    c.set_alpha(alpha);
    c.set_fill_color(rgba(255, 255, 255, 0.14));
    panel(c);
    c.fill();
    c.set_fill_color(with_alpha(hex(tint), 0.16));
    panel(c);
    c.fill();
    // Specular sheen: light pooling along the top, gone by mid-panel.
    c.set_fill(c.linear(x, y, x, y + h, &[(0.0, rgba(255, 255, 255, 0.4)), (0.28, rgba(255, 255, 255, 0.05)), (1.0, rgba(255, 255, 255, 0.0))]));
    panel(c);
    c.fill();
    // Fine inner border catching the light.
    c.set_stroke_color(rgba(255, 255, 255, 0.55));
    c.set_line_width(1.5);
    c.begin_path();
    c.round_rect(x + 0.75, y + 0.75, w - 1.5, h - 1.5, (r - 0.75).max(2.0));
    c.stroke();
    c.restore();
}

/// Flat text: its entrance animation, backing (box or per-line highlight), soft shadow and glow,
/// outline, fill, and accents (marker, underline, sheen, caret).
fn draw_flat(c: &mut Canvas, t: &TextProps, lt: f32, alpha: f32) {
    let e = ease((lt / 0.6).min(1.0));
    let (mut ox, mut oy, mut a, mut k, mut blur) = (0.0, 0.0, 1.0, 1.0f32, 0.0f32);
    let mut kx = 1.0f32;
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
        TextAnim::Descend => {
            a = e;
            oy = -(1.0 - e) * 36.0;
        }
        // Scale from 40% with a springy overshoot.
        TextAnim::Pop => {
            k = spring((lt / 0.28).clamp(0.0, 1.0), 0.4);
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
        TextAnim::Settle => {
            let p = ease((lt / 0.5).clamp(0.0, 1.0));
            k = 1.15 - 0.15 * p;
            a = p;
        }
        TextAnim::Squeeze => {
            kx = ease((lt / 0.45).clamp(0.0, 1.0)).max(0.001);
            a = (lt / 0.1).min(1.0);
        }
        TextAnim::Stretch => {
            let p = ease((lt / 0.6).clamp(0.0, 1.0));
            kx = 1.0 + 0.8 * (1.0 - p);
            a = p;
        }
        TextAnim::Blur => {
            let p = ease((lt / 0.5).clamp(0.0, 1.0));
            blur = (1.0 - p) * 26.0;
            a = p;
        }
        TextAnim::Fog => {
            let p = ease((lt / 0.9).clamp(0.0, 1.0));
            blur = (1.0 - p) * 30.0;
            oy = (1.0 - p) * 50.0;
            a = p;
        }
        TextAnim::Echo => {
            oy = (1.0 - ease((lt / 0.7).clamp(0.0, 1.0))) * 60.0;
            a = (lt / 0.15).min(1.0);
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
    // Typing: how many letters are out, and the line the caret is on.
    let mut caret: Option<usize> = None;
    if matches!(t.anim, TextAnim::Type | TextAnim::Caret) {
        let per = if t.anim == TextAnim::Caret { 0.06 } else { 0.045 };
        let mut n = (lt / per).floor() as i64;
        let last = lines.len().saturating_sub(1);
        lines = lines
            .into_iter()
            .enumerate()
            .map(|(i, l)| {
                let len = l.chars().count() as i64;
                if caret.is_none() && (n <= len || i == last) {
                    caret = Some(i);
                }
                let s: String = l.chars().take(n.max(0) as usize).collect();
                n -= len;
                s
            })
            .collect();
    }
    if t.anim == TextAnim::Count {
        let p = ease((lt / 1.2).clamp(0.0, 1.0));
        lines = lines.iter().map(|l| count_up(l, p)).collect();
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
    let size = lay.sizes.first().cloned().unwrap_or(60.0);
    let max_w = widths.iter().cloned().fold(0.0f32, f32::max);

    // Scale animations work around the text's centre.
    let scaled = k != 1.0 || kx != 1.0;
    if scaled {
        let cy = H() / 2.0 + t.y as f32;
        c.save();
        c.translate(lay.base_x, cy);
        c.scale(k * kx, k);
        c.translate(-lay.base_x, -cy);
    }
    // Wipe: everything left of a line moving to the right.
    let wiping = t.anim == TextAnim::Wipe;
    if wiping {
        let p = ease((lt / 0.6).clamp(0.0, 1.0));
        let x0 = left_of(max_w) - size;
        c.save();
        c.begin_path();
        c.rect(x0, top - size * 2.0, (max_w + size * 2.0) * p, lay.total + size * 4.0);
        c.clip();
    }

    // Liquid glass: a frosted panel that blurs whatever has been composited behind the
    // text, tinted, with a specular sheen and a fine light border. Behind everything else.
    if let Some(glass) = t.glass.as_deref().filter(|g| !g.is_empty() && *g != "none") {
        let bw = widths.iter().cloned().fold(f32::MIN, f32::max) + 56.0;
        let bx = left_of(bw - 56.0) - 28.0;
        glass_panel(c, bx + ox, top - 16.0 + oy, bw, lay.total + 32.0, alpha * a, glass);
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
    // A highlight behind each line: still, or swiped on like a marker.
    let marker = t.anim == TextAnim::Marker;
    if t.highlight.is_some() || marker {
        c.set_alpha(alpha * a);
        // Without a chosen colour, a marker that keeps the text readable: violet under light text,
        // yellow under dark.
        let light = crate::render::canvas::hex_rgb(&t.color).iter().zip([0.3f32, 0.59, 0.11]).map(|(c, w)| c / 255.0 * w).sum::<f32>() > 0.55;
        c.set_fill_color(hex(t.highlight.as_deref().unwrap_or(if light { "#7c3aed" } else { "#facc15" })));
        let mut y = top;
        for (i, w) in widths.iter().enumerate() {
            let p = if marker { ease(((lt - 0.15 - i as f32 * 0.2) / 0.45).clamp(0.0, 1.0)) } else { 1.0 };
            if *w > 0.0 && p > 0.0 {
                let pad = lay.sizes[i] * 0.22;
                c.begin_path();
                if marker {
                    c.round_rect(left_of(*w) - pad + ox, y + lay.lh[i] * 0.22, (w + pad * 2.0) * p, lay.lh[i] * 0.62, lay.sizes[i] * 0.06);
                } else {
                    c.round_rect(left_of(*w) - pad + ox, y + lay.lh[i] * 0.06, w + pad * 2.0, lay.lh[i] * 0.92, lay.sizes[i] * 0.18);
                }
                c.fill();
            }
            y += lay.lh[i];
        }
    }

    let outline = t.outline.as_ref().map(|o| (hex(o), t.outline_width as f32));
    let units = per_unit(t.anim);
    // The glyphs (outline under fill) in one colour, or their own colours.
    let paint = |c: &mut Canvas, only: Option<tiny_skia::Color>, a_lines: f32, dx: f32, dy: f32| {
        let mut y = top + dy;
        let mut count = 0usize;
        let blend = text_blend(t.blend);
        for (i, l) in lay.lines.iter().enumerate() {
            let line_a = (if i > 0 { 0.75 } else { 1.0 }) * a_lines;
            c.set_alpha(line_a);
            let st = style(t, i, lay.sizes[i]);
            let mut at = (lay.base_x + ox + dx, y + lay.lh[i] / 2.0);
            // Mask reveal: each line rises from behind its own lower edge.
            let revealing = t.anim == TextAnim::Reveal;
            if revealing {
                let p = ease(((lt - i as f32 * 0.12) / 0.55).clamp(0.0, 1.0));
                let pad = lay.sizes[i];
                c.save();
                c.begin_path();
                c.rect(left_of(widths[i]) + ox + dx - pad, y - lay.lh[i] * 0.1, widths[i] + pad * 2.0, lay.lh[i] * 1.2);
                c.clip();
                at.1 += (1.0 - p) * lay.lh[i] * 1.1;
            }
            if let Some(words) = units {
                // Letter by letter, or word by word, each with its own pose.
                let x0 = left_of(widths[i]) + ox + dx;
                let line_mid = x0 + widths[i] / 2.0;
                let mut pieces: Vec<(usize, usize)> = Vec::new();
                if words {
                    let mut start = None;
                    for (bi, ch) in l.char_indices() {
                        match (ch.is_whitespace(), start) {
                            (false, None) => start = Some(bi),
                            (true, Some(s0)) => {
                                pieces.push((s0, bi));
                                start = None;
                            }
                            _ => {}
                        }
                    }
                    if let Some(s0) = start {
                        pieces.push((s0, l.len()));
                    }
                } else {
                    pieces = l.char_indices().filter(|(_, ch)| !ch.is_whitespace()).map(|(bi, ch)| (bi, bi + ch.len_utf8())).collect();
                }
                for (s0, s1) in pieces {
                    let idx = count;
                    count += 1;
                    let piece = &l[s0..s1];
                    // Where the whole line puts this piece: measured through it, kerning with the
                    // letter before included, less its own width.
                    let uw = measure(piece, &st);
                    let px = x0 + measure(&l[..s1], &st) - uw;
                    let (udx, udy, ua, sx, sy) = unit_pose(t.anim, idx, lt, px + uw / 2.0 - line_mid);
                    if ua <= 0.0 {
                        continue;
                    }
                    let glyph: String = if t.anim == TextAnim::Scramble {
                        piece.chars().next().and_then(|ch| scrambled(ch, idx, lt)).map(String::from).unwrap_or_else(|| piece.to_string())
                    } else {
                        piece.to_string()
                    };
                    let (px, py) = (px + udx, at.1 + udy);
                    c.save();
                    if sx != 1.0 || sy != 1.0 {
                        c.translate(px + uw / 2.0, py);
                        c.scale(sx, sy);
                        c.translate(-(px + uw / 2.0), -py);
                    }
                    c.set_alpha(line_a * ua);
                    if let Some((oc, ow)) = outline {
                        stroke_text(c, &glyph, px, py, &st, Align::Left, Baseline::Middle, only.unwrap_or(oc), ow * 2.0);
                    }
                    if only.is_none() && blend != tiny_skia::BlendMode::SourceOver {
                        c.set_blend(blend);
                    }
                    fill_text(c, &glyph, px, py, &st, Align::Left, Baseline::Middle, only.unwrap_or(color));
                    c.restore();
                }
            } else {
                if let Some((oc, ow)) = outline {
                    stroke_text(c, l, at.0, at.1, &st, align(t.align), Baseline::Middle, only.unwrap_or(oc), ow * 2.0);
                }
                // Blend styles need the glyph fill to invert against everything already on this canvas.
                let saved = c.blend();
                if only.is_none() && blend != tiny_skia::BlendMode::SourceOver {
                    c.set_blend(blend);
                }
                fill_text(c, l, at.0, at.1, &st, align(t.align), Baseline::Middle, only.unwrap_or(color));
                c.set_blend(saved);
            }
            if revealing {
                c.restore();
            }
            y += lay.lh[i];
        }
    };
    // Soft passes: the glyphs alone on a layer, blurred, then laid on the canvas.
    let soft = |c: &mut Canvas, col: tiny_skia::Color, dx: f32, dy: f32, radius: f32, strength: f32| {
        let ts = c.transform();
        let dev = (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt();
        let mut layer = Canvas::from_pixmap(pixmap(c.width(), c.height()));
        layer.set_transform(ts.pre_translate(dx, dy));
        paint(&mut layer, Some(col), 1.0, 0.0, 0.0);
        // blur_of sizes its radius to the frame itself (1280-long-side units, like ours).
        let pm = if radius * dev >= 0.5 { crate::fx::engines::blur_of(&layer.into_pixmap(), radius) } else { layer.into_pixmap() };
        c.save();
        c.reset_transform();
        c.composite(&pm, (alpha * a * strength).min(1.0), tiny_skia::BlendMode::SourceOver);
        c.restore();
    };
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
    if t.anim == TextAnim::Echo {
        // Fading copies trailing below, closing in as the text settles.
        let p = ease((lt / 0.7).clamp(0.0, 1.0));
        for j in (1..=3).rev() {
            paint(c, None, a * alpha * (0.4 / j as f32) * (1.0 - p), 0.0, j as f32 * size * 0.35 * (1.0 - p));
        }
    }
    if blur > 0.3 {
        // Coming into focus: the whole text as one blurred layer.
        let ts = c.transform();
        let mut layer = Canvas::from_pixmap(pixmap(c.width(), c.height()));
        layer.set_transform(ts);
        paint(&mut layer, None, 1.0, 0.0, 0.0);
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
            paint(c, Some(hex("#ff2a4a")), a * alpha * 0.9, -j, 0.0);
            paint(c, Some(hex("#20e3ff")), a * alpha * 0.9, j, 0.0);
            c.set_blend(saved);
        }
        // Shimmer keeps the letters a little dim so the sheen reads on any colour.
        paint(c, None, a * alpha * if t.anim == TextAnim::Shimmer { 0.55 } else { 1.0 }, 0.0, 0.0);
    }
    if t.anim == TextAnim::Shimmer {
        // A slanted band of light crossing the letters, every 2.4 s.
        let p = (lt % 2.4) / 1.3;
        if p < 1.0 {
            let x0 = left_of(max_w) - size;
            let span = max_w + size * 2.0;
            let bx = x0 + span * p;
            let bw = size * 0.7;
            let (y0, y1) = (top - size * 0.2, top + lay.total + size * 0.2);
            let lean = (y1 - y0) * 0.35;
            c.save();
            c.begin_path();
            c.move_to(bx + lean, y0);
            c.line_to(bx + lean + bw, y0);
            c.line_to(bx + bw, y1);
            c.line_to(bx, y1);
            c.close_path();
            c.clip();
            paint(c, Some(tiny_skia::Color::WHITE), a * alpha * (1.0 - (p * 2.0 - 1.0).abs()).sqrt(), 0.0, 0.0);
            c.restore();
        }
    }
    if t.anim == TextAnim::Underline {
        // A line drawing itself under each line of text.
        let col = t.highlight.as_deref().map(hex).unwrap_or(color);
        let mut y = top;
        for (i, w) in widths.iter().enumerate() {
            let p = ease(((lt - 0.2 - i as f32 * 0.2) / 0.5).clamp(0.0, 1.0));
            if *w > 0.0 && p > 0.0 {
                let th = (lay.sizes[i] * 0.07).max(2.0);
                c.set_alpha(alpha * a);
                c.set_fill_color(col);
                c.begin_path();
                c.round_rect(left_of(*w) + ox, y + lay.lh[i] / 2.0 + lay.sizes[i] * 0.42, w * p, th, th / 2.0);
                c.fill();
            }
            y += lay.lh[i];
        }
    }
    if let (TextAnim::Caret, Some(li)) = (t.anim, caret) {
        // The caret blinks once the line is typed out.
        let typing = lay.lines.iter().map(|l| l.chars().count()).sum::<usize>() < t.content.chars().filter(|c| *c != '\n').count();
        if typing || (lt * 1.8) as i32 % 2 == 0 {
            let y: f32 = top + lay.lh[..li].iter().sum::<f32>();
            let s = lay.sizes[li];
            let x = left_of(widths[li]) + widths[li] + s * 0.06 + ox;
            c.set_alpha(alpha);
            c.fill_rect_color(x, y + lay.lh[li] / 2.0 - s * 0.42, (s * 0.06).max(2.0), s * 0.84, color);
        }
    }
    if wiping {
        c.restore();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::assets::{asset, TEXT_ANIMS};

    #[test]
    fn count_up_keeps_decimals_and_grouping() {
        assert_eq!(count_up("Ship 2,400 videos", 1.0), "Ship 2,400 videos");
        assert_eq!(count_up("Ship 2,400 videos", 0.5), "Ship 1,200 videos");
        assert_eq!(count_up("99.5% uptime", 1.0), "99.5% uptime");
        assert_eq!(count_up("99.5% uptime", 0.0), "0.0% uptime");
        assert_eq!(count_up("1,000,000", 1.0), "1,000,000");
        assert_eq!(count_up("v2.0, 3 apps.", 1.0), "v2.0, 3 apps.");
        assert_eq!(count_up("no numbers", 0.3), "no numbers");
        assert_eq!(count_up("", 0.3), "");
    }

    #[test]
    fn decode_settles_left_to_right() {
        assert!(scrambled('A', 0, 0.0).is_some());
        assert!(scrambled(' ', 0, 0.0).is_none());
        assert!(scrambled('A', 0, 0.3).is_none());
        // A later letter is still scrambled when an earlier one has settled.
        assert!(scrambled('A', 20, 0.3).is_some());
        assert!(scrambled('A', 20, 2.0).is_none());
    }

    #[test]
    fn letter_and_word_poses_end_at_rest() {
        for (a, _) in TEXT_ANIMS {
            if per_unit(a).is_none() {
                continue;
            }
            for i in 0..12 {
                let (dx, dy, al, sx, sy) = unit_pose(a, i, 10.0, 40.0);
                assert!(dx.abs() < 1e-3 && dy.abs() < 1e-3 && (al - 1.0).abs() < 1e-3 && (sx - 1.0).abs() < 1e-2 && (sy - 1.0).abs() < 1e-2, "{a:?} unit {i} not at rest: {:?}", (dx, dy, al, sx, sy));
            }
        }
    }

    fn render(t: &TextProps, lt: f32) -> Vec<u8> {
        let mut c = Canvas::new(1280, 720);
        draw_text(&mut c, t, lt, 1.0);
        c.into_pixmap().data().to_vec()
    }

    /// Every animation: moves at the start, and has settled into the same picture as no animation
    /// by 3 s (except the ones that keep going), with awkward text too.
    #[test]
    fn every_animation_settles_and_survives_edge_cases() {
        let base = asset("tx-title").and_then(|a| a.text.clone()).unwrap();
        for content in ["Ship 2,400 videos\nwithout a timeline", "", "a", "  spaced   out  ", "émoji 🎬 ünïcode", "\n\nblank lines\n"] {
            let mut t = base.clone();
            t.content = content.into();
            t.anim = TextAnim::None;
            let still = render(&t, 3.0);
            for (a, _) in TEXT_ANIMS {
                t.anim = a;
                for lt in [0.0, 0.01, 0.1, 0.3, 0.7, 1.5] {
                    render(&t, lt);
                }
                // Shimmer dims the letters for its sheen; marker and underline add marks; the
                // caret blinks; typing and words reflow centred text; flicker and glitch settle.
                let keeps = matches!(a, TextAnim::Shimmer | TextAnim::Marker | TextAnim::Underline | TextAnim::Caret);
                if !keeps && !content.trim().is_empty() {
                    let end = render(&t, 6.0);
                    let differ = end.iter().zip(&still).filter(|(x, y)| (**x as i32 - **y as i32).abs() > 24).count();
                    // Letter-by-letter drawing loses a little kerning; allow a few pixels of it.
                    assert!(differ < 1600, "{a:?} on {content:?} hasn't settled: {differ} bytes differ");
                }
            }
        }
    }

    #[test]
    fn animations_start_off_their_rest() {
        let mut t = asset("tx-title").and_then(|a| a.text.clone()).unwrap();
        t.content = "Ship 2,400 videos".into();
        for (a, _) in TEXT_ANIMS {
            if matches!(a, TextAnim::None | TextAnim::Marker | TextAnim::Underline | TextAnim::Shimmer) {
                continue;
            }
            t.anim = a;
            let (early, late) = (render(&t, 0.02), render(&t, 6.0));
            assert!(early != late, "{a:?} shows no entrance");
        }
        // The accents draw something extra once they've played.
        for a in [TextAnim::Marker, TextAnim::Underline] {
            t.anim = a;
            let with = render(&t, 3.0);
            t.anim = TextAnim::None;
            assert!(with != render(&t, 3.0), "{a:?} adds nothing");
        }
    }
}
