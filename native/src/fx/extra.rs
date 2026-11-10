// Phone-editor effects: beat flashes and camera moves, light (leaks, flares, bokeh, film burn),
// film texture, particles (rain, snow, hearts, confetti, embers, sparkles), speed lines, and
// layouts (split screens, border, blurred fill), colour pop and fisheye. All are functions of the
// clip's local time, so every frame renders the same way every time.

use super::engines::blur_of;
use super::registry::{num, text};
use crate::model::types::Params;
use crate::render::canvas::{hex, pixmap, rgba, Canvas};
use std::f32::consts::{PI, TAU};
use tiny_skia::{BlendMode, Color, Pixmap};

/// Scale of a frame relative to the 1280 design size (long side).
fn unit(pm: &Pixmap) -> f32 {
    pm.width().max(pm.height()) as f32 / 1280.0
}

/// A repeatable 0–1 number for (a, b).
fn rnd(a: u32, b: u32) -> f32 {
    let mut x = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77) ^ 0x27d4_eb2f;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297a_2d39);
    x ^= x >> 15;
    x as f32 / u32::MAX as f32
}

fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn with_alpha(mut c: Color, a: f32) -> Color {
    c.apply_opacity(a.clamp(0.0, 1.0));
    c
}

/// Run effect `id` (one of this module's) over `src` into `dst`. `t` is seconds into the clip,
/// `dur` the clip's length.
pub fn run(id: &str, src: &Pixmap, dst: &mut Pixmap, p: &Params, t: f32, dur: f32) {
    let out = match id {
        "flash" => pulse(src, p, t, Pulse::Flash),
        "strobe" => pulse(src, p, t, Pulse::Strobe),
        "invert-flash" => pulse(src, p, t, Pulse::Invert),
        "zoom-pulse" => moved(src, p, t, dur, Move::ZoomPulse),
        "push-in" => moved(src, p, t, dur, Move::PushIn),
        "handheld" => moved(src, p, t, dur, Move::Handheld),
        "speed-lines" => speed_lines(src, p, t),
        "light-leak" => light_leak(src, p, t),
        "lens-flare" => lens_flare(src, p, t),
        "bokeh" => particles(src, p, t, Particle::Bokeh),
        "film-burn" => film_burn(src, p, t),
        "film-8mm" => film_8mm(src, p, t),
        // "rain" is the ASCII code rain; this is weather.
        "rainfall" => particles(src, p, t, Particle::Rain),
        "snow" => particles(src, p, t, Particle::Snow),
        "hearts" => particles(src, p, t, Particle::Hearts),
        "confetti" => particles(src, p, t, Particle::Confetti),
        "embers" => particles(src, p, t, Particle::Embers),
        "sparkles" => particles(src, p, t, Particle::Sparkles),
        "split-2" | "split-3" | "split-4" => split(src, id[6..].parse().unwrap_or(2)),
        "border" => border(src, p),
        "blur-fill" => blur_fill(src, p),
        "color-pop" => color_pop(src, p),
        "fisheye" => fisheye(src, p),
        _ => src.clone(),
    };
    *dst = out;
}

// ---------- Beat: flashes and camera moves ----------

enum Pulse {
    Flash,
    Strobe,
    Invert,
}

fn pulse(src: &Pixmap, p: &Params, t: f32, kind: Pulse) -> Pixmap {
    let rate = num(p, "rate", 2.0) as f32;
    let k = num(p, "intensity", 0.7) as f32;
    let phase = fract(t * rate);
    let mut c = Canvas::from_pixmap(src.clone());
    let (w, h) = (c.w(), c.h());
    match kind {
        Pulse::Flash => {
            let a = k * (-phase * 7.0).exp();
            c.fill_rect_color(0.0, 0.0, w, h, with_alpha(hex(text(p, "color", "#ffffff")), a));
        }
        Pulse::Strobe => {
            if phase >= 0.5 {
                c.fill_rect_color(0.0, 0.0, w, h, rgba(0, 0, 0, k));
            }
        }
        Pulse::Invert => {
            if phase < 0.18 * k.max(0.2) * 2.0 {
                c.set_blend(BlendMode::Difference);
                c.fill_rect_color(0.0, 0.0, w, h, Color::WHITE);
            }
        }
    }
    c.into_pixmap()
}

enum Move {
    ZoomPulse,
    PushIn,
    Handheld,
}

