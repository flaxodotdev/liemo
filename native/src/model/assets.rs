// Asset library: procedural sample clips, music, stills, overlays, fonts, text styles and fades,
// plus the registry that imported media joins.

use super::time::{rng, tc, H, W};
use super::types::*;
use crate::render::canvas::{hex, pixmap, rgba, Canvas};
use crate::render::text::{fill_text, Align, Baseline, TextStyle};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tiny_skia::Pixmap;

pub type DrawFn = fn(&mut Canvas, f32);

/// Kept with the asset for reference (dimensions, streams), even where nothing reads it yet.
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct MediaInfo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub has_video: bool,
    pub has_audio: bool,
}

#[derive(Clone)]
pub struct Asset {
    pub id: String,
    pub kind: AssetType,
    pub name: String,
    pub sub: String,
    pub dur: f64,
    pub draw: Option<DrawFn>,
    /// Canvas composite op for overlays.
    pub blend: Option<&'static str>,
    pub text: Option<TextProps>,
    // Music
    pub bpm: f64,
    pub roots: [i32; 4],
    pub quality: &'static str,
    pub hue: &'static str,
    // Imported media
    pub media: Option<MediaInfo>,
    pub thumb: Option<Arc<Pixmap>>,
    /// A decoded photo.
    pub bitmap: Option<Arc<Pixmap>>,
    /// An animated shader background (fx::paper), drawn instead of `draw`.
    pub shader: Option<&'static str>,
}

impl Asset {
    fn base(id: &str, kind: AssetType, name: &str, sub: &str, dur: f64) -> Self {
        Asset {
            id: id.into(), kind, name: name.into(), sub: sub.into(), dur, draw: None, blend: None, text: None,
            bpm: 0.0, roots: [0; 4], quality: "maj", hue: "#ffffff", media: None, thumb: None, bitmap: None, shader: None,
        }
    }
    fn drawn(id: &str, kind: AssetType, name: &str, sub: &str, dur: f64, draw: DrawFn) -> Self {
        Asset { draw: Some(draw), ..Self::base(id, kind, name, sub, dur) }
    }
}

pub fn chord(q: &str) -> &'static [i32] {
    match q {
        "maj" => &[0, 4, 7],
        "min" => &[0, 3, 7],
        "maj7" => &[0, 4, 7, 11],
        "min7" => &[0, 3, 7, 10],
        "sus" => &[0, 5, 7, 14],
        _ => &[0, 4, 7],
    }
}

// ---------- Stills ----------
fn spot(c: &mut Canvas, a: &str, b: &str, fx: f32, fy: f32) {
    c.fill_rect_color(0.0, 0.0, W(), H(), hex(b));
    let g = c.radial(W() * fx, H() * fy, 0.0, W() * fx, H() * fy, W() * 0.8, &[(0.0, hex(a)), (0.45, hex(&format!("{a}88"))), (1.0, hex(&format!("{b}00")))]);
    c.set_fill(g);
    c.fill_rect(0.0, 0.0, W(), H());
}
fn grid(c: &mut Canvas, _: f32) {
    c.fill_rect_color(0.0, 0.0, W(), H(), hex("#0b1220"));
    c.set_stroke_color(rgba(0, 153, 255, 0.25));
    c.set_line_width(1.0);
    let mut x = 0.0;
    while x < W() {
        c.begin_path();
        c.move_to(x + 0.5, 0.0);
        c.line_to(x + 0.5, H());
        c.stroke();
        x += 40.0;
    }
    let mut y = 0.0;
    while y < H() {
        c.begin_path();
        c.move_to(0.0, y + 0.5);
        c.line_to(W(), y + 0.5);
        c.stroke();
        y += 40.0;
    }
}
fn dots(c: &mut Canvas, _: f32) {
    c.fill_rect_color(0.0, 0.0, W(), H(), hex("#111"));
    c.set_fill_color(hex("#ff7a3d"));
    let mut y = 0.0;
    while y < H() {
        let mut x = 0.0;
        while x < W() {
            c.begin_path();
            c.circle(x + 12.0, y + 12.0, 1.0 + 9.0 * (x / W()) * (1.0 - y / H()));
            c.fill();
            x += 24.0;
        }
        y += 24.0;
    }
}

