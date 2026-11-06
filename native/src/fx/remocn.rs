// Effects ported from remocn (https://github.com/Remocn/remocn-studio, MIT). The fragment shaders
// in assets/remocn are remocn's own, run through the GPU runtime in `gpu.rs`; this file carries
// each component's defaults and the uniform setup its React wrapper did.
//
// remocn sizes things in pixels of a 1280×720 composition; here those are scaled by the frame's
// long side / 1280 so a look holds at any export size.

use super::gpu::{gpu, U};
use super::registry::{color, flag, num, range, select, text, toggle, EffectDef, EffectGroup, Engine, ParamDef};
use crate::model::types::{ParamValue, Params};
use crate::render::canvas::{hex_rgb, rgb, Canvas};
use crate::render::text::{fill_text, Align, Baseline, TextStyle};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tiny_skia::Pixmap;

/// What a shader's uniforms are built from.
pub struct Ctx {
    pub w: f32,
    pub h: f32,
    /// Seconds into the clip, and the clip's length.
    pub t: f32,
    pub dur: f32,
    /// Long side / 1280.
    pub k: f32,
}
impl Ctx {
    fn frame(&self) -> f32 {
        (self.t * 30.0).floor()
    }
    fn aspect(&self) -> f32 {
        self.w / self.h
    }
}

