// Effect engines. Each one reads `src` and paints the result into `dst` (same size, cleared).
// Sizes are expressed against a frame whose long side is 1280 and scaled, so thumbnails and
// vertical or square canvases match the full landscape render.
// Hot loops (glyph stamping, dots, kernels, per-pixel maps) work on pixel buffers directly.

use super::registry::{dot_palette, effect_def, flag, glyph_set, gradient_map, num, palette, text, Engine, GlyphSet, Shape};
use crate::model::time::rng;
use crate::model::types::{ClipEffect, Params};
use crate::render::canvas::{from_straight, hex, hex_rgb, map_pixels, pixmap, resample_area, resize_bilinear, resize_nearest, rgba, to_straight, Canvas};
use crate::render::text::{glyph_tile, TextStyle};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::{Arc, OnceLock};
use tiny_skia::{BlendMode, LineCap, PathBuilder, Pixmap, Rect};

fn clamp01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}
fn lum(r: f32, g: f32, b: f32) -> f32 {
    (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0
}
/// Smooth pseudo-random wobble in -1..1 for shakes and jitters.
fn wobble(t: f32, seed: f32) -> f32 {
    (t * 7.1 + seed).sin() * 0.5 + (t * 13.7 + seed * 2.3).sin() * 0.3 + (t * 29.3 + seed * 4.1).sin() * 0.2
}
const BAYER: [f32; 16] = [0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0];

/// Scale of a frame relative to the 1280 design size (long side, so portrait frames match).
fn unit_of(pm: &Pixmap) -> f32 {
    pm.width().max(pm.height()) as f32 / 1280.0
}

/// Soft blur by shrinking and scaling back up. `radius` is in 1280-long-side pixels.
pub fn blur_of(src: &Pixmap, radius: f32) -> Pixmap {
    let (w, h) = (src.width(), src.height());
    let k = (radius * unit_of(src) * 0.7).max(1.0);
    let small = resample_area(src, ((w as f32 / k).round() as u32).max(1), ((h as f32 / k).round() as u32).max(1));
    resize_bilinear(&small, w, h)
}

/// Each channel multiplied by a colour, alpha kept (multiply-fill then destination-in the source).
fn tinted(src: &Pixmap, col: [f32; 3]) -> Pixmap {
    let mut out = src.clone();
    for px in out.data_mut().chunks_exact_mut(4) {
        for c in 0..3 {
            px[c] = (px[c] as f32 * col[c] / 255.0).round() as u8;
        }
    }
    out
}

// ---------- ASCII / glyph mosaic ----------
struct Atlas {
    tiles: Vec<Pixmap>,
}
fn atlas_for(set_id: &str, cell: u32) -> Arc<Atlas> {
    static C: OnceLock<Mutex<HashMap<(String, u32), Arc<Atlas>>>> = OnceLock::new();
    let cache = C.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (set_id.to_string(), cell);
    if let Some(a) = cache.lock().get(&key) {
        return a.clone();
    }
    let set = glyph_set(set_id);
    let cf = cell as f32;
    let tiles: Vec<Pixmap> = match set {
        GlyphSet::Chars { chars, emoji, .. } => chars
            .chars()
            .map(|ch| {
                let st = if *emoji {
                    TextStyle { family: "Inter", weight: 400.0, size: (cf * 0.86).round(), spacing: 0.0 }
                } else {
                    TextStyle { family: "JetBrains Mono", weight: 600.0, size: (cf * 1.08).round(), spacing: 0.0 }
                };
                glyph_tile(&ch.to_string(), cell, &st, if *emoji { cf * 0.04 } else { cf * 0.06 })
            })
            .collect(),
        GlyphSet::Shape { shape, levels, .. } => (0..*levels)
            .map(|i| {
                let mut g = Canvas::new(cell, cell);
                let s = if *levels > 1 { i as f32 / (*levels - 1) as f32 } else { 1.0 };
                if s > 0.0 {
                    g.set_fill_color(hex("#fff"));
                    g.set_stroke_color(hex("#fff"));
                    g.translate(cf / 2.0, cf / 2.0);
                    g.begin_path();
                    match shape {
                        Shape::Circle => {
                            g.circle(0.0, 0.0, cf / 2.0 * 1.12 * s.sqrt());
                            g.fill();
                        }
                        Shape::Square => {
                            let d = cf * s.sqrt();
                            g.fill_rect(-d / 2.0, -d / 2.0, d, d);
                        }
                        Shape::Diamond => {
                            let d = cf / 2.0 * 1.35 * s.sqrt();
                            g.move_to(0.0, -d);
                            g.line_to(d, 0.0);
                            g.line_to(0.0, d);
                            g.line_to(-d, 0.0);
                            g.fill();
                        }
                        Shape::Cross => {
                            let t = (cf * 0.42 * s).max(1.0);
                            let l = cf * 0.9;
                            g.fill_rect(-t / 2.0, -l / 2.0, t, l);
                            g.fill_rect(-l / 2.0, -t / 2.0, l, t);
                        }
                        Shape::Hatch => {
                            g.set_line_width((cf * 0.36 * s).max(0.6));
                            g.set_line_cap(LineCap::Square);
                            g.move_to(-cf / 2.0, cf / 2.0);
                            g.line_to(cf / 2.0, -cf / 2.0);
                            if s > 0.55 {
                                g.move_to(-cf / 2.0, -cf / 2.0);
                                g.line_to(cf / 2.0, cf / 2.0);
                            }
                            g.stroke();
                        }
                        Shape::Ring => {
                            g.set_line_width((cf * 0.22 * s + 0.5).max(0.8));
                            g.circle(0.0, 0.0, cf * 0.32);
                            g.stroke();
                            if s > 0.8 {
                                g.begin_path();
                                g.circle(0.0, 0.0, cf * 0.12);
                                g.fill();
                            }
                        }
                    }
                }
                g.into_pixmap()
            })
            .collect(),
    };
    let a = Arc::new(Atlas { tiles });
    let mut c = cache.lock();
    if c.len() > 60 {
        c.clear();
    }
    c.insert(key, a.clone());
    a
}

/// Source-over a premultiplied tile into a buffer at an integer position, scaled to `size` by
/// nearest sampling when needed, with a global alpha.
fn stamp(dst: &mut [u8], dw: usize, dh: usize, tile: &Pixmap, x0: i32, y0: i32, size: usize, alpha: f32) {
    let ts = tile.width() as usize;
    let td = tile.data();
    for ty in 0..size {
        let y = y0 + ty as i32;
        if y < 0 || y as usize >= dh {
            continue;
        }
        let sy = (ty * ts / size).min(ts - 1);
        for tx in 0..size {
            let x = x0 + tx as i32;
            if x < 0 || x as usize >= dw {
                continue;
            }
            let sx = (tx * ts / size).min(ts - 1);
            let si = (sy * ts + sx) * 4;
            let sa = td[si + 3] as f32 * alpha;
            if sa <= 0.0 {
                continue;
            }
            let o = (y as usize * dw + x as usize) * 4;
            let inv = 1.0 - sa / 255.0;
            for c in 0..3 {
                dst[o + c] = (td[si + c] as f32 * alpha + dst[o + c] as f32 * inv).round().min(255.0) as u8;
            }
            dst[o + 3] = (sa + dst[o + 3] as f32 * inv).round().min(255.0) as u8;
        }
    }
}

/// Composite `under` beneath `dst` (destination-over) at an alpha.
fn under(dst: &mut Pixmap, under: &Pixmap, alpha: f32) {
    let u = under.data();
    for (px, up) in dst.data_mut().chunks_exact_mut(4).zip(u.chunks_exact(4)) {
        let inv = 1.0 - px[3] as f32 / 255.0;
        if inv <= 0.0 {
            continue;
        }
        for c in 0..4 {
            px[c] = (px[c] as f32 + up[c] as f32 * alpha * inv).round().min(255.0) as u8;
        }
    }
}
fn under_color(dst: &mut Pixmap, rgb: [u8; 3]) {
    for px in dst.data_mut().chunks_exact_mut(4) {
        let inv = 1.0 - px[3] as f32 / 255.0;
        if inv <= 0.0 {
            continue;
        }
        for c in 0..3 {
            px[c] = (px[c] as f32 + rgb[c] as f32 * inv).round().min(255.0) as u8;
        }
        px[3] = 255;
    }
}

fn ascii(src: &Pixmap, dst: &mut Pixmap, p: &Params, t: f32, object: bool) {
    let (w, h) = (dst.width() as usize, dst.height() as usize);
    let k = unit_of(dst);
    let cell_px = (num(p, "cell", 12.0) as f32 * k).max(2.0);
    let cols = (w as f32 / cell_px).ceil() as usize;
    let rows = (h as f32 / cell_px).ceil() as usize;
    let set_id = text(p, "set", "classic");
    let set = glyph_set(set_id);
    let contrast = num(p, "contrast", 1.3) as f32;
    let invert = flag(p, "invert");
    // On a single object (a title, a placed photo) there's no background box, just the glyphs.
    let color_mode = text(p, "color", "source");
    let bg = if object { "none" } else { text(p, "bg", "black") };

    // 1. Sample: one pixel per cell.
    let small = resample_area(src, cols as u32, rows as u32);
    let px = to_straight(&small);
    // 2. Stamp one glyph per cell from the atlas (white on transparent).
    let tile_size = cell_px.round().max(4.0) as u32;
    let atlas = atlas_for(set_id, tile_size);
    let levels = atlas.tiles.len().max(1);
    let (rain, is_shape, emoji) = match set {
        GlyphSet::Chars { rain, emoji, .. } => (*rain, false, *emoji),
        GlyphSet::Shape { .. } => (false, true, false),
    };
    let mut out = pixmap(w as u32, h as u32);
    {
        let d = out.data_mut();
        for y in 0..rows {
            for x in 0..cols {
                let i = (y * cols + x) * 4;
                let a = px[i + 3] as f32 / 255.0;
                if a == 0.0 {
                    continue;
                }
                let mut l = lum(px[i] as f32, px[i + 1] as f32, px[i + 2] as f32) * a;
                l = clamp01((l - 0.5) * contrast + 0.5);
                if invert {
                    l = 1.0 - l;
                }
                let (level, ga);
                if rain {
                    // Glyphs change as columns "fall"; brightness comes from alpha.
                    let drift = (t * 9.0 + x as f32 * 7.31).floor() as i64;
                    let (xi, yi) = (x as i64, y as i64);
                    level = ((xi * 131 + yi * 17 + drift * 29 + ((xi * 7) ^ (yi + drift))) % levels as i64).unsigned_abs() as usize;
                    let head = ((t * 12.0 + x as f32 * 3.7) % (rows as f32 + 8.0)) - y as f32;
                    ga = clamp01(l * 1.15 + if head > 0.0 && head < 1.5 { 0.6 } else { 0.0 });
                    if ga < 0.04 {
                        continue;
                    }
                } else {
                    level = (l * (levels - 1) as f32).round() as usize;
                    ga = 1.0;
                    if level == 0 && is_shape {
                        continue;
                    }
                }
                let x0 = (x as f32 * cell_px).round() as i32;
                let y0 = (y as f32 * cell_px).round() as i32;
                let size = (((x + 1) as f32 * cell_px).round() as i32 - x0).max(1) as usize;
                stamp(d, w, h, &atlas.tiles[level.min(levels - 1)], x0, y0, size, ga);
            }
        }
    }
    // 3. Colour the glyphs: keep emoji colours, otherwise recolour (source-in).
    if !emoji && color_mode != "mono" {
        let tint = hex_rgb(text(p, "tint", "#22c55e"));
        let sd = small.data();
        let d = out.data_mut();
        for y in 0..h {
            let cy = ((y as f32 / cell_px) as usize).min(rows - 1);
            for x in 0..w {
                let o = (y * w + x) * 4;
                let a = d[o + 3] as f32 / 255.0;
                if a == 0.0 {
                    continue;
                }
                if color_mode == "tint" {
                    for c in 0..3 {
                        d[o + c] = (tint[c] * a).round() as u8;
                    }
                } else {
                    let cx = ((x as f32 / cell_px) as usize).min(cols - 1);
                    let si = (cy * cols + cx) * 4;
                    for c in 0..4 {
                        d[o + c] = (sd[si + c] as f32 * a).round() as u8;
                    }
                }
            }
        }
    }
    // 4. Background goes underneath.
    if bg == "dim" {
        under(&mut out, src, 0.22);
    }
    if bg != "none" {
        under_color(&mut out, [0, 0, 0]);
    }
    *dst = out;
}

// ---------- Pixelate ----------
fn pixelate(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    let (w, h) = (dst.width(), dst.height());
    let size = (num(p, "size", 16.0) as f32 * unit_of(dst)).max(1.0);
    let cols = ((w as f32 / size).round() as u32).max(1);
    let rows = ((h as f32 / size).round() as u32).max(1);
    *dst = resize_nearest(&resample_area(src, cols, rows), w, h);
}

// ---------- Ordered dither to a palette ----------
fn dither(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    let (w, h) = (dst.width(), dst.height());
    let size = (num(p, "pixel", 3.0) as f32 * unit_of(dst)).max(1.0);
    let cols = ((w as f32 / size).round() as u32).max(1);
    let rows = ((h as f32 / size).round() as u32).max(1);
    let pal: Vec<[f32; 3]> = palette(text(p, "palette", "1bit")).iter().map(|c| hex_rgb(c)).collect();
    let spread = num(p, "spread", 1.0) as f32 * 64.0;
    let mut small = resample_area(src, cols, rows);
    map_pixels(&mut small, |i, v| {
        let (x, y) = (i % cols as usize, i / cols as usize);
        let o = (BAYER[(y & 3) * 4 + (x & 3)] / 16.0 - 0.5) * spread;
        let (r, g, b) = (v[0] + o, v[1] + o, v[2] + o);
        let mut best = 0;
        let mut bd = f32::INFINITY;
        for (k, c) in pal.iter().enumerate() {
            let (dr, dg, db) = (r - c[0], g - c[1], b - c[2]);
            let dist = dr * dr * 0.3 + dg * dg * 0.59 + db * db * 0.11;
            if dist < bd {
                bd = dist;
                best = k;
            }
        }
        v[0] = pal[best][0];
        v[1] = pal[best][1];
        v[2] = pal[best][2];
    });
    *dst = resize_nearest(&small, w, h);
}

// ---------- RGB split ----------
/// The photo-editor sliders in one pass: light, colour, vignette, grain, and sharpening
/// (unsharp mask). Every slider is −100…100 (vignette, grain, sharpen 0…100); 0 changes nothing.
fn adjust(src: &Pixmap, dst: &mut Pixmap, p: &Params, t: f32) {
    let g = |k: &str| (num(p, k, 0.0) / 100.0) as f32;
    let (br, ct, sat, warm, tint) = (g("brightness"), g("contrast"), g("saturation"), g("warmth"), g("tint"));
    let (hi, sh, vig, grain, sharp) = (g("highlights"), g("shadows"), g("vignette"), g("grain"), g("sharpen"));
    let (w, h) = (dst.width() as i32, dst.height() as i32);
    let soft = if sharp > 0.0 { Some(blur_of(src, 1.6)) } else { None };
    let s = src.data();
    let sb = soft.as_ref().map(|b| b.data());
    let d = dst.data_mut();
    let frame = (t * 24.0) as u32;
    let smooth = |a: f32, b: f32, x: f32| {
        let k = ((x - a) / (b - a)).clamp(0.0, 1.0);
        k * k * (3.0 - 2.0 * k)
    };
    for y in 0..h {
        for x in 0..w {
            let o = ((y * w + x) * 4) as usize;
            let a = s[o + 3] as f32 / 255.0;
            if a <= 0.0 {
                d[o..o + 4].copy_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let un = |i: usize, buf: &[u8]| buf[i] as f32 / 255.0 / a;
            let (mut r, mut gg, mut b) = (un(o, s), un(o + 1, s), un(o + 2, s));
            if let Some(sb) = sb {
                r += sharp * 1.5 * (r - un(o, sb));
                gg += sharp * 1.5 * (gg - un(o + 1, sb));
                b += sharp * 1.5 * (b - un(o + 2, sb));
            }
            r += br * 0.35;
            gg += br * 0.35;
            b += br * 0.35;
            let c = 1.0 + ct;
            r = (r - 0.5) * c + 0.5;
            gg = (gg - 0.5) * c + 0.5;
            b = (b - 0.5) * c + 0.5;
            let l = 0.2126 * r + 0.7152 * gg + 0.0722 * b;
            let lift = hi * 0.35 * smooth(0.45, 1.0, l) + sh * 0.35 * (1.0 - smooth(0.0, 0.55, l));
            r += lift;
            gg += lift;
            b += lift;
            let l = 0.2126 * r + 0.7152 * gg + 0.0722 * b;
            let k = 1.0 + sat;
            r = l + (r - l) * k;
            gg = l + (gg - l) * k;
            b = l + (b - l) * k;
            r += warm * 0.12;
            b -= warm * 0.12;
            gg -= tint * 0.1;
            r += tint * 0.04;
            b += tint * 0.04;
            if vig > 0.0 {
                let dx = (x as f32 / w as f32 - 0.5) * 2.0;
                let dy = (y as f32 / h as f32 - 0.5) * 2.0;
                let v = 1.0 - vig * 0.85 * smooth(0.45, 1.45, (dx * dx + dy * dy).sqrt());
                r *= v;
                gg *= v;
                b *= v;
            }
            if grain > 0.0 {
                let mut n = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ frame.wrapping_mul(2246822519);
                n = (n ^ (n >> 13)).wrapping_mul(1274126177);
                let v = ((n >> 8) & 0xffff) as f32 / 65535.0 - 0.5;
                r += v * grain * 0.22;
                gg += v * grain * 0.22;
                b += v * grain * 0.22;
            }
            let q = |v: f32| (v.clamp(0.0, 1.0) * a * 255.0).round() as u8;
            d[o] = q(r);
            d[o + 1] = q(gg);
            d[o + 2] = q(b);
            d[o + 3] = s[o + 3];
        }
    }
}

fn rgbsplit(src: &Pixmap, dst: &mut Pixmap, p: &Params, t: f32) {
    let (w, h) = (dst.width() as i32, dst.height() as i32);
    let k = unit_of(dst);
    let mut off = num(p, "offset", 10.0) as f32 * k;
    if flag(p, "jitter") {
        off *= 0.6 + 0.8 * ((t * 23.7).sin() * (t * 7.1).cos()).abs();
    }
    let off = off.round() as i32;
    let s = src.data();
    let d = dst.data_mut();
    for y in 0..h {
        for x in 0..w {
            let o = ((y * w + x) * 4) as usize;
            let at = |xx: i32| if xx >= 0 && xx < w { Some(((y * w + xx) * 4) as usize) } else { None };
            let (r, a0) = at(x + off).map(|i| (s[i] as u32, s[i + 3] as u32)).unwrap_or((0, 0));
            let (g, a1) = (s[o + 1] as u32, s[o + 3] as u32);
            let (b, a2) = at(x - off).map(|i| (s[i + 2] as u32, s[i + 3] as u32)).unwrap_or((0, 0));
            let a = (a0 + a1 + a2).min(255);
            d[o] = r.min(a) as u8;
            d[o + 1] = g.min(a) as u8;
            d[o + 2] = b.min(a) as u8;
            d[o + 3] = a as u8;
        }
    }
}

// ---------- Glitch ----------
fn glitch(src: &Pixmap, dst: &mut Pixmap, p: &Params, t: f32) {
    let mut g = Canvas::from_pixmap(std::mem::replace(dst, pixmap(1, 1)));
    let (w, h) = (g.w(), g.h());
    let intensity = num(p, "intensity", 0.5) as f32;
    let slices = num(p, "slices", 14.0).round() as i32;
    let mut r = rng((t * num(p, "speed", 12.0) as f32).floor() as i64 + 7);
    g.draw_image(src, 0.0, 0.0);
    for _ in 0..slices {
        if r.next() > 0.35 + intensity * 0.6 {
            continue;
        }
        let y = r.next() * h;
        let sh = (r.next() * h * 0.08 * (0.3 + intensity)).max(1.0);
        let dx = (r.next() - 0.5) * w * 0.18 * intensity;
        g.draw_image_sub(src, 0.0, y, w, sh, dx, y, w, sh);
        if r.next() < intensity * 0.5 {
            g.set_op("lighter");
            let col = if r.next() > 0.5 { rgba(255, 0, 80, 0.18) } else { rgba(0, 200, 255, 0.18) };
            g.fill_rect_color(0.0, y, w, sh, col);
            g.set_op("source-over");
        }
    }
    *dst = g.into_pixmap();
}

// ---------- Per-pixel maps ----------
fn posterize(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    *dst = src.clone();
    let n = (num(p, "levels", 4.0).round().max(2.0) - 1.0) as f32;
    let step = 255.0 / n;
    map_pixels(dst, |_, v| {
        for c in v.iter_mut().take(3) {
            *c = (*c / step).round() * step;
        }
    });
}
fn duotone(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    *dst = src.clone();
    let d0 = hex_rgb(text(p, "dark", "#1b0f2e"));
    let d1 = hex_rgb(text(p, "light", "#ff7a3d"));
    let c = num(p, "contrast", 1.1) as f32;
    map_pixels(dst, |_, v| {
        let l = clamp01((lum(v[0], v[1], v[2]) - 0.5) * c + 0.5);
        for i in 0..3 {
            v[i] = d0[i] + (d1[i] - d0[i]) * l;
        }
    });
}
/// One-colour scale: luminance runs black → the colour (at `PIVOT`) → toward white by `highlights`.
/// `keep` lets the original colours show back through.
fn color_scale(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    const PIVOT: f32 = 0.62;
    *dst = src.clone();
    let col = hex_rgb(text(p, "color", "#ff2a1a"));
    let c = num(p, "contrast", 1.15) as f32;
    let hi = num(p, "highlights", 0.35) as f32;
    let keep = num(p, "keep", 0.0) as f32;
    map_pixels(dst, |_, v| {
        let l = clamp01((lum(v[0], v[1], v[2]) - 0.5) * c + 0.5);
        let mut out = [0.0; 3];
        for i in 0..3 {
            out[i] = if l <= PIVOT { col[i] * (l / PIVOT) } else { col[i] + (255.0 - col[i]) * hi * ((l - PIVOT) / (1.0 - PIVOT)) };
        }
        for i in 0..3 {
            v[i] = out[i] + (v[i] - out[i]) * keep;
        }
    });
}
fn threshold(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    *dst = src.clone();
    let lv = num(p, "level", 0.5) as f32;
    let d0 = hex_rgb(text(p, "dark", "#000000"));
    let d1 = hex_rgb(text(p, "light", "#ffffff"));
    map_pixels(dst, |_, v| {
        let on = lum(v[0], v[1], v[2]) >= lv;
        let c = if on { d1 } else { d0 };
        v[..3].copy_from_slice(&c);
    });
}
fn solarize(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    *dst = src.clone();
    let th = num(p, "threshold", 0.5) as f32 * 255.0;
    map_pixels(dst, |_, v| {
        for c in v.iter_mut().take(3) {
            if *c > th {
                *c = 255.0 - *c;
            }
        }
    });
}
fn lut_from(stops: &[&str]) -> Vec<[f32; 3]> {
    let s: Vec<[f32; 3]> = stops.iter().map(|c| hex_rgb(c)).collect();
    (0..256)
        .map(|i| {
            let x = i as f32 / 255.0 * (s.len() - 1) as f32;
            let j = (x.floor() as usize).min(s.len() - 2);
            let f = x - j as f32;
            [0, 1, 2].map(|c| s[j][c] + (s[j + 1][c] - s[j][c]) * f)
        })
        .collect()
}
fn gradient_map_fx(src: &Pixmap, dst: &mut Pixmap, p: &Params, t: f32) {
    *dst = src.clone();
    let map = gradient_map(text(p, "map", "thermal"));
    let c = num(p, "contrast", 1.2) as f32;
    let lut = lut_from(map.stops);
    let mut r = rng((t * 24.0).floor() as i64 + 5);
    let noise = map.noise;
    map_pixels(dst, |_, v| {
        let mut l = clamp01((lum(v[0], v[1], v[2]) - 0.5) * c + 0.5);
        if noise {
            l = clamp01(l + (r.next() - 0.5) * 0.12);
        }
        let k = (l * 255.0).round() as usize;
        v[..3].copy_from_slice(&lut[k]);
    });
    if noise {
        let mut g = Canvas::from_pixmap(std::mem::replace(dst, pixmap(1, 1)));
        let (w, h) = (g.w(), g.h());
        let v = g.radial(w / 2.0, h / 2.0, h * 0.3, w / 2.0, h / 2.0, w * 0.6, &[(0.0, rgba(0, 0, 0, 0.0)), (1.0, rgba(0, 0, 0, 0.85))]);
        g.set_fill(v);
        g.fill_rect(0.0, 0.0, w, h);
        *dst = g.into_pixmap();
    }
}

// ---------- Dots: rounded halftone and braille dot grid ----------
// Dots are stamped straight into pixel data: at braille density a frame has 100k+ of them.
struct Mask {
    dx: Vec<i16>,
    dy: Vec<i16>,
    a: Vec<f32>,
}
fn dot_mask(radius: f32) -> Arc<Mask> {
    static C: OnceLock<Mutex<HashMap<i32, Arc<Mask>>>> = OnceLock::new();
    let cache = C.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (radius * 4.0).round() as i32;
    if let Some(m) = cache.lock().get(&key) {
        return m.clone();
    }
    let r = key as f32 / 4.0;
    let big = (r + 1.0).ceil() as i32;
    let mut m = Mask { dx: vec![], dy: vec![], a: vec![] };
    for y in -big..=big {
        for x in -big..=big {
            let cov = (r + 0.5 - (x as f32).hypot(y as f32)).clamp(0.0, 1.0); // anti-aliased edge
            if cov > 0.0 {
                m.dx.push(x as i16);
                m.dy.push(y as i16);
                m.a.push(cov);
            }
        }
    }
    let m = Arc::new(m);
    let mut c = cache.lock();
    if c.len() > 400 {
        c.clear();
    }
    c.insert(key, m.clone());
    m
}
fn dots_engine(src: &Pixmap, dst: &mut Pixmap, p: &Params, object: bool) {
    let (w, h) = (dst.width() as usize, dst.height() as usize);
    let k = unit_of(dst);
    let braille = text(p, "pattern", "halftone") == "braille";
    let cell = (num(p, "cell", 14.0) as f32 * k).max(2.0);
    let pitch = if braille { (cell / 2.0).max(2.0) } else { cell }; // a braille character is two dots wide
    let cols = ((w as f32 / pitch).floor() as usize).max(1);
    let rows = ((h as f32 / pitch).floor() as usize).max(1);
    let ox = (w as f32 - cols as f32 * pitch) / 2.0;
    let oy = (h as f32 - rows as f32 * pitch) / 2.0;
    let contrast = num(p, "contrast", 1.15) as f32;
    let invert = flag(p, "invert");
    let scale = num(p, "size", 0.9) as f32;
    let color_mode = text(p, "color", "white");
    let bg: Option<[f32; 3]> = if object {
        None
    } else {
        match text(p, "bg", "black") {
            "black" => Some([0.0, 0.0, 0.0]),
            "midnight" => Some([13.0, 15.0, 20.0]),
            _ => None,
        }
    };
    let lut = if color_mode != "white" && color_mode != "source" { Some(lut_from(dot_palette(color_mode))) } else { None };

    let small = resample_area(src, cols as u32, rows as u32);
    let px = to_straight(&small);
    let mut d = vec![0f32; w * h * 4];
    if let Some(bg) = bg {
        for o in d.chunks_exact_mut(4) {
            o[0] = bg[0];
            o[1] = bg[1];
            o[2] = bg[2];
            o[3] = 255.0;
        }
    }
    let fixed = if braille { Some(dot_mask(pitch / 2.0 * scale * 0.82)) } else { None };
    for y in 0..rows {
        for x in 0..cols {
            let i = (y * cols + x) * 4;
            let alpha = px[i + 3] as f32 / 255.0;
            if alpha == 0.0 {
                continue;
            }
            let mut l = lum(px[i] as f32, px[i + 1] as f32, px[i + 2] as f32) * alpha;
            l = clamp01((l - 0.5) * contrast + 0.5);
            if invert {
                l = 1.0 - l;
            }
            let mask = if braille {
                if l <= (BAYER[(y & 3) * 4 + (x & 3)] + 0.5) / 16.0 {
                    continue; // dot off
                }
                fixed.clone().unwrap()
            } else {
                let r = pitch / 2.0 * scale * l.sqrt();
                if r < 0.35 {
                    continue;
                }
                dot_mask(r)
            };
            let (cr, cg, cb) = if let Some(lut) = &lut {
                let c = lut[(l * 255.0).round() as usize];
                (c[0], c[1], c[2])
            } else if color_mode == "source" {
                (px[i] as f32 * 1.15, px[i + 1] as f32 * 1.15, px[i + 2] as f32 * 1.15)
            } else {
                (255.0, 255.0, 255.0)
            };
            let (cr, cg, cb) = (cr.min(255.0), cg.min(255.0), cb.min(255.0));
            let cx = (ox + (x as f32 + 0.5) * pitch).round() as i32;
            let cy = (oy + (y as f32 + 0.5) * pitch).round() as i32;
            for m in 0..mask.a.len() {
                let xx = cx + mask.dx[m] as i32;
                let yy = cy + mask.dy[m] as i32;
                if xx < 0 || yy < 0 || xx as usize >= w || yy as usize >= h {
                    continue;
                }
                let o = (yy as usize * w + xx as usize) * 4;
                let a = mask.a[m];
                if bg.is_some() {
                    d[o] += (cr - d[o]) * a;
                    d[o + 1] += (cg - d[o + 1]) * a;
                    d[o + 2] += (cb - d[o + 2]) * a;
                } else {
                    d[o] = cr;
                    d[o + 1] = cg;
                    d[o + 2] = cb;
                    d[o + 3] = d[o + 3].max(a * 255.0);
                }
            }
        }
    }
    let bytes: Vec<u8> = d.iter().map(|v| v.round().clamp(0.0, 255.0) as u8).collect();
    let mut out = from_straight(&bytes, w as u32, h as u32);
    let glow = num(p, "glow", 0.0) as f32;
    if glow > 0.0 {
        // Soft bloom around bright dots, like a lit screen.
        let halo = blur_of(&out, 6.0 + glow * 10.0);
        let mut g = Canvas::from_pixmap(out);
        g.composite(&halo, (glow * 1.4).min(1.0), BlendMode::Screen);
        out = g.into_pixmap();
    }
    *dst = out;
}

// ---------- VHS ----------
fn vhs(src: &Pixmap, dst: &mut Pixmap, p: &Params, t: f32) {
    let mut g = Canvas::from_pixmap(std::mem::replace(dst, pixmap(1, 1)));
    let (w, h) = (g.w(), g.h());
    let k = unit_of(src);
    let amt = num(p, "intensity", 0.6) as f32;
    let soft = blur_of(src, 2.0 + amt * 3.0);
    g.draw_image(&soft, 0.0, 0.0);
    // Colour bleed: red and blue fringes either side.
    g.set_op("lighter");
    g.set_alpha(0.18 + amt * 0.25);
    for (col, dx) in [([255.0, 0.0, 64.0], -4.0), ([0.0, 64.0, 255.0], 4.0)] {
        let tint = tinted(&soft, col);
        g.draw_image(&tint, dx * k * (1.0 + amt), 0.0);
    }
    g.set_alpha(1.0);
    g.set_op("source-over");
    // A tracking band that rolls down the frame, shifting the picture sideways.
    let band = ((t * 0.35) % 1.3) * h - h * 0.15;
    let bh = h * 0.06;
    g.draw_image_sub(src, 0.0, band, w, bh, (wobble(t, 1.0) * 18.0 + 10.0) * k * amt, band, w, bh);
    g.fill_rect_color(0.0, band, w, bh * 0.3, rgba(255, 255, 255, 0.06 * amt));
    // Scanlines and noise.
    let mut pb = PathBuilder::new();
    let step = (3.0 * k).max(2.0);
    let mut y = 0.0;
    while y < h {
        if let Some(r) = Rect::from_xywh(0.0, y, w, k.max(1.0)) {
            pb.push_rect(r);
        }
        y += step;
    }
    if let Some(path) = pb.finish() {
        g.fill_path_with(&path, &crate::render::canvas::Fill::Solid(rgba(0, 0, 0, 0.18 + amt * 0.12)));
    }
    let mut r = rng((t * 24.0).floor() as i64 + 9);
    g.set_fill_color(rgba(255, 255, 255, 0.5));
    let mut i = 0.0;
    while i < 160.0 * amt {
        let (x, y, ww) = (r.next() * w, r.next() * h, (1.0 + r.next() * 3.0) * k);
        g.fill_rect(x, y, ww, k);
        i += 1.0;
    }
    *dst = g.into_pixmap();
}

// ---------- CRT ----------
fn crt(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    let mut g = Canvas::from_pixmap(std::mem::replace(dst, pixmap(1, 1)));
    let (w, h) = (g.w(), g.h());
    let k = unit_of(src);
    g.draw_image(src, 0.0, 0.0);
    // Glow from a blurred copy, then RGB phosphor stripes and scanlines.
    let glow = blur_of(src, 14.0);
    g.composite(&glow, num(p, "glow", 0.35) as f32, BlendMode::Plus);
    g.set_op("multiply");
    let step = (num(p, "pitch", 4.0) as f32 * k).max(3.0);
    for (ci, col) in ["#ff8080", "#80ff80", "#8080ff"].iter().enumerate() {
        let mut pb = PathBuilder::new();
        let mut x = 0.0;
        while x < w {
            if let Some(r) = Rect::from_xywh(x + ci as f32 * step / 3.0, 0.0, step / 3.0, h) {
                pb.push_rect(r);
            }
            x += step;
        }
        if let Some(path) = pb.finish() {
            g.fill_path_with(&path, &crate::render::canvas::Fill::Solid(hex(col)));
        }
    }
    let mut pb = PathBuilder::new();
    let mut y = 0.0;
    while y < h {
        if let Some(r) = Rect::from_xywh(0.0, y, w, (step * 0.35).max(1.0)) {
            pb.push_rect(r);
        }
        y += step;
    }
    if let Some(path) = pb.finish() {
        g.fill_path_with(&path, &crate::render::canvas::Fill::Solid(rgba(0, 0, 0, 0.45)));
    }
    g.set_op("source-over");
    let v = g.radial(w / 2.0, h / 2.0, h * 0.45, w / 2.0, h / 2.0, w * 0.72, &[(0.0, rgba(0, 0, 0, 0.0)), (1.0, rgba(0, 0, 0, 0.75))]);
    g.set_fill(v);
    g.fill_rect(0.0, 0.0, w, h);
    *dst = g.into_pixmap();
}

// ---------- Camera shake ----------
fn shake(src: &Pixmap, dst: &mut Pixmap, p: &Params, t: f32) {
    let mut g = Canvas::from_pixmap(std::mem::replace(dst, pixmap(1, 1)));
    let (w, h) = (g.w(), g.h());
    let amt = num(p, "amount", 0.4) as f32;
    let tt = t * num(p, "speed", 1.0) as f32;
    let z = 1.0 + amt * 0.08;
    g.translate(w / 2.0 + wobble(tt, 1.0) * w * 0.025 * amt, h / 2.0 + wobble(tt, 2.0) * h * 0.03 * amt);
    g.rotate(wobble(tt, 3.0) * 0.02 * amt);
    g.scale(z, z);
    g.draw_image(src, -w / 2.0, -h / 2.0);
    *dst = g.into_pixmap();
}

// ---------- Mirror & kaleidoscope ----------
fn mirror(src: &Pixmap, dst: &mut Pixmap, p: &Params, t: f32) {
    let mut g = Canvas::from_pixmap(std::mem::replace(dst, pixmap(1, 1)));
    let (w, h) = (g.w(), g.h());
    match text(p, "mode", "left") {
        m @ ("left" | "right") => {
            let sx = if m == "left" { 0.0 } else { w / 2.0 };
            g.draw_image_sub(src, sx, 0.0, w / 2.0, h, 0.0, 0.0, w / 2.0, h);
            g.save();
            g.translate(w, 0.0);
            g.scale(-1.0, 1.0);
            g.draw_image_sub(src, sx, 0.0, w / 2.0, h, 0.0, 0.0, w / 2.0, h);
            g.restore();
        }
        "top" => {
            g.draw_image_sub(src, 0.0, 0.0, w, h / 2.0, 0.0, 0.0, w, h / 2.0);
            g.save();
            g.translate(0.0, h);
            g.scale(1.0, -1.0);
            g.draw_image_sub(src, 0.0, 0.0, w, h / 2.0, 0.0, 0.0, w, h / 2.0);
            g.restore();
        }
        "quad" => {
            for (fx, fy) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                g.save();
                g.translate(if fx < 0.0 { w } else { 0.0 }, if fy < 0.0 { h } else { 0.0 });
                g.scale(fx, fy);
                g.draw_image_sub(src, 0.0, 0.0, w / 2.0, h / 2.0, 0.0, 0.0, w / 2.0, h / 2.0);
                g.restore();
            }
        }
        _ => {
            // Kaleidoscope: one wedge of the picture, rotated and flipped around the centre.
            let n = num(p, "segments", 6.0).round().max(3.0) as i32;
            let ang = PI * 2.0 / n as f32;
            let r = w.hypot(h) / 2.0;
            let spin = t * num(p, "spin", 0.15) as f32;
            for i in 0..n {
                g.save();
                g.translate(w / 2.0, h / 2.0);
                g.rotate(i as f32 * ang + spin);
                if i % 2 == 1 {
                    g.scale(1.0, -1.0);
                }
                g.begin_path();
                g.move_to(0.0, 0.0);
                g.arc(0.0, 0.0, r, -ang / 2.0 - 0.01, ang / 2.0 + 0.01);
                g.close_path();
                g.clip();
                g.draw_image(src, -w / 2.0 + w * 0.12, -h / 2.0);
                g.restore();
            }
        }
    }
    *dst = g.into_pixmap();
}

// ---------- Edge sketch / neon edges / emboss (3×3 kernels at half resolution) ----------
fn kernel(src: &Pixmap, dst: &mut Pixmap, p: &Params, emboss: bool) {
    let (w, h) = (dst.width(), dst.height());
    let sw = ((w as f32 / 2.0).round() as usize).max(2);
    let sh = ((h as f32 / 2.0).round() as usize).max(2);
    let small = resample_area(src, sw as u32, sh as u32);
    let d = to_straight(&small);
    let mut o = vec![0u8; sw * sh * 4];
    let strength = num(p, "strength", 1.5) as f32;
    let mode = text(p, "mode", "sketch");
    let l = |x: usize, y: usize| {
        let i = (y * sw + x) * 4;
        lum(d[i] as f32, d[i + 1] as f32, d[i + 2] as f32)
    };
    for y in 1..sh - 1 {
        for x in 1..sw - 1 {
            let i = (y * sw + x) * 4;
            let v = if emboss {
                0.5 + (l(x + 1, y + 1) - l(x - 1, y - 1) + (l(x + 1, y) - l(x - 1, y)) * 0.5) * strength
            } else {
                let gx = -l(x - 1, y - 1) - 2.0 * l(x - 1, y) - l(x - 1, y + 1) + l(x + 1, y - 1) + 2.0 * l(x + 1, y) + l(x + 1, y + 1);
                let gy = -l(x - 1, y - 1) - 2.0 * l(x, y - 1) - l(x + 1, y - 1) + l(x - 1, y + 1) + 2.0 * l(x, y + 1) + l(x + 1, y + 1);
                (gx.hypot(gy) * strength).min(1.0)
            };
            if !emboss && mode == "neon" {
                for c in 0..3 {
                    o[i + c] = (d[i + c] as f32 * v * 1.6).round().clamp(0.0, 255.0) as u8;
                }
            } else {
                let c = if !emboss && mode == "sketch" { 255.0 * (1.0 - v) } else { 255.0 * v };
                let c = c.round().clamp(0.0, 255.0) as u8;
                o[i] = c;
                o[i + 1] = c;
                o[i + 2] = c;
            }
            o[i + 3] = 255;
        }
    }
    let up = resize_bilinear(&from_straight(&o, sw as u32, sh as u32), w, h);
    let mut g = Canvas::from_pixmap(std::mem::replace(dst, pixmap(1, 1)));
    if !emboss && mode == "sketch" {
        g.fill_rect_color(0.0, 0.0, w as f32, h as f32, hex("#f4f1ea"));
        g.composite(&up, 1.0, BlendMode::Multiply);
    } else {
        g.composite(&up, 1.0, BlendMode::SourceOver);
    }
    *dst = g.into_pixmap();
}

// ---------- Zoom blur, wave, tilt-shift, glow ----------
fn zoomblur(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    let mut g = Canvas::from_pixmap(std::mem::replace(dst, pixmap(1, 1)));
    let (w, h) = (g.w(), g.h());
    let amt = num(p, "amount", 0.5) as f32;
    g.draw_image(src, 0.0, 0.0);
    for i in 1..=10 {
        let k = 1.0 + (i as f32 / 10.0) * amt * 0.35;
        g.set_alpha(0.5 / i as f32 + 0.05);
        g.draw_image_scaled(src, (w - w * k) / 2.0, (h - h * k) / 2.0, w * k, h * k);
    }
    *dst = g.into_pixmap();
}
fn wave(src: &Pixmap, dst: &mut Pixmap, p: &Params, t: f32) {
    let (w, h) = (dst.width() as usize, dst.height() as usize);
    let k = unit_of(dst);
    let amp = num(p, "amplitude", 18.0) as f32 * k;
    let freq = num(p, "frequency", 3.0) as f32;
    let speed = num(p, "speed", 2.0) as f32;
    let vertical = text(p, "direction", "horizontal") == "vertical";
    let step = (2.0 * k).round().max(1.0) as usize;
    let s = src.data();
    let d = dst.data_mut();
    let sample = |x: f32, y: usize, c: usize| -> f32 {
        // Linear interpolation along x; outside the frame is transparent.
        let x0 = x.floor();
        let f = x - x0;
        let get = |xi: i64| if xi >= 0 && (xi as usize) < w { s[(y * w + xi as usize) * 4 + c] as f32 } else { 0.0 };
        get(x0 as i64) * (1.0 - f) + get(x0 as i64 + 1) * f
    };
    if !vertical {
        for y in 0..h {
            let band = y / step * step;
            let off = ((band as f32 / h as f32) * freq * PI * 2.0 + t * speed).sin() * amp;
            for x in 0..w {
                for c in 0..4 {
                    d[(y * w + x) * 4 + c] = sample(x as f32 - off, y, c).round() as u8;
                }
            }
        }
    } else {
        for x in 0..w {
            let band = x / step * step;
            let off = ((band as f32 / w as f32) * freq * PI * 2.0 + t * speed).sin() * amp;
            for y in 0..h {
                let sy = y as f32 - off;
                let y0 = sy.floor();
                let f = sy - y0;
                for c in 0..4 {
                    let get = |yi: i64| if yi >= 0 && (yi as usize) < h { s[(yi as usize * w + x) * 4 + c] as f32 } else { 0.0 };
                    d[(y * w + x) * 4 + c] = (get(y0 as i64) * (1.0 - f) + get(y0 as i64 + 1) * f).round() as u8;
                }
            }
        }
    }
}
fn tiltshift(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    let mut g = Canvas::from_pixmap(std::mem::replace(dst, pixmap(1, 1)));
    let (w, h) = (g.w(), g.h());
    let focus = num(p, "focus", 0.55) as f32;
    let band = num(p, "band", 0.18) as f32;
    g.draw_image(src, 0.0, 0.0);
    let mut m = Canvas::from_pixmap(blur_of(src, num(p, "blur", 10.0) as f32));
    let (a, b) = ((focus - band).max(0.0), (focus + band).min(1.0));
    let black = hex("#000");
    let clear = rgba(0, 0, 0, 0.0);
    let mut stops = vec![(0.0, black), ((a - 0.12).max(0.0), black), (a, clear), (b, clear), ((b + 0.12).min(1.0), black), (1.0, black)];
    // Gradient stops must not go backwards.
    for i in 1..stops.len() {
        if stops[i].0 < stops[i - 1].0 {
            stops[i].0 = stops[i - 1].0;
        }
    }
    let grad = m.linear(0.0, 0.0, 0.0, h, &stops);
    m.set_op("destination-in");
    m.set_fill(grad);
    m.fill_rect(0.0, 0.0, w, h);
    g.draw_image(&m.into_pixmap(), 0.0, 0.0);
    if flag(p, "saturate") {
        g.set_blend(BlendMode::Saturation);
        g.set_alpha(0.25);
        g.fill_rect_color(0.0, 0.0, w, h, crate::render::canvas::rgb(230, 25, 25));
        g.set_alpha(1.0);
        g.set_op("source-over");
    }
    *dst = g.into_pixmap();
}
fn glow(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    let mut g = Canvas::from_pixmap(std::mem::replace(dst, pixmap(1, 1)));
    g.draw_image(src, 0.0, 0.0);
    let halo = blur_of(src, num(p, "radius", 18.0) as f32);
    g.composite(&halo, num(p, "strength", 0.7) as f32, BlendMode::Screen);
    *dst = g.into_pixmap();
}

// ---------- CSS colour filters: one 3×4 colour matrix, blur as a gaussian ----------
type Matrix = [f32; 12];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
/// a ∘ b: apply b first, then a.
fn mul(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [0.0; 12];
    for r in 0..3 {
        for c in 0..3 {
            out[r * 4 + c] = a[r * 4] * b[c] + a[r * 4 + 1] * b[4 + c] + a[r * 4 + 2] * b[8 + c];
        }
        out[r * 4 + 3] = a[r * 4] * b[3] + a[r * 4 + 1] * b[7] + a[r * 4 + 2] * b[11] + a[r * 4 + 3];
    }
    out
}
fn m3(v: [f32; 9], off: f32) -> Matrix {
    [v[0], v[1], v[2], off, v[3], v[4], v[5], off, v[6], v[7], v[8], off]
}
// Matrices from the Filter Effects spec.
fn op_matrix(name: &str, v: f32) -> Option<Matrix> {
    Some(match name {
        "grayscale" => {
            let a = 1.0 - v.min(1.0);
            m3([0.2126 + 0.7874 * a, 0.7152 - 0.7152 * a, 0.0722 - 0.0722 * a, 0.2126 - 0.2126 * a, 0.7152 + 0.2848 * a, 0.0722 - 0.0722 * a, 0.2126 - 0.2126 * a, 0.7152 - 0.7152 * a, 0.0722 + 0.9278 * a], 0.0)
        }
        "sepia" => {
            let a = 1.0 - v.min(1.0);
            m3([0.393 + 0.607 * a, 0.769 - 0.769 * a, 0.189 - 0.189 * a, 0.349 - 0.349 * a, 0.686 + 0.314 * a, 0.168 - 0.168 * a, 0.272 - 0.272 * a, 0.534 - 0.534 * a, 0.131 + 0.869 * a], 0.0)
        }
        "saturate" => m3([0.213 + 0.787 * v, 0.715 - 0.715 * v, 0.072 - 0.072 * v, 0.213 - 0.213 * v, 0.715 + 0.285 * v, 0.072 - 0.072 * v, 0.213 - 0.213 * v, 0.715 - 0.715 * v, 0.072 + 0.928 * v], 0.0),
        "hue-rotate" => {
            let t = v * PI / 180.0;
            let (c, s) = (t.cos(), t.sin());
            m3([
                0.213 + c * 0.787 - s * 0.213, 0.715 - c * 0.715 - s * 0.715, 0.072 - c * 0.072 + s * 0.928,
                0.213 - c * 0.213 + s * 0.143, 0.715 + c * 0.285 + s * 0.14, 0.072 - c * 0.072 - s * 0.283,
                0.213 - c * 0.213 - s * 0.787, 0.715 - c * 0.715 + s * 0.715, 0.072 + c * 0.928 + s * 0.072,
            ], 0.0)
        }
        "brightness" => m3([v, 0.0, 0.0, 0.0, v, 0.0, 0.0, 0.0, v], 0.0),
        "contrast" => m3([v, 0.0, 0.0, 0.0, v, 0.0, 0.0, 0.0, v], 255.0 * (0.5 - 0.5 * v)),
        "invert" => {
            let a = v.min(1.0);
            let d = 1.0 - 2.0 * a;
            m3([d, 0.0, 0.0, 0.0, d, 0.0, 0.0, 0.0, d], 255.0 * a)
        }
        _ => return None,
    })
}
/// Parse "sepia(0.35) saturate(1.4) hue-rotate(-12deg) blur(2px)".
fn parse_filter(f: &str) -> (Option<Matrix>, f32) {
    let mut m = IDENTITY;
    let mut any = false;
    let mut blur = 0.0;
    for part in f.split(')') {
        let Some((name, arg)) = part.trim().split_once('(') else { continue };
        let v: f32 = arg.trim().trim_end_matches("deg").trim_end_matches("px").parse().unwrap_or(0.0);
        if name.trim() == "blur" {
            blur += v;
            continue;
        }
        if let Some(om) = op_matrix(name.trim(), v) {
            m = mul(&om, &m);
            any = true;
        }
    }
    (any.then_some(m), blur)
}

/// Separable box blur, three passes, approximating a gaussian of std-dev `sigma`. Outside the
/// frame is transparent, as with a CSS blur on a canvas.
pub fn gaussian(pm: &mut Pixmap, sigma: f32) {
    if sigma < 0.3 {
        return;
    }
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let n = 3.0;
    let wide = (12.0 * sigma * sigma / n + 1.0).sqrt();
    let r = ((wide - 1.0) / 2.0).round().max(1.0) as usize;
    let mut buf: Vec<f32> = pm.data().iter().map(|&v| v as f32).collect();
    let mut tmp = vec![0f32; buf.len()];
    let pass = |src: &[f32], dst: &mut [f32], len: usize, count: usize, stride: usize, step: usize| {
        let norm = 1.0 / (2 * r + 1) as f32;
        for line in 0..count {
            let base = line * stride;
            for c in 0..4 {
                let mut acc = 0.0;
                for i in 0..=r.min(len - 1) {
                    acc += src[base + i * step + c];
                }
                for i in 0..len {
                    dst[base + i * step + c] = acc * norm;
                    if i + r + 1 < len {
                        acc += src[base + (i + r + 1) * step + c];
                    }
                    if i >= r {
                        acc -= src[base + (i - r) * step + c];
                    }
                }
            }
        }
    };
    for _ in 0..3 {
        pass(&buf, &mut tmp, w, h, w * 4, 4);
        pass(&tmp, &mut buf, h, w, 4, w * 4);
    }
    for (o, v) in pm.data_mut().iter_mut().zip(buf) {
        *o = v.round().clamp(0.0, 255.0) as u8;
    }
    // Keep premultiplied invariants.
    for px in pm.data_mut().chunks_exact_mut(4) {
        let a = px[3];
        px[0] = px[0].min(a);
        px[1] = px[1].min(a);
        px[2] = px[2].min(a);
    }
}

/// Apply a CSS filter string to a frame in place.
pub fn apply_css_filter(pm: &mut Pixmap, filter: &str) {
    let (matrix, blur) = parse_filter(filter);
    if blur > 0.3 {
        let sigma = blur * unit_of(pm);
        gaussian(pm, sigma);
    }
    if let Some(m) = matrix {
        map_pixels(pm, |_, v| {
            let (r, g, b) = (v[0], v[1], v[2]);
            v[0] = m[0] * r + m[1] * g + m[2] * b + m[3];
            v[1] = m[4] * r + m[5] * g + m[6] * b + m[7];
            v[2] = m[8] * r + m[9] * g + m[10] * b + m[11];
        });
    }
}

// ---------- Effect stack ----------
/// Engines that paint pixels where the input was transparent (backgrounds, noise, vignettes,
/// kernels). On an object they get cut back to its shape plus a soft margin.
fn fills(e: Engine) -> bool {
    matches!(e, Engine::Ascii | Engine::Dots | Engine::Vhs | Engine::Crt | Engine::Glitch | Engine::Edges | Engine::Emboss | Engine::GradientMap)
}
fn mask_to_object(out: &mut Pixmap, shape: &Pixmap) {
    let halo = blur_of(shape, 10.0);
    let (hd, sd) = (halo.data(), shape.data());
    for (i, px) in out.data_mut().chunks_exact_mut(4).enumerate() {
        let mut a = hd[i * 4 + 3] as f32 / 255.0;
        a = a + a * (1.0 - a); // firm up the halo
        a = a + a * (1.0 - a);
        let s = sd[i * 4 + 3] as f32 / 255.0;
        a = s + a * (1.0 - s); // and keep the object itself fully covered
        if a < 1.0 {
            for c in px.iter_mut() {
                *c = (*c as f32 * a).round() as u8;
            }
        }
    }
}

/// Run a clip's effect stack over `src`. With `object`, effects stay on the clip's own shape
/// (a title, a placed photo) instead of filling the whole frame.
pub fn run_effects(src: &Pixmap, stack: &[ClipEffect], t: f32, dur: f32, object: bool) -> Pixmap {
    let (w, h) = (src.width(), src.height());
    let mut cur = src.clone();
    for fx in stack {
        if !fx.enabled {
            continue;
        }
        let Some(def) = effect_def(&fx.id) else { continue };
        let mut out = pixmap(w, h);
        if def.engine == Engine::Css {
            out = cur.clone();
            apply_css_filter(&mut out, &(def.css.unwrap())(fx.amount));
            cur = out;
            continue;
        }
        let p = &fx.params;
        match def.engine {
            Engine::Ascii => ascii(&cur, &mut out, p, t, object),
            Engine::Pixelate => pixelate(&cur, &mut out, p),
            Engine::Dither => dither(&cur, &mut out, p),
            Engine::RgbSplit => rgbsplit(&cur, &mut out, p, t),
            Engine::Adjust => adjust(&cur, &mut out, p, t),
            Engine::Extra => super::extra::run(def.id, &cur, &mut out, p, t, dur),
            Engine::Cutout => super::cutout::run(def.id, &cur, &mut out, p),
            Engine::Glitch => glitch(&cur, &mut out, p, t),
            Engine::Posterize => posterize(&cur, &mut out, p),
            Engine::Duotone => duotone(&cur, &mut out, p),
            Engine::Dots => dots_engine(&cur, &mut out, p, object),
            Engine::Vhs => vhs(&cur, &mut out, p, t),
            Engine::Crt => crt(&cur, &mut out, p),
            Engine::Shake => shake(&cur, &mut out, p, t),
            Engine::Mirror => mirror(&cur, &mut out, p, t),
            Engine::Edges => kernel(&cur, &mut out, p, false),
            Engine::Emboss => kernel(&cur, &mut out, p, true),
            Engine::GradientMap => gradient_map_fx(&cur, &mut out, p, t),
            Engine::ZoomBlur => zoomblur(&cur, &mut out, p),
            Engine::Wave => wave(&cur, &mut out, p, t),
            Engine::TiltShift => tiltshift(&cur, &mut out, p),
            Engine::Glow => glow(&cur, &mut out, p),
            Engine::Threshold => threshold(&cur, &mut out, p),
            Engine::Solarize => solarize(&cur, &mut out, p),
            Engine::ColorScale => color_scale(&cur, &mut out, p),
            Engine::Css => {}
            Engine::Shader => out = super::remocn::apply(&cur, &fx.id, p, t, dur),
            Engine::Look => {
                if let Some(l) = super::looks::look_def(&fx.id) {
                    out = super::looks::apply_look(&cur, l, t);
                }
            }
        }
        if fx.amount < 0.999 {
            // Mix: previous result underneath, effect on top at `amount`.
            let mut mix = Canvas::from_pixmap(cur.clone());
            mix.composite(&out, fx.amount as f32, BlendMode::SourceOver);
            out = mix.into_pixmap();
        }
        if object && fills(def.engine) {
            mask_to_object(&mut out, &cur);
        }
        cur = out;
    }
    cur
}