// ---------- Overlays ----------
fn leak(c: &mut Canvas, t: f32) {
    let x = W() * (0.2 + 0.6 * (0.5 + 0.5 * (t * 0.6).sin()));
    let g = c.radial(x, H() * 0.3, 0.0, x, H() * 0.3, W() * 0.7, &[(0.0, rgba(255, 140, 60, 0.9)), (0.4, rgba(255, 60, 110, 0.35)), (1.0, rgba(0, 0, 0, 0.0))]);
    c.set_fill(g);
    c.fill_rect(0.0, 0.0, W(), H());
}
pub fn grain_tile() -> Arc<Pixmap> {
    static T: OnceLock<Arc<Pixmap>> = OnceLock::new();
    T.get_or_init(|| {
        let mut r = rng(5);
        let mut d = vec![0u8; 256 * 256 * 4];
        for px in d.chunks_exact_mut(4) {
            let v = (r.next() * 255.0) as u8;
            px[0] = v;
            px[1] = v;
            px[2] = v;
            px[3] = 90;
        }
        Arc::new(crate::render::canvas::from_straight(&d, 256, 256))
    })
    .clone()
}
fn grain(c: &mut Canvas, t: f32) {
    let f = (t * 24.0).floor();
    c.save();
    c.translate(-((f * 73.0) % 256.0), -((f * 151.0) % 256.0));
    let p = c.pattern(grain_tile());
    c.set_fill(p);
    c.fill_rect(0.0, 0.0, W() + 256.0, H() + 256.0);
    c.restore();
}
fn letterbox(c: &mut Canvas, _: f32) {
    let bar = (H() - W() / 2.39) / 2.0;
    c.fill_rect_color(0.0, 0.0, W(), bar, hex("#000"));
    c.fill_rect_color(0.0, H() - bar, W(), bar, hex("#000"));
}
fn vignette(c: &mut Canvas, _: f32) {
    let g = c.radial(W() / 2.0, H() / 2.0, H() * 0.35, W() / 2.0, H() / 2.0, W() * 0.65, &[(0.0, rgba(0, 0, 0, 0.0)), (1.0, rgba(0, 0, 0, 0.85))]);
    c.set_fill(g);
    c.fill_rect(0.0, 0.0, W(), H());
}
fn scan(c: &mut Canvas, t: f32) {
    c.set_fill_color(rgba(0, 0, 0, 0.35));
    let mut y = 0.0;
    while y < H() {
        c.fill_rect(0.0, y, W(), 2.0);
        y += 4.0;
    }
    c.fill_rect_color(0.0, (t * 220.0) % H(), W(), 30.0, rgba(255, 255, 255, 0.08));
}
fn dust(c: &mut Canvas, t: f32) {
    let mut r = rng((t * 12.0).floor() as i64 + 1);
    c.set_stroke_color(rgba(255, 255, 255, 0.5));
    c.set_fill_color(rgba(255, 255, 255, 0.6));
    let mut lw = 1.0;
    for _ in 0..6 {
        let x = r.next() * W();
        let w = r.next() * 1.5;
        if w > 0.0 {
            lw = w;
        }
        c.set_line_width(lw);
        c.begin_path();
        c.move_to(x, 0.0);
        c.line_to(x + r.next() * 10.0, H());
        c.stroke();
    }
    for _ in 0..25 {
        let (x, y, rad) = (r.next() * W(), r.next() * H(), r.next() * 2.5);
        c.begin_path();
        c.circle(x, y, rad);
        c.fill();
    }
}
fn rec_frame(c: &mut Canvas, t: f32) {
    c.set_stroke_color(rgba(255, 255, 255, 0.85));
    c.set_line_width(3.0);
    let (m, l) = (40.0, 50.0);
    for (x, y, dx, dy) in [(m, m, 1.0, 1.0), (W() - m, m, -1.0, 1.0), (m, H() - m, 1.0, -1.0), (W() - m, H() - m, -1.0, -1.0)] {
        c.begin_path();
        c.move_to(x, y + l * dy);
        c.line_to(x, y);
        c.line_to(x + l * dx, y);
        c.stroke();
    }
    if (t * 2.0).floor() as i64 % 2 == 0 {
        c.set_fill_color(hex("#ff3b3b"));
        c.begin_path();
        c.circle(m + 30.0, m + 34.0, 9.0);
        c.fill();
    }
    let st = TextStyle { family: "JetBrains Mono", weight: 600.0, size: 24.0, spacing: 0.0 };
    fill_text(c, "REC", m + 50.0, m + 43.0, &st, Align::Left, Baseline::Alphabetic, hex("#fff"));
    fill_text(c, &tc(t as f64), W() - m - 200.0, m + 43.0, &st, Align::Left, Baseline::Alphabetic, hex("#fff"));
}