pub struct ShaderFilter {
    pub id: &'static str,
    pub name: &'static str,
    pub src: &'static str,
    pub defaults: fn() -> Vec<(&'static str, ParamValue)>,
    pub params: fn() -> Vec<ParamDef>,
    pub uniforms: fn(&Params, &Ctx) -> Vec<(&'static str, U)>,
}

fn n(v: f64) -> ParamValue {
    ParamValue::Num(v)
}
fn s(v: &str) -> ParamValue {
    ParamValue::Str(v.into())
}
fn b(v: bool) -> ParamValue {
    ParamValue::Bool(v)
}
fn f(p: &Params, k: &str, d: f64) -> f32 {
    num(p, k, d) as f32
}
fn c3(p: &Params, k: &str, d: &str) -> U {
    let [r, g, b] = hex_rgb(text(p, k, d));
    U::V3([r / 255.0, g / 255.0, b / 255.0])
}
fn amount() -> ParamDef {
    range("intensity", "Intensity", 0.0, 2.0, 0.01, "")
}

const CHARSETS: [(&str, &str, &str); 5] = [
    ("classic", "Classic", " .:-=+*#%@"),
    ("dense", "Dense", " .'`^\",:;Il!i><~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$"),
    ("blocks", "Blocks", " ░▒▓█"),
    ("binary", "Binary", " 01"),
    ("dots", "Dots", " ·•●"),
];

pub const FILTERS: [ShaderFilter; 11] = [
    ShaderFilter {
        id: "remocn-ascii",
        name: "ASCII render",
        src: include_str!("../../assets/remocn/ascii-render.frag"),
        defaults: || vec![("glyphSize", n(26.0)), ("charset", s("classic")), ("colored", b(false)), ("ink", s("#9dff9d")), ("intensity", n(1.0))],
        params: || {
            vec![
                range("glyphSize", "Glyph size", 8.0, 64.0, 1.0, "px"),
                select("charset", "Characters", CHARSETS.iter().map(|(k, l, _)| (*k, *l)).collect()),
                toggle("colored", "Source colours"),
                color("ink", "Ink"),
                amount(),
            ]
        },
        uniforms: |p, c| {
            let g = f(p, "glyphSize", 26.0).max(2.0) * c.k;
            vec![
                ("u_cell", U::V2([g * 0.545 / c.w, g / c.h])),
                ("u_glyph_count", U::F(charset(p).chars().count() as f32)),
                ("u_colored", U::F(if flag(p, "colored") { 1.0 } else { 0.0 })),
                ("u_ink", c3(p, "ink", "#9dff9d")),
                ("u_intensity", U::F(f(p, "intensity", 1.0))),
            ]
        },
    },
    ShaderFilter {
        id: "remocn-camera-lens",
        name: "Camera lens",
        src: include_str!("../../assets/remocn/camera-lens.frag"),
        defaults: || vec![("softness", n(0.5)), ("bloom", n(0.5)), ("aberration", n(0.5)), ("vignette", n(0.5)), ("distortion", n(0.05))],
        params: || {
            vec![
                range("softness", "Softness", 0.0, 1.0, 0.01, ""),
                range("bloom", "Bloom", 0.0, 1.0, 0.01, ""),
                range("aberration", "Aberration", 0.0, 1.0, 0.01, ""),
                range("vignette", "Vignette", 0.0, 1.0, 0.01, ""),
                range("distortion", "Barrel", -0.3, 0.3, 0.005, ""),
            ]
        },
        uniforms: |p, c| {
            vec![
                ("u_aspect", U::F(c.aspect())),
                ("u_softness", U::F(f(p, "softness", 0.5))),
                ("u_bloom", U::F(f(p, "bloom", 0.5))),
                ("u_aberration", U::F(f(p, "aberration", 0.5))),
                ("u_vignette", U::F(f(p, "vignette", 0.5))),
                ("u_distortion", U::F(f(p, "distortion", 0.05))),
            ]
        },
    },
    ShaderFilter {
        id: "remocn-crt",
        name: "CRT screen",
        src: include_str!("../../assets/remocn/crt-screen.frag"),
        defaults: || vec![("curvature", n(1.0)), ("scanlines", n(1.0)), ("maskScale", n(2.0)), ("vignette", n(1.0))],
        params: || {
            vec![
                range("curvature", "Curvature", 0.0, 2.0, 0.01, ""),
                range("scanlines", "Scanlines", 0.0, 2.0, 0.01, ""),
                range("maskScale", "Phosphor size", 1.0, 8.0, 0.5, "px"),
                range("vignette", "Vignette", 0.0, 2.0, 0.01, ""),
            ]
        },
        uniforms: |p, c| {
            vec![
                ("u_frame", U::F(c.frame())),
                ("u_curvature", U::F(f(p, "curvature", 1.0))),
                ("u_scanlines", U::F(f(p, "scanlines", 1.0))),
                ("u_mask_scale", U::F((f(p, "maskScale", 2.0) * c.k).max(1.0))),
                ("u_vignette", U::F(f(p, "vignette", 1.0))),
            ]
        },
    },
    ShaderFilter {
        id: "remocn-halftone",
        name: "Halftone print",
        src: include_str!("../../assets/remocn/halftone-print.frag"),
        defaults: || vec![("dotSize", n(10.0)), ("angle", n(0.0)), ("misregistration", n(1.2)), ("paperTint", s("#f4efe4"))],
        params: || {
            vec![
                range("dotSize", "Dot size", 3.0, 40.0, 0.5, "px"),
                range("angle", "Angle", -90.0, 90.0, 1.0, "°"),
                range("misregistration", "Misregistration", 0.0, 8.0, 0.1, "px"),
                color("paperTint", "Paper"),
            ]
        },
        uniforms: |p, c| {
            vec![
                ("u_aspect", U::F(c.aspect())),
                ("u_cell", U::F(f(p, "dotSize", 10.0).max(1.0) * c.k / c.h)),
                ("u_angle", U::F(f(p, "angle", 0.0).to_radians())),
                ("u_shift", U::F(f(p, "misregistration", 1.2) * c.k / c.h)),
                ("u_paper", c3(p, "paperTint", "#f4efe4")),
            ]
        },
    },
    ShaderFilter {
        id: "remocn-hologram",
        name: "Hologram",
        src: include_str!("../../assets/remocn/hologram.frag"),
        defaults: || vec![("tint", s("#63e8ff")), ("glow", n(1.0)), ("ghost", n(1.0)), ("flicker", n(1.0)), ("intensity", n(1.0))],
        params: || {
            vec![
                color("tint", "Tint"),
                range("glow", "Glow", 0.0, 2.0, 0.01, ""),
                range("ghost", "Ghosting", 0.0, 2.0, 0.01, ""),
                range("flicker", "Flicker", 0.0, 2.0, 0.01, ""),
                amount(),
            ]
        },
        uniforms: |p, c| {
            vec![
                ("u_aspect", U::F(c.aspect())),
                ("u_frame", U::F(c.frame())),
                ("u_tint", c3(p, "tint", "#63e8ff")),
                ("u_glow", U::F(f(p, "glow", 1.0))),
                ("u_ghost", U::F(f(p, "ghost", 1.0))),
                ("u_flicker", U::F(f(p, "flicker", 1.0))),
                ("u_intensity", U::F(f(p, "intensity", 1.0))),
            ]
        },
    },
    ShaderFilter {
        id: "remocn-pixelate-region",
        name: "Pixelate region",
        src: include_str!("../../assets/remocn/pixelate-region.frag"),
        defaults: || vec![("cellSize", n(24.0)), ("x", n(35.0)), ("y", n(20.0)), ("width", n(30.0)), ("height", n(45.0))],
        params: || {
            vec![
                range("cellSize", "Cell size", 4.0, 80.0, 1.0, "px"),
                range("x", "Left", 0.0, 100.0, 0.5, "%"),
                range("y", "Top", 0.0, 100.0, 0.5, "%"),
                range("width", "Width", 1.0, 100.0, 0.5, "%"),
                range("height", "Height", 1.0, 100.0, 0.5, "%"),
            ]
        },
        uniforms: |p, c| {
            let cell = f(p, "cellSize", 24.0).max(1.0) * c.k;
            let r = [f(p, "x", 35.0) / 100.0, f(p, "y", 20.0) / 100.0, f(p, "width", 30.0) / 100.0, f(p, "height", 45.0) / 100.0];
            vec![("u_regions", U::V4s(vec![r])), ("u_region_count", U::I(1)), ("u_cell", U::V2([cell / c.w, cell / c.h]))]
        },
    },
    ShaderFilter {
        id: "remocn-security-cam",
        name: "Security cam",
        src: include_str!("../../assets/remocn/security-cam.frag"),
        defaults: || vec![("compression", n(0.7)), ("blockSize", n(10.0)), ("noise", n(0.6)), ("intensity", n(1.0))],
        params: || {
            vec![
                range("compression", "Compression", 0.0, 1.0, 0.01, ""),
                range("blockSize", "Block size", 2.0, 40.0, 1.0, "px"),
                range("noise", "Noise", 0.0, 1.0, 0.01, ""),
                amount(),
            ]
        },
        uniforms: |p, c| {
            let bs = f(p, "blockSize", 10.0).max(1.0) * c.k;
            vec![
                ("u_frame", U::F(c.frame())),
                ("u_cell", U::V2([bs / c.w, bs / c.h])),
                ("u_compression", U::F(f(p, "compression", 0.7))),
                ("u_noise", U::F(f(p, "noise", 0.6))),
                ("u_intensity", U::F(f(p, "intensity", 1.0))),
            ]
        },
    },
    ShaderFilter {
        id: "remocn-sustained-glitch",
        name: "Sustained glitch",
        src: include_str!("../../assets/remocn/sustained-glitch.frag"),
        defaults: || vec![("intensity", n(1.0)), ("frequency", n(1.0)), ("slices", n(24.0)), ("seed", n(1.0))],
        params: || {
            vec![
                amount(),
                range("frequency", "Frequency", 0.0, 3.0, 0.01, "×"),
                range("slices", "Slices", 4.0, 80.0, 1.0, ""),
                range("seed", "Seed", 0.0, 100.0, 1.0, ""),
            ]
        },
        uniforms: |p, c| {
            vec![
                ("u_frame", U::F(c.frame())),
                ("u_intensity", U::F(f(p, "intensity", 1.0))),
                ("u_frequency", U::F(f(p, "frequency", 1.0))),
                ("u_slices", U::F(f(p, "slices", 24.0))),
                ("u_seed", U::F(f(p, "seed", 1.0))),
            ]
        },
    },
    ShaderFilter {
        id: "remocn-underwater",
        name: "Underwater ripple",
        src: include_str!("../../assets/remocn/underwater-ripple.frag"),
        defaults: || vec![("amplitude", n(7.0)), ("scale", n(1.0)), ("speed", n(2.0)), ("dispersion", n(1.0))],
        params: || {
            vec![
                range("amplitude", "Amplitude", 0.0, 30.0, 0.5, "px"),
                range("scale", "Scale", 0.2, 4.0, 0.05, "×"),
                range("speed", "Speed", 0.0, 6.0, 0.05, "×"),
                range("dispersion", "Dispersion", 0.0, 3.0, 0.01, ""),
            ]
        },
        uniforms: |p, c| {
            // remocn loops the ripple once over its 3 s preview.
            let phase = (c.t / 3.0) * f(p, "speed", 2.0) * std::f32::consts::TAU;
            vec![
                ("u_aspect", U::F(c.aspect())),
                ("u_phase", U::F(phase)),
                ("u_amplitude", U::F(f(p, "amplitude", 7.0) * c.k / c.h)),
                ("u_scale", U::F(f(p, "scale", 1.0))),
                ("u_dispersion", U::F(f(p, "dispersion", 1.0))),
            ]
        },
    },
    ShaderFilter {
        id: "remocn-tv-off",
        name: "TV power off",
        src: include_str!("../../assets/remocn/tv-power-off.frag"),
        defaults: || vec![("length", n(0.6)), ("at", s("end")), ("gain", n(1.0)), ("afterglow", n(1.0)), ("phosphor", s("#d8ecff"))],
        params: || {
            vec![
                range("length", "Length", 0.1, 3.0, 0.05, "s"),
                select("at", "Plays", vec![("end", "Power off at the end"), ("start", "Power on at the start")]),
                range("gain", "Brightness", 0.0, 2.0, 0.01, ""),
                range("afterglow", "Afterglow", 0.0, 2.0, 0.01, ""),
                color("phosphor", "Phosphor"),
            ]
        },
        uniforms: |p, c| {
            let len = f(p, "length", 0.6).max(0.02);
            let progress = if text(p, "at", "end") == "start" { 1.0 - (c.t / len).clamp(0.0, 1.0) } else { ((c.t - (c.dur - len)) / len).clamp(0.0, 1.0) };
            vec![
                ("u_resolution", U::V2([c.w, c.h])),
                ("u_aspect", U::F(c.aspect())),
                ("u_progress", U::F(progress)),
                ("u_gain", U::F(f(p, "gain", 1.0))),
                ("u_afterglow", U::F(f(p, "afterglow", 1.0))),
                ("u_phosphor", c3(p, "phosphor", "#d8ecff")),
            ]
        },
    },
    ShaderFilter {
        id: "remocn-vhs",
        name: "VHS tape",
        src: include_str!("../../assets/remocn/vhs-filter.frag"),
        defaults: || vec![("bleed", n(1.0)), ("wobble", n(1.0)), ("noise", n(1.0)), ("intensity", n(1.0))],
        params: || {
            vec![
                range("bleed", "Chroma bleed", 0.0, 3.0, 0.01, ""),
                range("wobble", "Wobble", 0.0, 3.0, 0.01, ""),
                range("noise", "Noise", 0.0, 3.0, 0.01, ""),
                amount(),
            ]
        },
        uniforms: |p, c| {
            vec![
                ("u_frame", U::F(c.frame())),
                ("u_time", U::F(c.t)),
                ("u_bleed", U::F(f(p, "bleed", 1.0))),
                ("u_wobble", U::F(f(p, "wobble", 1.0))),
                ("u_noise", U::F(f(p, "noise", 1.0))),
                ("u_intensity", U::F(f(p, "intensity", 1.0))),
            ]
        },
    },
];

fn charset(p: &Params) -> &'static str {
    let id = text(p, "charset", "classic");
    CHARSETS.iter().find(|(k, _, _)| *k == id).unwrap_or(&CHARSETS[0]).2
}

/// remocn's glyph atlas: one 24×44 cell per character, white on black, centred.
fn atlas(chars: &'static str) -> Arc<Pixmap> {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, Arc<Pixmap>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(a) = cache.lock().get(chars) {
        return a.clone();
    }
    let n = chars.chars().count().max(1) as u32;
    let mut c = Canvas::new(24 * n, 44);
    c.fill_all(rgb(0, 0, 0));
    let st = TextStyle { family: "JetBrains Mono", weight: 400.0, size: (44.0f32 * 0.74).round(), spacing: 0.0 };
    for (i, ch) in chars.chars().enumerate() {
        fill_text(&mut c, &ch.to_string(), i as f32 * 24.0 + 12.0, 22.0, &st, Align::Center, Baseline::Middle, rgb(255, 255, 255));
    }
    let a = Arc::new(c.into_pixmap());
    cache.lock().insert(chars, a.clone());
    a
}

