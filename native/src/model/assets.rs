// Asset library: procedural sample clips, music, stills, overlays, fonts, text styles and fades,
// plus the registry that imported media joins.

use super::time::{lerp, rng, tc, H, W};
use super::types::*;
use crate::render::canvas::{hex, pixmap, rgba, Canvas};
use crate::render::text::{fill_text, Align, Baseline, TextStyle};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::f32::consts::PI;
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

// ---------- Sample scenes (stand-ins for footage) ----------
fn coast(c: &mut Canvas, t: f32) {
    let g = c.linear(0.0, 0.0, 0.0, H() * 0.62, &[(0.0, hex("#2b1d5c")), (0.55, hex("#d4507a")), (1.0, hex("#ff9a4d"))]);
    c.set_fill(g);
    c.fill_rect(0.0, 0.0, W(), H());
    c.set_fill_color(hex("#ffd9a0"));
    c.begin_path();
    c.arc(W() * 0.62, H() * 0.5 + t * 14.0, 70.0, 0.0, PI * 2.0);
    c.fill();
    let s = c.linear(0.0, H() * 0.6, 0.0, H(), &[(0.0, hex("#3a2a6a")), (1.0, hex("#0d0b24"))]);
    c.set_fill(s);
    c.fill_rect(0.0, H() * 0.6, W(), H() * 0.4);
    c.set_stroke_color(rgba(255, 200, 150, 0.35));
    c.set_line_width(2.0);
    for i in 0..14 {
        let fi = i as f32;
        let y = H() * 0.62 + fi * fi * 1.9;
        c.begin_path();
        let mut x = 0.0;
        while x <= W() {
            c.line_to(x, y + (x * 0.02 + t * 1.6 + fi).sin() * (1.0 + fi * 0.4));
            x += 20.0;
        }
        c.stroke();
    }
}

fn drive(c: &mut Canvas, t: f32) {
    c.fill_rect_color(0.0, 0.0, W(), H(), hex("#07070d"));
    let mut r = rng(7);
    for i in 0..40 {
        let bw = 40.0 + r.next() * 90.0;
        let bh = 120.0 + r.next() * 260.0;
        let x = (((i as f32 * 70.0 - t * 160.0) % (W() + 200.0)) + W() + 200.0) % (W() + 200.0) - 100.0;
        c.fill_rect_color(x, H() * 0.55 - bh, bw, bh, hex("#12121e"));
        for k in 0..18 {
            if r.next() > 0.55 {
                let col = if r.next() > 0.5 { hex("#ffcf6b") } else { hex("#6bc8ff") };
                c.set_alpha(0.7);
                c.fill_rect_color(x + 6.0 + (k % 3) as f32 * (bw / 3.2), H() * 0.55 - bh + 10.0 + (k / 3) as f32 * 22.0, 6.0, 9.0, col);
            }
        }
        c.set_alpha(1.0);
    }
    c.set_fill_color(hex("#0e0e16"));
    c.begin_path();
    c.move_to(0.0, H());
    c.line_to(W() * 0.46, H() * 0.55);
    c.line_to(W() * 0.54, H() * 0.55);
    c.line_to(W(), H());
    c.fill();
    c.set_fill_color(hex("#f5f5f5"));
    for i in 0..8 {
        let p = (i as f32 / 8.0 + t * 0.6) % 1.0;
        let pp = p * p;
        c.fill_rect(W() / 2.0 - lerp(2.0, 18.0, pp) / 2.0, lerp(H() * 0.56, H(), pp), lerp(2.0, 18.0, pp), lerp(4.0, 70.0, pp));
    }
    let glow = c.radial(W() * 0.3, H() * 0.5, 0.0, W() * 0.3, H() * 0.5, 300.0, &[(0.0, rgba(255, 60, 90, 0.25)), (1.0, rgba(0, 0, 0, 0.0))]);
    c.set_fill(glow);
    c.fill_rect(0.0, 0.0, W(), H());
}