// ---------- Effects: confetti, bursts, scribbles, a cursor and keys (after remocn's set) ----------
fn ease_out(p: f32) -> f32 {
    1.0 - (1.0 - p.clamp(0.0, 1.0)).powi(3)
}
/// Stroke the first `p` (0..1) of a polyline.
fn stroke_part(c: &mut Canvas, pts: &[(f32, f32)], p: f32) {
    if pts.len() < 2 || p <= 0.0 {
        return;
    }
    let lens: Vec<f32> = pts.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).collect();
    let mut left = lens.iter().sum::<f32>() * p.min(1.0);
    c.begin_path();
    c.move_to(pts[0].0, pts[0].1);
    for (i, l) in lens.iter().enumerate() {
        if left >= *l {
            c.line_to(pts[i + 1].0, pts[i + 1].1);
            left -= l;
        } else {
            let k = left / l.max(1e-6);
            c.line_to(pts[i].0 + (pts[i + 1].0 - pts[i].0) * k, pts[i].1 + (pts[i + 1].1 - pts[i].1) * k);
            break;
        }
    }
    c.stroke();
}
const PARTY: [&str; 6] = ["#facc15", "#ff5577", "#22c55e", "#0099ff", "#d44df0", "#ff7a3d"];
fn confetti(c: &mut Canvas, t: f32) {
    // Two cannons in the lower corners; pieces fly up and in, then flutter down.
    let mut r = rng(11);
    for i in 0..220 {
        let left = i % 2 == 0;
        let (x0, y0) = (if left { -20.0 } else { W() + 20.0 }, H() * 0.95);
        let ang = (38.0 + r.next() * 47.0).to_radians();
        let speed = H() * (0.9 + r.next() * 1.6);
        let delay = r.next() * 0.3;
        let tt = (t - delay).max(0.0);
        if tt <= 0.0 {
            continue;
        }
        let drag = (1.0 - (-tt * 1.6).exp()) / 1.6;
        let x = x0 + if left { 1.0 } else { -1.0 } * ang.cos() * speed * drag + (tt * 3.0 + i as f32).sin() * 18.0;
        let y = y0 - ang.sin() * speed * drag + 260.0 * tt * tt * 0.5;
        if y > H() + 40.0 {
            continue;
        }
        let spin = tt * (4.0 + r.next() * 8.0) + i as f32;
        let (w, h) = (10.0 + r.next() * 10.0, 6.0 + r.next() * 7.0);
        c.save();
        c.translate(x, y);
        c.rotate(spin);
        c.scale(1.0, spin.cos().abs().max(0.15));
        c.set_alpha((1.0 - (tt - 3.0).max(0.0)).clamp(0.0, 1.0));
        c.fill_rect_color(-w / 2.0, -h / 2.0, w, h, hex(PARTY[i % PARTY.len()]));
        c.restore();
    }
}
fn radial_burst(c: &mut Canvas, t: f32) {
    // Lines shoot out of the middle and fade, every two seconds.
    let p = (t % 2.0) / 0.7;
    if p >= 1.0 {
        return;
    }
    let (cx, cy) = (W() / 2.0, H() / 2.0);
    c.set_line_cap(tiny_skia::LineCap::Round);
    c.set_line_width(6.0);
    c.set_stroke_color(hex("#ffffff"));
    c.set_alpha(1.0 - p);
    for i in 0..12 {
        let a = i as f32 / 12.0 * std::f32::consts::TAU;
        let (r0, r1) = (60.0 + ease_out(p) * 120.0, 90.0 + ease_out(p * 1.4) * 170.0);
        c.begin_path();
        c.move_to(cx + a.cos() * r0, cy + a.sin() * r0);
        c.line_to(cx + a.cos() * r1, cy + a.sin() * r1);
        c.stroke();
    }
    c.set_alpha(1.0);
}
fn scribble(c: &mut Canvas, t: f32) {
    // A loose ring, drawn round about one and a half times, as if with a marker.
    let (cx, cy, rx, ry) = (W() / 2.0, H() / 2.0, W() * 0.2, H() * 0.18);
    let pts: Vec<(f32, f32)> = (0..=120)
        .map(|i| {
            let k = i as f32 / 120.0;
            let a = -2.2 + k * std::f32::consts::TAU * 1.15;
            let wob = 1.0 + 0.05 * (k * 19.0).sin() + 0.06 * k;
            (cx + a.cos() * rx * wob, cy + a.sin() * ry * wob * (1.0 - 0.04 * k))
        })
        .collect();
    c.set_line_cap(tiny_skia::LineCap::Round);
    c.set_line_width(9.0);
    c.set_stroke_color(hex("#ff3b3b"));
    stroke_part(c, &pts, ease_out(t / 0.8));
}
fn ink_arrow(c: &mut Canvas, t: f32) {
    // A curved stroke from the lower left that ends in an arrowhead at the middle.
    let (a, b, d) = ((W() * 0.18, H() * 0.82), (W() * 0.22, H() * 0.42), (W() * 0.44, H() * 0.48));
    let pts: Vec<(f32, f32)> = (0..=60)
        .map(|i| {
            let k = i as f32 / 60.0;
            let u = 1.0 - k;
            (u * u * a.0 + 2.0 * u * k * b.0 + k * k * d.0, u * u * a.1 + 2.0 * u * k * b.1 + k * k * d.1)
        })
        .collect();
    let p = ease_out(t / 0.7);
    c.set_line_cap(tiny_skia::LineCap::Round);
    c.set_line_width(8.0);
    c.set_stroke_color(hex("#ffffff"));
    stroke_part(c, &pts, p);
    if p >= 0.98 {
        let h = ease_out((t - 0.65) / 0.2);
        let n = pts.len();
        let (dx, dy) = (pts[n - 1].0 - pts[n - 4].0, pts[n - 1].1 - pts[n - 4].1);
        let ang = dy.atan2(dx);
        for s in [-1.0f32, 1.0] {
            let a2 = ang + std::f32::consts::PI + s * 0.55;
            c.begin_path();
            c.move_to(d.0, d.1);
            c.line_to(d.0 + a2.cos() * 34.0 * h, d.1 + a2.sin() * 34.0 * h);
            c.stroke();
        }
    }
}
fn cursor_click(c: &mut Canvas, t: f32) {
    // The pointer glides in, clicks in the middle (a ripple), rests, and does it again every 3 s.
    let lt = t % 3.0;
    let k = ease_out(lt / 1.0);
    let (x0, y0, x1, y1) = (W() * 0.78, H() * 0.86, W() / 2.0, H() / 2.0);
    let (x, y) = (x0 + (x1 - x0) * k, y0 + (y1 - y0) * k - (k * std::f32::consts::PI).sin() * 40.0);
    if lt > 1.0 && lt < 1.8 {
        let p = (lt - 1.0) / 0.8;
        c.set_stroke_color(hex("#ffffff"));
        c.set_line_width(3.0);
        c.set_alpha(1.0 - p);
        c.begin_path();
        c.circle(x1, y1, 10.0 + p * 46.0);
        c.stroke();
        c.set_alpha(1.0);
    }
    let press = if lt > 1.0 && lt < 1.15 { 0.85 } else { 1.0 };
    c.save();
    c.translate(x, y);
    c.scale(press * 2.6, press * 2.6);
    let arrow = [(0.0, 0.0), (0.0, 22.0), (5.5, 17.0), (9.5, 26.0), (13.0, 24.5), (9.0, 15.5), (16.0, 15.5)];
    c.begin_path();
    c.move_to(arrow[0].0, arrow[0].1);
    for p in &arrow[1..] {
        c.line_to(p.0, p.1);
    }
    c.close_path();
    c.set_fill_color(hex("#111111"));
    c.fill();
    c.set_stroke_color(hex("#ffffff"));
    c.set_line_width(1.6);
    c.stroke();
    c.restore();
}
fn keystroke(c: &mut Canvas, t: f32) {
    // Two key caps, ⌘ then K, pressed one after the other near the bottom.
    let st = TextStyle { family: "Inter", weight: 600.0, size: 40.0, spacing: 0.0 };
    for (i, key) in ["⌘", "K"].iter().enumerate() {
        let appear = ease_out((t - i as f32 * 0.25) / 0.3);
        if appear <= 0.0 {
            continue;
        }
        let down = t > 0.6 + i as f32 * 0.12 && t < 0.85 + i as f32 * 0.12;
        let (w, h) = (96.0, 96.0);
        let x = W() / 2.0 - 104.0 + i as f32 * 112.0;
        let y = H() * 0.78 + (1.0 - appear) * 40.0 + if down { 6.0 } else { 0.0 };
        c.set_alpha(appear);
        if !down {
            c.set_fill_color(rgba(0, 0, 0, 0.5));
            c.begin_path();
            c.round_rect(x, y + 8.0, w, h, 18.0);
            c.fill();
        }
        c.set_fill_color(if down { hex("#d9d9de") } else { hex("#f4f4f6") });
        c.begin_path();
        c.round_rect(x, y, w, h, 18.0);
        c.fill();
        fill_text(c, key, x + w / 2.0, y + h / 2.0, &st, Align::Center, Baseline::Middle, hex("#18181b"));
    }
    c.set_alpha(1.0);
}
// ---------- Sample footage: nature photographs (Unsplash licence) in slow camera moves ----------
struct Shot {
    jpg: &'static [u8],
    /// Zoom from, zoom to, and the drift across the clip as a fraction of the frame (x, y).
    zoom: (f32, f32),
    pan: (f32, f32),
}
const SHOTS: [Shot; 6] = [
    Shot { jpg: include_bytes!("../../assets/footage/coastal-cliffs.jpg"), zoom: (1.04, 1.16), pan: (0.03, -0.01) },
    Shot { jpg: include_bytes!("../../assets/footage/canyon-sunset.jpg"), zoom: (1.14, 1.05), pan: (-0.04, 0.0) },
    Shot { jpg: include_bytes!("../../assets/footage/snow-peaks.jpg"), zoom: (1.06, 1.12), pan: (-0.03, 0.02) },
    Shot { jpg: include_bytes!("../../assets/footage/forest-waterfall.jpg"), zoom: (1.03, 1.15), pan: (0.0, 0.02) },
    Shot { jpg: include_bytes!("../../assets/footage/valley-pines.jpg"), zoom: (1.12, 1.04), pan: (0.03, 0.0) },
    Shot { jpg: include_bytes!("../../assets/footage/ocean-surf.jpg"), zoom: (1.05, 1.18), pan: (-0.02, -0.02) },
];
const SHOT_LEN: f32 = 6.0;