fn moved(src: &Pixmap, p: &Params, t: f32, dur: f32, kind: Move) -> Pixmap {
    let (w, h) = (src.width() as f32, src.height() as f32);
    let u = unit(src);
    let (mut dx, mut dy, mut rot) = (0.0f32, 0.0f32, 0.0f32);
    let scale = match kind {
        Move::ZoomPulse => {
            let rate = num(p, "rate", 2.0) as f32;
            1.0 + num(p, "amount", 0.12) as f32 * (-fract(t * rate) * 6.0).exp()
        }
        Move::PushIn => {
            let x = (t / dur.max(0.1)).clamp(0.0, 1.0);
            let e = x * x * (3.0 - 2.0 * x);
            1.0 + num(p, "amount", 0.15) as f32 * e
        }
        Move::Handheld => {
            // Several slow sines: a drifting, breathing handheld camera.
            let a = num(p, "amount", 10.0) as f32 * u;
            dx = a * ((t * 1.3).sin() * 0.6 + (t * 2.9 + 1.0).sin() * 0.3 + (t * 5.3).sin() * 0.1);
            dy = a * ((t * 1.1 + 2.0).sin() * 0.6 + (t * 3.1).sin() * 0.3 + (t * 4.7 + 3.0).sin() * 0.1);
            rot = 0.012 * (num(p, "amount", 10.0) as f32 / 10.0) * (t * 0.9).sin();
            1.0 + 2.2 * a / w.min(h)
        }
    };
    let mut c = Canvas::from_pixmap(pixmap(src.width(), src.height()));
    c.translate(w / 2.0 + dx, h / 2.0 + dy);
    c.rotate(rot);
    c.scale(scale, scale);
    c.translate(-w / 2.0, -h / 2.0);
    c.draw_image(src, 0.0, 0.0);
    c.into_pixmap()
}

fn speed_lines(src: &Pixmap, p: &Params, t: f32) -> Pixmap {
    let mut c = Canvas::from_pixmap(src.clone());
    let (w, h) = (c.w(), c.h());
    let (cx, cy) = (w / 2.0, h / 2.0);
    let far = (w * w + h * h).sqrt() * 0.6;
    let n = num(p, "density", 60.0) as u32;
    let k = num(p, "intensity", 0.7) as f32;
    let frame = (t * 14.0) as u32;
    c.set_fill_color(with_alpha(Color::WHITE, k));
    for i in 0..n {
        let a = rnd(i, frame) * TAU;
        let spread = 0.004 + rnd(i, frame + 7) * 0.012;
        let inner = far * (0.45 + rnd(i, frame + 13) * 0.3);
        c.begin_path();
        c.move_to(cx + a.cos() * inner, cy + a.sin() * inner);
        c.line_to(cx + (a - spread).cos() * far * 1.4, cy + (a - spread).sin() * far * 1.4);
        c.line_to(cx + (a + spread).cos() * far * 1.4, cy + (a + spread).sin() * far * 1.4);
        c.close_path();
        c.fill();
    }
    c.into_pixmap()
}

// ---------- Light ----------

fn glow_at(c: &mut Canvas, x: f32, y: f32, r: f32, col: Color, a: f32) {
    let f = c.radial(x, y, 0.0, x, y, r.max(1.0), &[(0.0, with_alpha(col, a)), (0.45, with_alpha(col, a * 0.45)), (1.0, with_alpha(col, 0.0))]);
    c.set_fill(f);
    c.begin_path();
    c.circle(x, y, r.max(1.0));
    c.fill();
}

fn light_leak(src: &Pixmap, p: &Params, t: f32) -> Pixmap {
    let mut c = Canvas::from_pixmap(src.clone());
    let (w, h) = (c.w(), c.h());
    let k = num(p, "intensity", 0.7) as f32;
    let sp = num(p, "speed", 1.0) as f32;
    let base = hex(text(p, "color", "#ff8a3d"));
    c.set_blend(BlendMode::Screen);
    let tones = [base, hex("#ff3d6e"), hex("#ffd27a")];
    for (i, col) in tones.iter().enumerate() {
        let f = i as f32 * 1.7;
        let x = w * (0.5 + 0.55 * (t * 0.23 * sp + f).sin());
        let y = h * (0.5 + 0.5 * (t * 0.17 * sp + f * 1.3).cos());
        let r = w.max(h) * (0.45 + 0.15 * (t * 0.3 * sp + f).sin());
        glow_at(&mut c, x, y, r, *col, k * (0.55 + 0.25 * (t * 0.9 * sp + f).sin()));
    }
    c.into_pixmap()
}