pub fn filter(id: &str) -> Option<&'static ShaderFilter> {
    FILTERS.iter().find(|f| f.id == id)
}

pub fn effect_defs() -> Vec<EffectDef> {
    FILTERS
        .iter()
        .map(|f| EffectDef {
            id: f.id,
            name: f.name,
            group: EffectGroup::Shader,
            engine: Engine::Shader,
            css: None,
            defaults: super::registry::params(&(f.defaults)()),
            params: (f.params)(),
        })
        .collect()
}

/// Run a remocn filter over `src`. Without a GPU the frame passes through untouched.
pub fn apply(src: &Pixmap, id: &str, p: &Params, t: f32, dur: f32) -> Pixmap {
    let Some(fx) = filter(id) else { return src.clone() };
    let Some(g) = gpu() else { return src.clone() };
    let (w, h) = (src.width(), src.height());
    let ctx = Ctx { w: w as f32, h: h as f32, t, dur, k: w.max(h) as f32 / 1280.0 };
    let uniforms = (fx.uniforms)(p, &ctx);
    let consts = HashMap::new();
    let out = if fx.id == "remocn-ascii" {
        let a = atlas(charset(p));
        g.run(fx.id, fx.src, &consts, w, h, &[("u_scene", src), ("u_atlas", &a)], &uniforms)
    } else {
        g.run(fx.id, fx.src, &consts, w, h, &[("u_scene", src)], &uniforms)
    };
    out.unwrap_or_else(|| src.clone())
}