fn shot_image(i: usize) -> &'static Pixmap {
    static P: OnceLock<Vec<Pixmap>> = OnceLock::new();
    &P.get_or_init(|| {
        SHOTS
            .iter()
            .map(|s| {
                let img = image::load_from_memory(s.jpg).expect("bundled footage").to_rgba8();
                let (w, h) = img.dimensions();
                crate::render::canvas::from_straight(img.as_raw(), w, h)
            })
            .collect()
    })[i]
}

/// A slow, eased push-in or pull-out with a drift, always covering the frame.
fn shot(c: &mut Canvas, t: f32, i: usize) {
    let s = &SHOTS[i];
    let pm = shot_image(i);
    let p = (t / SHOT_LEN).clamp(0.0, 1.0);
    let e = p * p * (3.0 - 2.0 * p);
    let zoom = s.zoom.0 + (s.zoom.1 - s.zoom.0) * e;
    let (iw, ih) = (pm.width() as f32, pm.height() as f32);
    let k = (W() / iw).max(H() / ih) * zoom;
    let (dw, dh) = (iw * k, ih * k);
    // The drift stays inside the overscan, so no edge ever shows.
    let (sx, sy) = ((dw - W()) / 2.0, (dh - H()) / 2.0);
    let ox = (s.pan.0 * (e - 0.5) * 2.0 * W()).clamp(-sx, sx);
    let oy = (s.pan.1 * (e - 0.5) * 2.0 * H()).clamp(-sy, sy);
    c.draw_image_scaled(pm, (W() - dw) / 2.0 + ox, (H() - dh) / 2.0 + oy, dw, dh);
}