fn lens_flare(src: &Pixmap, p: &Params, t: f32) -> Pixmap {
    let mut c = Canvas::from_pixmap(src.clone());
    let (w, h) = (c.w(), c.h());
    let u = unit(src);
    let k = num(p, "intensity", 0.8) as f32;
    let sp = num(p, "speed", 1.0) as f32;
    // The light source drifts along the top; ghosts line up through the centre.
    let sx = w * (0.5 + 0.38 * (t * 0.35 * sp).sin());
    let sy = h * (0.22 + 0.05 * (t * 0.5 * sp).cos());
    c.set_blend(BlendMode::Screen);
    glow_at(&mut c, sx, sy, 260.0 * u, hex("#fff2d6"), k);
    glow_at(&mut c, sx, sy, 70.0 * u, Color::WHITE, k);
    // Anamorphic streak.
    let f = c.linear(sx - w * 0.6, sy, sx + w * 0.6, sy, &[(0.0, with_alpha(hex("#7fb8ff"), 0.0)), (0.5, with_alpha(hex("#cfe4ff"), k * 0.8)), (1.0, with_alpha(hex("#7fb8ff"), 0.0))]);
    c.set_fill(f);
    c.begin_path();
    c.ellipse(sx, sy, w * 0.6, 3.5 * u, 0.0, TAU, false);
    c.fill();
    let (cx, cy) = (w / 2.0, h / 2.0);
    let ghosts = [(0.55, 34.0, "#7dff9b"), (0.9, 18.0, "#c27dff"), (1.35, 52.0, "#ffb27d"), (1.75, 26.0, "#7dd8ff")];
    for (d, r, col) in ghosts {
        let x = sx + (cx - sx) * 2.0 * d;
        let y = sy + (cy - sy) * 2.0 * d;
        glow_at(&mut c, x, y, r * u, hex(col), k * 0.35);
    }
    c.into_pixmap()
}

fn film_burn(src: &Pixmap, p: &Params, t: f32) -> Pixmap {
    let mut c = Canvas::from_pixmap(src.clone());
    let (w, h) = (c.w(), c.h());
    let k = num(p, "intensity", 0.8) as f32;
    let beat = (t * 3.0) as u32;
    let flick = 0.75 + 0.25 * (t * 23.0).sin();
    c.set_blend(BlendMode::Screen);
    for i in 0..3u32 {
        // Blobs hug the edges, jumping every third of a second.
        let side = (rnd(i, beat) * 4.0) as u32;
        let along = rnd(i + 9, beat);
        let (x, y) = match side {
            0 => (along * w, 0.0),
            1 => (w, along * h),
            2 => (along * w, h),
            _ => (0.0, along * h),
        };
        let r = w.max(h) * (0.35 + rnd(i + 3, beat) * 0.3);
        let col = if i == 1 { hex("#ff3b1f") } else { hex("#ff9a2e") };
        glow_at(&mut c, x, y, r, col, k * flick);
    }
    c.set_blend(BlendMode::SourceOver);
    c.fill_rect_color(0.0, 0.0, w, h, rgba(255, 140, 60, 0.08 * k * flick));
    c.into_pixmap()
}