fn aurora(c: &mut Canvas, t: f32) {
    c.fill_rect_color(0.0, 0.0, W(), H(), hex("#04060f"));
    let mut r = rng(3);
    c.set_fill_color(hex("#fff"));
    for i in 0..160 {
        c.set_alpha(0.3 + 0.7 * (t * 2.0 + i as f32).sin().abs());
        let x = r.next() * W();
        let y = r.next() * H() * 0.7;
        c.fill_rect(x, y, 1.5, 1.5);
    }
    c.set_alpha(1.0);
    c.set_op("screen");
    for (col, o) in [(rgba(60, 255, 170, 0.22), 0.0), (rgba(106, 76, 245, 0.28), 2.0), (rgba(212, 77, 240, 0.18), 4.0)] {
        c.set_fill_color(col);
        for b in 0..3 {
            let b = b as f32;
            c.begin_path();
            c.move_to(0.0, H());
            let mut x = 0.0;
            while x <= W() {
                c.line_to(x, H() * 0.35 + (x * 0.004 + t * 0.7 + o + b).sin() * 80.0 + (x * 0.011 - t + b).sin() * 30.0 - b * 25.0);
                x += 16.0;
            }
            c.line_to(W(), H());
            c.fill();
        }
    }
    c.set_op("source-over");
    c.set_fill_color(hex("#02030a"));
    c.begin_path();
    c.move_to(0.0, H());
    let mut x = 0.0;
    while x <= W() {
        c.line_to(x, H() * 0.82 - (x * 0.007).sin().abs() * 90.0 - (x * 0.03).sin() * 12.0);
        x += 40.0;
    }
    c.line_to(W(), H());
    c.fill();
}

fn studio(c: &mut Canvas, t: f32) {
    let g = c.linear(0.0, 0.0, 0.0, H(), &[(0.0, hex("#1a1a1a")), (0.7, hex("#3a3a3a")), (1.0, hex("#202020"))]);
    c.set_fill(g);
    c.fill_rect(0.0, 0.0, W(), H());
    let lx = W() / 2.0 + (t * 0.9).sin() * 380.0;
    let s = c.radial(lx, H() * 0.5, 10.0, lx, H() * 0.5, 520.0, &[(0.0, rgba(255, 255, 255, 0.35)), (1.0, rgba(255, 255, 255, 0.0))]);
    c.set_fill(s);
    c.fill_rect(0.0, 0.0, W(), H());
    c.save();
    c.translate(W() / 2.0, H() * 0.58);
    c.rotate(t * 0.5);
    c.fill_rect_color(-80.0, -80.0, 160.0, 160.0, hex("#f2f2f2"));
    c.restore();
    c.set_fill_color(rgba(0, 0, 0, 0.35));
    c.begin_path();
    c.move_to(W() / 2.0 + 150.0, H() * 0.78);
    c.ellipse(W() / 2.0, H() * 0.78, 150.0, 18.0, 0.0, PI * 2.0, false);
    c.fill();
}