pub fn scenes() -> Vec<Asset> {
    use AssetType::Scene;
    vec![
        Asset::drawn("sc-coast", Scene, "Coastal cliffs", "4K · 24p · Sea stacks", 6.0, SCENE_DRAWS[0]),
        Asset::drawn("sc-drive", Scene, "Canyon sunset", "4K · 24p · Golden hour", 6.0, SCENE_DRAWS[1]),
        Asset::drawn("sc-aurora", Scene, "Snow peaks", "4K · 24p · Aerial", 6.0, SCENE_DRAWS[2]),
        Asset::drawn("sc-studio", Scene, "Forest waterfall", "4K · 24p · Gorge", 6.0, SCENE_DRAWS[3]),
        Asset::drawn("sc-ink", Scene, "Valley pines", "4K · 24p · Yosemite", 6.0, SCENE_DRAWS[4]),
        Asset::drawn("sc-surf", Scene, "Ocean surf", "4K · 30p · Aerial", 6.0, SCENE_DRAWS[5]),
    ]
}
/// Animated shader backgrounds from remocn (see fx::paper).
pub fn shaders() -> Vec<Asset> {
    crate::fx::paper::backgrounds()
        .iter()
        .map(|b| Asset { shader: Some(b.id.as_str()), ..Asset::base(&format!("sh-{}", b.id.trim_start_matches("shader-")), AssetType::Scene, &b.name, &b.sub, 8.0) })
        .collect()
}
pub const SCENE_DRAWS: [DrawFn; 6] = [|c, t| shot(c, t, 0), |c, t| shot(c, t, 1), |c, t| shot(c, t, 2), |c, t| shot(c, t, 3), |c, t| shot(c, t, 4), |c, t| shot(c, t, 5)];