fn film_8mm(src: &Pixmap, p: &Params, t: f32) -> Pixmap {
    let (w, h) = (src.width() as f32, src.height() as f32);
    let u = unit(src);
    let dirt = num(p, "dirt", 0.6) as f32;
    let weave = num(p, "weave", 0.5) as f32;
    let f = (t * 18.0) as u32;
    let mut c = Canvas::from_pixmap(pixmap(src.width(), src.height()));
    c.fill_all(Color::BLACK);
    // Gate weave: the frame jitters a little in the gate.
    c.draw_image(src, (rnd(f, 1) - 0.5) * 4.0 * u * weave, (rnd(f, 2) - 0.5) * 6.0 * u * weave);
    // Warm, faded stock and a flickering exposure.
    c.fill_rect_color(0.0, 0.0, w, h, rgba(120, 80, 30, 0.12));
    c.fill_rect_color(0.0, 0.0, w, h, rgba(0, 0, 0, rnd(f, 3) * 0.12));
    // Dust and hairs.
    let specks = (dirt * 26.0) as u32;
    for i in 0..specks {
        let (x, y) = (rnd(i, f + 11) * w, rnd(i, f + 12) * h);
        let r = (0.6 + rnd(i, f + 13) * 2.4) * u;
        c.set_fill_color(if rnd(i, f + 14) > 0.3 { rgba(10, 8, 6, 0.75) } else { rgba(255, 250, 235, 0.6) });
        c.begin_path();
        c.circle(x, y, r);
        c.fill();
    }
    // A scratch or two running down the frame.
    for i in 0..((dirt * 2.5) as u32) {
        if rnd(i, f / 6 + 40) > 0.55 {
            let x = rnd(i, f / 6 + 41) * w + (rnd(i, f) - 0.5) * 3.0 * u;
            c.fill_rect_color(x, 0.0, 1.2 * u, h, rgba(240, 235, 220, 0.35));
        }
    }
    let v = c.radial(w / 2.0, h / 2.0, w.min(h) * 0.35, w / 2.0, h / 2.0, w.max(h) * 0.75, &[(0.0, rgba(0, 0, 0, 0.0)), (1.0, rgba(0, 0, 0, 0.6))]);
    c.set_fill(v);
    c.fill_rect(0.0, 0.0, w, h);
    c.into_pixmap()
}

// ---------- Particles ----------

#[derive(Clone, Copy, PartialEq)]
enum Particle {
    Bokeh,
    Rain,
    Snow,
    Hearts,
    Confetti,
    Embers,
    Sparkles,
}

fn heart(c: &mut Canvas, x: f32, y: f32, s: f32) {
    c.begin_path();
    c.circle(x - s * 0.26, y - s * 0.12, s * 0.3);
    c.fill();
    c.begin_path();
    c.circle(x + s * 0.26, y - s * 0.12, s * 0.3);
    c.fill();
    c.begin_path();
    c.move_to(x - s * 0.55, y - s * 0.02);
    c.line_to(x + s * 0.55, y - s * 0.02);
    c.line_to(x, y + s * 0.62);
    c.close_path();
    c.fill();
}

fn star(c: &mut Canvas, x: f32, y: f32, r: f32) {
    c.begin_path();
    for i in 0..8 {
        let a = i as f32 * PI / 4.0 - PI / 2.0;
        let rr = if i % 2 == 0 { r } else { r * 0.22 };
        let (px, py) = (x + a.cos() * rr, y + a.sin() * rr);
        if i == 0 { c.move_to(px, py) } else { c.line_to(px, py) }
    }
    c.close_path();
    c.fill();
}