fn ink(c: &mut Canvas, t: f32) {
    c.fill_rect_color(0.0, 0.0, W(), H(), hex("#f3f0ea"));
    let mut r = rng(11);
    for i in 0..9 {
        let x = W() * (0.2 + r.next() * 0.6);
        let y0 = H() * (0.2 + r.next() * 0.5);
        let y = y0 + t * 20.0 * (r.next() + 0.3);
        let rad = 40.0 + t * (30.0 + r.next() * 50.0);
        let (cr, cg, cb) = match i % 3 { 0 => (106, 76, 245), 1 => (212, 77, 240), _ => (20, 20, 40) };
        let g = c.radial(x, y, 0.0, x, y, rad, &[(0.0, rgba(cr, cg, cb, 0.55)), (1.0, rgba(cr, cg, cb, 0.0))]);
        c.set_fill(g);
        c.begin_path();
        c.circle(x, y, rad);
        c.fill();
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

pub fn scenes() -> Vec<Asset> {
    use AssetType::Scene;
    vec![
        Asset::drawn("sc-coast", Scene, "Coastline at dusk", "4K · 24p · Lisbon", 6.0, coast),
        Asset::drawn("sc-drive", Scene, "Night drive", "HD · 30p · Dashcam", 5.0, drive),
        Asset::drawn("sc-aurora", Scene, "Aurora field", "4K · 24p · Timelapse", 6.0, aurora),
        Asset::drawn("sc-studio", Scene, "Studio sweep", "HD · 60p · Product", 4.0, studio),
        Asset::drawn("sc-ink", Scene, "Ink in water", "4K · 120p · Macro", 5.0, ink),
    ]
}
/// Animated shader backgrounds from remocn (see fx::paper).
pub fn shaders() -> Vec<Asset> {
    crate::fx::paper::backgrounds()
        .iter()
        .map(|b| Asset { shader: Some(b.id.as_str()), ..Asset::base(&format!("sh-{}", b.id.trim_start_matches("shader-")), AssetType::Scene, &b.name, &b.sub, 8.0) })
        .collect()
}
pub const SCENE_DRAWS: [DrawFn; 5] = [coast, drive, aurora, studio, ink];

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
pub fn font_def(family: &str) -> &'static FontDef {
    FONTS.iter().find(|f| f.family == family).unwrap_or(&FONTS[0])
}

fn tp(content: &str, font: &str, size: f64, weight: f64, color: &str, align: TextAlign, anim: TextAnim, boxed: bool, y: f64) -> TextProps {
    TextProps { content: content.into(), font: font.into(), size, weight, color: color.into(), align, anim, boxed, y }
}
pub fn text_presets() -> Vec<Asset> {
    use TextAlign::*;
    use TextAnim::*;
    let t = |id: &str, name: &str, sub: &str, dur: f64, text: TextProps| Asset { text: Some(text), ..Asset::base(id, AssetType::Text, name, sub, dur) };
    vec![
        t("tx-title", "Title", "Centered display", 4.0, tp("Summer, slowly", "Mona Sans", 120.0, 700.0, "#ffffff", Center, Rise, false, 0.0)),
        t("tx-lower", "Lower third", "Name + role", 4.0, tp("Marta Silva\nDirector of photography", "Inter", 44.0, 600.0, "#ffffff", Left, Slide, true, 200.0)),
        t("tx-caption", "Caption", "Subtitle line", 3.0, tp("The light changes every ten minutes here.", "Inter", 36.0, 500.0, "#ffffff", Center, Fade, true, 260.0)),
        t("tx-quote", "Serif quote", "Editorial", 4.0, tp("“We came for the sea,\nwe stayed for the light.”", "Instrument Serif", 76.0, 400.0, "#ffffff", Center, Fade, false, 0.0)),
        t("tx-poster", "Poster", "Condensed caps", 3.0, tp("CHAPTER TWO", "Bebas Neue", 150.0, 400.0, "#ff7a3d", Center, Type, false, 0.0)),
        t("tx-note", "Handwritten", "Script note", 3.0, tp("day three", "Caveat", 90.0, 700.0, "#ffd9a0", Center, Rise, false, -180.0)),
    ]
}
pub const TEXT_COLORS: [&str; 9] = ["#ffffff", "#000000", "#ffd9a0", "#ff7a3d", "#ff5577", "#d44df0", "#6a4cf5", "#0099ff", "#22c55e"];
pub const TEXT_ANIMS: [(TextAnim, &str); 5] =
    [(TextAnim::None, "None"), (TextAnim::Fade, "Fade"), (TextAnim::Rise, "Rise"), (TextAnim::Slide, "Slide"), (TextAnim::Type, "Type")];

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