pub fn music() -> Vec<Asset> {
    let m = |id: &str, name: &str, sub: &str, bpm: f64, dur: f64, roots: [i32; 4], quality: &'static str, hue: &'static str| Asset {
        bpm, roots, quality, hue, ..Asset::base(id, AssetType::Music, name, sub, dur)
    };
    vec![
        m("mu-golden", "Golden Hour", "Warm · Lo-fi", 84.0, 32.0, [57, 53, 60, 55], "maj7", "#ff7a3d"),
        m("mu-neon", "Neon Tide", "Synthwave · Drive", 108.0, 30.0, [45, 41, 48, 43], "min", "#d44df0"),
        m("mu-paper", "Paper Planes", "Acoustic · Bright", 96.0, 28.0, [60, 55, 57, 53], "maj", "#22c55e"),
        m("mu-orbit", "Slow Orbit", "Ambient · Cinematic", 70.0, 40.0, [50, 46, 53, 48], "sus", "#6a4cf5"),
        m("mu-tape", "Tape Machine", "Hip-hop · Dusty", 90.0, 24.0, [52, 48, 55, 50], "min7", "#ff5577"),
    ]
}

pub fn images() -> Vec<Asset> {
    use AssetType::Image;
    vec![
        Asset::drawn("im-magenta", Image, "Magenta bloom", "Gradient still", 5.0, |c, _| spot(c, "#d44df0", "#3b0a52", 0.3, 0.35)),
        Asset::drawn("im-violet", Image, "Violet field", "Gradient still", 5.0, |c, _| spot(c, "#6a4cf5", "#0b0838", 0.7, 0.4)),
        Asset::drawn("im-sunset", Image, "Sunset wash", "Gradient still", 5.0, |c, _| spot(c, "#ff7a3d", "#4a0f2a", 0.5, 0.8)),
        Asset::drawn("im-coral", Image, "Coral glow", "Gradient still", 5.0, |c, _| spot(c, "#ff5577", "#1d0710", 0.2, 0.7)),
        Asset::drawn("im-grid", Image, "Blueprint grid", "Pattern", 5.0, grid),
        Asset::drawn("im-dots", Image, "Halftone", "Pattern", 5.0, dots),
    ]
}

pub fn overlays() -> Vec<Asset> {
    use AssetType::Overlay;
    let o = |id: &str, name: &str, sub: &str, blend: &'static str, draw: DrawFn| Asset { blend: Some(blend), ..Asset::drawn(id, Overlay, name, sub, 6.0, draw) };
    vec![
        o("ov-leak", "Light leak", "Screen blend", "screen", leak),
        o("ov-grain", "Film grain", "Overlay blend", "overlay", grain),
        o("ov-letterbox", "Letterbox 2.39:1", "Cinema bars", "source-over", letterbox),
        o("ov-vignette", "Vignette", "Edge falloff", "source-over", vignette),
        o("ov-scan", "VHS scanlines", "Multiply", "multiply", scan),
        o("ov-dust", "Dust & scratches", "Screen blend", "screen", dust),
        o("ov-frame", "REC frame", "Camera HUD", "source-over", rec_frame),
        o("ov-confetti", "Confetti", "Celebration burst", "source-over", confetti),
        o("ov-burst", "Radial burst", "Emphasis lines", "source-over", radial_burst),
        o("ov-scribble", "Scribble circle", "Hand-drawn ring", "source-over", scribble),
        o("ov-arrow", "Ink arrow", "Points at the middle", "source-over", ink_arrow),
        o("ov-cursor", "Cursor click", "Pointer and ripple", "source-over", cursor_click),
        o("ov-keys", "Keystroke", "⌘ K pressed", "source-over", keystroke),
    ]
}