fn particles(src: &Pixmap, p: &Params, t: f32, kind: Particle) -> Pixmap {
    let mut c = Canvas::from_pixmap(src.clone());
    let (w, h) = (c.w(), c.h());
    let u = unit(src);
    let n = num(p, "amount", 40.0) as u32;
    let size = num(p, "size", 1.0) as f32;
    let sp = num(p, "speed", 1.0) as f32;
    if kind == Particle::Rain {
        c.fill_rect_color(0.0, 0.0, w, h, rgba(20, 30, 50, 0.18));
    }
    if matches!(kind, Particle::Bokeh | Particle::Embers | Particle::Sparkles) {
        c.set_blend(BlendMode::Screen);
    }
    let palette = ["#ff4d6d", "#ffd23f", "#3ec1ff", "#7cff6b", "#c77dff", "#ff8c42", "#ffffff"];
    for i in 0..n {
        let (a, b, d) = (rnd(i, 1), rnd(i, 2), rnd(i, 3));
        match kind {
            Particle::Rain => {
                let len = (30.0 + 40.0 * d) * u * size;
                let y = fract(b + t * (1.6 + d) * sp) * (h + len) - len;
                let x = a * w * 1.2 - w * 0.1 + y * 0.18;
                c.set_stroke_color(with_alpha(hex("#dce9ff"), 0.25 + 0.3 * d));
                c.set_line_width(1.4 * u);
                c.begin_path();
                c.move_to(x, y);
                c.line_to(x + len * 0.18, y + len);
                c.stroke();
            }
            Particle::Snow => {
                let r = (1.5 + 4.0 * d) * u * size;
                let y = fract(b + t * (0.06 + 0.08 * d) * sp) * (h + 2.0 * r) - r;
                let x = a * w + (t * (0.6 + d) + i as f32).sin() * 22.0 * u;
                c.set_fill_color(with_alpha(Color::WHITE, 0.6 + 0.35 * d));
                c.begin_path();
                c.circle(x, y, r);
                c.fill();
            }
            Particle::Hearts => {
                let s = (18.0 + 26.0 * d) * u * size;
                let y = h + s - fract(b + t * (0.1 + 0.08 * d) * sp) * (h + 2.0 * s);
                let x = a * w + (t * 1.4 + i as f32).sin() * 18.0 * u;
                let col = if d > 0.5 { "#ff3b6b" } else { "#ff7aa8" };
                c.set_fill_color(with_alpha(hex(col), 0.85));
                heart(&mut c, x, y, s);
            }
            Particle::Confetti => {
                let s = (6.0 + 8.0 * d) * u * size;
                let y = fract(b + t * (0.18 + 0.12 * d) * sp) * (h + 2.0 * s) - s;
                let x = a * w + (t * 2.0 + i as f32).sin() * 26.0 * u;
                c.save();
                c.translate(x, y);
                c.rotate(t * (2.0 + 4.0 * d) + i as f32);
                c.scale(1.0, (t * 5.0 + i as f32).cos());
                c.fill_rect_color(-s / 2.0, -s * 0.3, s, s * 0.6, hex(palette[(i as usize) % palette.len()]));
                c.restore();
            }
            Particle::Embers => {
                let r = (1.2 + 2.6 * d) * u * size;
                let y = h + r - fract(b + t * (0.12 + 0.15 * d) * sp) * (h + 2.0 * r);
                let x = a * w + (t * 1.7 + i as f32 * 2.0).sin() * 30.0 * u;
                let flick = 0.5 + 0.5 * (t * (8.0 + 6.0 * d) + i as f32).sin();
                glow_at(&mut c, x, y, r * 5.0, hex("#ff7a1a"), 0.35 * flick);
                c.set_fill_color(with_alpha(hex("#ffd27a"), 0.9 * flick));
                c.begin_path();
                c.circle(x, y, r);
                c.fill();
            }
            Particle::Sparkles => {
                // Each sparkle lives a few seconds in one place, twinkling.
                let life = 2.5;
                let gen = ((t + d * life) / life) as u32;
                let (x, y) = (rnd(i, gen * 3 + 5) * w, rnd(i, gen * 3 + 6) * h);
                let tw = ((t * (3.0 + 3.0 * d) + i as f32).sin()).max(0.0).powi(4);
                let r = (8.0 + 18.0 * d) * u * size * (0.5 + tw);
                let col = if d > 0.6 { "#ffe08a" } else { "#ffffff" };
                c.set_fill_color(with_alpha(hex(col), tw));
                star(&mut c, x, y, r);
            }
            Particle::Bokeh => {
                let r = (14.0 + 40.0 * d) * u * size;
                let y = h + r - fract(b + t * (0.02 + 0.03 * d) * sp) * (h + 2.0 * r);
                let x = a * w + (t * 0.3 + i as f32).sin() * 40.0 * u;
                let col = ["#ffd9a0", "#ffb3c7", "#b3e1ff"][(i % 3) as usize];
                let pulse = 0.6 + 0.4 * (t * 0.8 + i as f32).sin();
                let f = c.radial(x, y, 0.0, x, y, r, &[(0.0, with_alpha(hex(col), 0.32 * pulse)), (0.8, with_alpha(hex(col), 0.26 * pulse)), (1.0, with_alpha(hex(col), 0.0))]);
                c.set_fill(f);
                c.begin_path();
                c.circle(x, y, r);
                c.fill();
            }
        }
    }
    c.into_pixmap()
}

// ---------- Layouts ----------

/// The frame `n` times: stacked on tall frames, side by side on wide ones.
fn split(src: &Pixmap, n: u32) -> Pixmap {
    let (w, h) = (src.width() as f32, src.height() as f32);
    let u = unit(src);
    let mut c = Canvas::from_pixmap(pixmap(src.width(), src.height()));
    c.fill_all(Color::BLACK);
    let tall = h >= w;
    let gap = 3.0 * u;
    for i in 0..n {
        let (cx, cy, cw, ch) = if tall {
            let ch = (h - gap * (n - 1) as f32) / n as f32;
            (0.0, i as f32 * (ch + gap), w, ch)
        } else {
            let cw = (w - gap * (n - 1) as f32) / n as f32;
            (i as f32 * (cw + gap), 0.0, cw, h)
        };
        // Centre crop of the frame at the cell's shape.
        let k = (cw / w).max(ch / h);
        let (sw, sh) = (cw / k, ch / k);
        c.draw_image_sub(src, (w - sw) / 2.0, (h - sh) / 2.0, sw, sh, cx, cy, cw, ch);
    }
    c.into_pixmap()
}