// ---------- Transitions ----------
// remocn's scene shaders get `u_from` (the outgoing scene, shown at progress 0) and `u_to`.

pub struct ShaderTransition {
    pub id: &'static str,
    pub src: &'static str,
    pub uniforms: fn(&Ctx) -> Vec<(&'static str, U)>,
}

fn rgb01(h: &str) -> U {
    let [r, g, b] = hex_rgb(h);
    U::V3([r / 255.0, g / 255.0, b / 255.0])
}
fn grid_wave(c: &Ctx, dir: [f32; 2], radial: bool) -> Vec<(&'static str, U)> {
    let size = 90.0 * c.k;
    vec![
        ("u_aspect", U::F(c.aspect())),
        ("u_grid", U::V2([(c.w / size).round().max(2.0), (c.h / size).round().max(2.0)])),
        ("u_wave", U::F(0.16)),
        ("u_lift", U::F(2.0 * 0.35)),
        ("u_gap", U::F(0.12)),
        ("u_tint", rgb01("#8fb4ff")),
        ("u_dir", U::V2(dir)),
        ("u_radial", U::F(if radial { 1.0 } else { 0.0 })),
    ]
}
fn particles(c: &Ctx, dir: [f32; 2], radial: bool) -> Vec<(&'static str, U)> {
    vec![
        ("u_aspect", U::F(c.aspect())),
        ("u_cells", U::F(c.h / (4.0 * c.k))),
        ("u_scatter", U::F(0.08)),
        ("u_shimmer", U::F(0.6)),
        ("u_aberration", U::F(0.5)),
        ("u_stagger", U::F(0.55)),
        ("u_dir", U::V2(dir)),
        ("u_radial", U::F(if radial { 1.0 } else { 0.0 })),
    ]
}

const DISPLACEMENT: &str = include_str!("../../assets/remocn/displacement.frag");
const GRID_WAVE: &str = include_str!("../../assets/remocn/grid-wave.frag");
const PARTICLES: &str = include_str!("../../assets/remocn/particle-dissolve.frag");

pub const TRANSITIONS: [ShaderTransition; 8] = [
    ShaderTransition {
        id: "remocn-displacement",
        src: DISPLACEMENT,
        uniforms: |c| {
            let across = 60.0f32;
            vec![
                ("u_aspect", U::F(c.aspect())),
                ("u_cells", U::V2([across, (across * c.h / c.w).max(2.0)])),
                ("u_shift", U::F(1.0)),
                ("u_aberration", U::F(1.0)),
                ("u_grain", U::F(0.5)),
                ("u_stagger", U::F(0.45)),
            ]
        },
    },
    ShaderTransition {
        id: "remocn-ember-burn",
        src: include_str!("../../assets/remocn/ember-burn.frag"),
        uniforms: |c| {
            vec![
                ("u_aspect", U::F(c.aspect())),
                ("u_patches", U::F(5.0)),
                ("u_softness", U::F(0.07)),
                ("u_content", U::F(0.6)),
                ("u_heat", U::F(0.5)),
                ("u_glow", rgb01("#ff7a2f")),
                ("u_embers", U::F(0.5)),
            ]
        },
    },
    ShaderTransition {
        id: "remocn-glitch-cut",
        src: include_str!("../../assets/remocn/glitch-cut.frag"),
        uniforms: |_| vec![("u_intensity", U::F(1.0)), ("u_slices", U::F(24.0)), ("u_rgb_split", U::F(1.0)), ("u_block_noise", U::F(0.6))],
    },
    ShaderTransition { id: "remocn-grid-wave", src: GRID_WAVE, uniforms: |c| grid_wave(c, [0.0, 0.0], true) },
    ShaderTransition { id: "remocn-grid-sweep", src: GRID_WAVE, uniforms: |c| grid_wave(c, [1.0, 0.0], false) },
    ShaderTransition { id: "remocn-particles", src: PARTICLES, uniforms: |c| particles(c, [0.0, 0.0], true) },
    ShaderTransition { id: "remocn-particles-up", src: PARTICLES, uniforms: |c| particles(c, [0.0, -1.0], false) },
    ShaderTransition {
        id: "remocn-shatter",
        src: DISPLACEMENT,
        uniforms: |c| {
            vec![
                ("u_aspect", U::F(c.aspect())),
                ("u_cells", U::V2([14.0, (14.0 * 0.5 * c.h / c.w).max(2.0)])),
                ("u_shift", U::F(1.6)),
                ("u_aberration", U::F(1.4)),
                ("u_grain", U::F(0.3)),
                ("u_stagger", U::F(0.7)),
            ]
        },
    },
];

/// Draw a remocn shader transition onto `c` (already sized to the frame).
pub fn transition(c: &mut Canvas, id: &str, a: Option<&Pixmap>, b: &Pixmap, p: f32) {
    let Some(tr) = TRANSITIONS.iter().find(|t| t.id == id) else { return };
    let (w, h) = (b.width(), b.height());
    let blank;
    let a = match a {
        Some(a) => a,
        None => {
            blank = tiny_skia::Pixmap::new(w, h).unwrap();
            &blank
        }
    };
    let ctx = Ctx { w: w as f32, h: h as f32, t: 0.0, dur: 0.0, k: w.max(h) as f32 / 1280.0 };
    let mut uniforms = (tr.uniforms)(&ctx);
    uniforms.push(("u_progress", U::F(p)));
    // Each shader is compiled once; variants share a program.
    let key = format!("tr:{:p}", tr.src.as_ptr());
    let out = gpu().and_then(|g| g.run(&key, tr.src, &HashMap::new(), w, h, &[("u_from", a), ("u_to", b)], &uniforms));
    match out {
        Some(out) => c.draw_image(&out, 0.0, 0.0),
        // No GPU: a plain dissolve.
        None => {
            c.draw_image(a, 0.0, 0.0);
            c.save();
            c.set_alpha(p);
            c.draw_image(b, 0.0, 0.0);
            c.restore();
        }
    }
}