// ---------- Fonts & text styles ----------
pub struct FontDef {
    pub family: &'static str,
    pub role: &'static str,
    pub weight: f64,
    pub track: f32,
}
pub const FONTS: [FontDef; 6] = [
    FontDef { family: "Mona Sans", role: "Display · Geometric", weight: 700.0, track: -0.045 },
    FontDef { family: "Inter", role: "Sans · Interface", weight: 600.0, track: -0.02 },
    FontDef { family: "Instrument Serif", role: "Serif · Editorial", weight: 400.0, track: -0.01 },
    FontDef { family: "Bebas Neue", role: "Condensed · Poster", weight: 400.0, track: 0.02 },
    FontDef { family: "JetBrains Mono", role: "Mono · Technical", weight: 500.0, track: 0.0 },
    FontDef { family: "Caveat", role: "Script · Handwritten", weight: 700.0, track: 0.0 },
];
/// A bundled font, or any font installed on the Mac (made once per family); unknown → Mona Sans.
pub fn font_def(family: &str) -> &'static FontDef {
    if let Some(f) = FONTS.iter().find(|f| f.family == family) {
        return f;
    }
    if !crate::render::text::is_installed(family) {
        return &FONTS[0];
    }
    static INSTALLED: OnceLock<parking_lot::Mutex<HashMap<String, &'static FontDef>>> = OnceLock::new();
    let mut m = INSTALLED.get_or_init(Default::default).lock();
    *m.entry(family.to_string())
        .or_insert_with(|| Box::leak(Box::new(FontDef { family: Box::leak(family.to_string().into_boxed_str()), role: "Installed", weight: 400.0, track: 0.0 })))
}