fn rounded_picture(c: &mut Canvas, src: &Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32) {
    c.save();
    c.begin_path();
    c.round_rect(x, y, w, h, r);
    c.clip();
    c.draw_image_scaled(src, x, y, w, h);
    c.restore();
}

fn border(src: &Pixmap, p: &Params) -> Pixmap {
    let (w, h) = (src.width() as f32, src.height() as f32);
    let u = unit(src);
    let b = num(p, "width", 28.0) as f32 * u;
    let r = num(p, "radius", 18.0) as f32 * u;
    let mut c = Canvas::from_pixmap(pixmap(src.width(), src.height()));
    c.fill_all(hex(text(p, "color", "#ffffff")));
    rounded_picture(&mut c, src, b, b, w - 2.0 * b, h - 2.0 * b, r);
    c.into_pixmap()
}

fn blur_fill(src: &Pixmap, p: &Params) -> Pixmap {
    let (w, h) = (src.width() as f32, src.height() as f32);
    let u = unit(src);
    let s = num(p, "scale", 0.8) as f32;
    let mut c = Canvas::from_pixmap(pixmap(src.width(), src.height()));
    let back = blur_of(src, 36.0);
    c.draw_image_scaled(&back, -w * 0.05, -h * 0.05, w * 1.1, h * 1.1);
    c.fill_rect_color(0.0, 0.0, w, h, rgba(0, 0, 0, 0.25));
    let (fw, fh) = (w * s, h * s);
    rounded_picture(&mut c, src, (w - fw) / 2.0, (h - fh) / 2.0, fw, fh, num(p, "radius", 16.0) as f32 * u);
    c.into_pixmap()
}

// ---------- Per-pixel ----------

fn color_pop(src: &Pixmap, p: &Params) -> Pixmap {
    let target = num(p, "hue", 0.0) as f32;
    let range = num(p, "range", 30.0) as f32;
    let mut out = src.clone();
    for px in out.data_mut().chunks_exact_mut(4) {
        let (r, g, b) = (px[0] as f32, px[1] as f32, px[2] as f32);
        let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
        let sat = if mx > 0.0 { (mx - mn) / mx } else { 0.0 };
        let hue = if mx == mn {
            0.0
        } else if mx == r {
            60.0 * ((g - b) / (mx - mn)).rem_euclid(6.0)
        } else if mx == g {
            60.0 * ((b - r) / (mx - mn) + 2.0)
        } else {
            60.0 * ((r - g) / (mx - mn) + 4.0)
        };
        let d = ((hue - target + 540.0).rem_euclid(360.0) - 180.0).abs();
        // Keep the chosen colour (when it's actually colourful); everything else goes grey.
        let keep = (1.0 - ((d - range) / 12.0).clamp(0.0, 1.0)) * (sat * 3.0).min(1.0);
        let l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        px[0] = (l + (r - l) * keep) as u8;
        px[1] = (l + (g - l) * keep) as u8;
        px[2] = (l + (b - l) * keep) as u8;
    }
    out
}

fn fisheye(src: &Pixmap, p: &Params) -> Pixmap {
    let k = num(p, "amount", 0.6) as f32;
    let (w, h) = (src.width() as i32, src.height() as i32);
    let mut out = pixmap(src.width(), src.height());
    let s = src.data();
    let d = out.data_mut();
    let (hw, hh) = (w as f32 / 2.0, h as f32 / 2.0);
    let norm = hw.max(hh);
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = ((x as f32 - hw) / norm, (y as f32 - hh) / norm);
            let r2 = nx * nx + ny * ny;
            // Edges stay put; the middle swells.
            let f = 1.0 + k * 0.4 * (r2 - 1.0);
            let (sx, sy) = ((hw + nx * f * norm) as i32, (hh + ny * f * norm) as i32);
            if sx >= 0 && sx < w && sy >= 0 && sy < h {
                let (o, i) = (((y * w + x) * 4) as usize, ((sy * w + sx) * 4) as usize);
                d[o..o + 4].copy_from_slice(&s[i..i + 4]);
            }
        }
    }
    out
}