fn tp(content: &str, font: &str, size: f64, weight: f64, color: &str, align: TextAlign, anim: TextAnim, boxed: bool, y: f64) -> TextProps {
    TextProps { blend: TextBlend::Normal, ..tp_full(content, font, size, weight, color, align, anim, boxed, y) }
}
/// Same, with the blend style set (Invert reads on any footage).
fn tp_inv(content: &str, font: &str, size: f64, weight: f64, color: &str, align: TextAlign, anim: TextAnim, boxed: bool, y: f64) -> TextProps {
    TextProps { blend: TextBlend::Invert, ..tp_full(content, font, size, weight, color, align, anim, boxed, y) }
}
fn tp_full(content: &str, font: &str, size: f64, weight: f64, color: &str, align: TextAlign, anim: TextAnim, boxed: bool, y: f64) -> TextProps {
    TextProps { content: content.into(), font: font.into(), size, weight, color: color.into(), align, anim, boxed, y, blend: TextBlend::Normal, outline: None, outline_width: 6.0, shadow: None, glow: None, highlight: None, glass: None, tilt_x: 0.0, tilt_y: 0.0 }
}
pub fn text_presets() -> Vec<Asset> {
    use TextAlign::*;
    use TextAnim::*;
    let t = |id: &str, name: &str, sub: &str, dur: f64, text: TextProps| Asset { text: Some(text), ..Asset::base(id, AssetType::Text, name, sub, dur) };
    vec![
        t("tx-title", "Title", "Centered display", 4.0, tp("Summer, slowly", "Mona Sans", 120.0, 700.0, "#ffffff", Center, Rise, false, 0.0)),
        t("tx-invert", "Invert", "Readable on anything", 4.0, tp_inv("STAY BRIGHT", "Bebas Neue", 150.0, 400.0, "#ffffff", Center, Fade, false, 0.0)),
        t("tx-lower", "Lower third", "Name + role", 4.0, tp("Marta Silva\nDirector of photography", "Inter", 44.0, 600.0, "#ffffff", Left, Slide, true, 200.0)),
        t("tx-caption", "Caption", "Subtitle line", 3.0, tp("The light changes every ten minutes here.", "Inter", 36.0, 500.0, "#ffffff", Center, Fade, true, 260.0)),
        t("tx-quote", "Serif quote", "Editorial", 4.0, tp("“We came for the sea,\nwe stayed for the light.”", "Instrument Serif", 76.0, 400.0, "#ffffff", Center, Fade, false, 0.0)),
        t("tx-poster", "Poster", "Condensed caps", 3.0, tp("CHAPTER TWO", "Bebas Neue", 150.0, 400.0, "#ff7a3d", Center, Type, false, 0.0)),
        t("tx-note", "Handwritten", "Script note", 3.0, tp("day three", "Caveat", 90.0, 700.0, "#ffd9a0", Center, Rise, false, -180.0)),
    ]
}
pub const TEXT_COLORS: [&str; 9] = ["#ffffff", "#000000", "#ffd9a0", "#ff7a3d", "#ff5577", "#d44df0", "#6a4cf5", "#0099ff", "#22c55e"];
pub const TEXT_ANIMS: [(TextAnim, &str); 33] = [
    (TextAnim::None, "None"), (TextAnim::Fade, "Fade"), (TextAnim::Rise, "Rise"), (TextAnim::Slide, "Slide"), (TextAnim::Descend, "Descend"),
    (TextAnim::Type, "Type"), (TextAnim::Caret, "Typewriter"), (TextAnim::Pop, "Pop"), (TextAnim::Bounce, "Bounce"), (TextAnim::Zoom, "Zoom"),
    (TextAnim::Settle, "Settle"), (TextAnim::Squeeze, "Squeeze"), (TextAnim::Stretch, "Stretch"), (TextAnim::Blur, "Blur"), (TextAnim::Fog, "Fog"),
    (TextAnim::Reveal, "Mask reveal"), (TextAnim::Wipe, "Wipe"), (TextAnim::Echo, "Echo"), (TextAnim::Wave, "Wave"), (TextAnim::Drop, "Letter drop"),
    (TextAnim::Spring, "Letter spring"), (TextAnim::Flip, "Rolodex"), (TextAnim::Tracking, "Tracking in"), (TextAnim::Scramble, "Decode"),
    (TextAnim::Words, "Word by word"), (TextAnim::WordRise, "Word rise"), (TextAnim::WordPop, "Word pop"), (TextAnim::Marker, "Marker"),
    (TextAnim::Underline, "Underline"), (TextAnim::Shimmer, "Shimmer"), (TextAnim::Count, "Count up"), (TextAnim::Glitch, "Glitch"), (TextAnim::Flicker, "Flicker"),
];
/// A text animation's name in project JSON and the MCP tools ("wordrise").
pub fn text_anim_id(a: TextAnim) -> String {
    serde_json::to_value(a).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

/// Fades listed with the transitions.
pub const FADES: [(&str, &str, &str); 3] =
    [("fadein", "Fade in", "0.5 s from black"), ("fadeout", "Fade out", "0.5 s to black"), ("both", "Fade both", "0.5 s each end")];

// ---------- Registry ----------
fn registry() -> &'static RwLock<HashMap<String, Arc<Asset>>> {
    static R: OnceLock<RwLock<HashMap<String, Arc<Asset>>>> = OnceLock::new();
    R.get_or_init(|| {
        let mut m = HashMap::new();
        for a in scenes().into_iter().chain(shaders()).chain(music()).chain(images()).chain(overlays()).chain(text_presets()) {
            m.insert(a.id.clone(), Arc::new(a));
        }
        RwLock::new(m)
    })
}
pub fn asset(id: &str) -> Option<Arc<Asset>> {
    registry().read().get(id).cloned()
}
pub fn has_asset(id: &str) -> bool {
    registry().read().contains_key(id)
}
pub fn register(a: Asset) {
    registry().write().insert(a.id.clone(), Arc::new(a));
}
pub fn update_asset(id: &str, f: impl FnOnce(&mut Asset)) {
    let mut r = registry().write();
    if let Some(a) = r.get(id) {
        let mut a = (**a).clone();
        f(&mut a);
        r.insert(id.to_string(), Arc::new(a));
    }
}

/// A still's picture, drawn once at 1280×720.
pub fn bitmap_of(a: &Asset) -> Option<Arc<Pixmap>> {
    static C: OnceLock<RwLock<HashMap<String, Arc<Pixmap>>>> = OnceLock::new();
    let c = C.get_or_init(|| RwLock::new(HashMap::new()));
    if let Some(b) = &a.bitmap {
        return Some(b.clone());
    }
    let draw = a.draw?;
    let key = format!("{}@{}x{}", a.id, W(), H());
    if let Some(p) = c.read().get(&key) {
        return Some(p.clone());
    }
    let mut cv = Canvas::from_pixmap(pixmap(W() as u32, H() as u32));
    draw(&mut cv, 0.0);
    let p = Arc::new(cv.into_pixmap());
    c.write().insert(key, p.clone());
    Some(p)
}
