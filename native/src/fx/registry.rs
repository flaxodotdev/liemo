// Effect catalogue. Each library entry is a preset of an engine: the engine does the pixel work,
// the preset picks its starting parameters. Users can change every parameter in the inspector.

use crate::model::types::{ClipEffect, ParamValue, Params};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub enum ParamKind {
    Range { min: f64, max: f64, step: f64, unit: &'static str },
    Select(Vec<(&'static str, &'static str)>),
    Color,
    Toggle,
}
#[derive(Clone, Debug)]
pub struct ParamDef {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Css, Ascii, Dots, Pixelate, Dither, RgbSplit, Glitch, Posterize, Duotone, Vhs, Crt, Shake, Mirror, Edges, Emboss,
    GradientMap, ZoomBlur, Wave, TiltShift, Glow, Threshold, Solarize, Look, Shader,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectGroup {
    Filters,
    Shader,
    Stylize,
    Retro,
    Distort,
    Color,
}
pub const EFFECT_GROUPS: [(EffectGroup, &str); 6] =
    [(EffectGroup::Filters, "Instagram filters"), (EffectGroup::Shader, "Shader filters"), (EffectGroup::Stylize, "Stylize"), (EffectGroup::Retro, "Retro"), (EffectGroup::Distort, "Distort & camera"), (EffectGroup::Color, "Color & look")];

#[derive(Clone)]
pub struct EffectDef {
    pub id: &'static str,
    pub name: &'static str,
    pub group: EffectGroup,
    pub engine: Engine,
    /// For colour effects: the CSS filter string at a given strength (0–1).
    pub css: Option<fn(f64) -> String>,
    pub defaults: Params,
    pub params: Vec<ParamDef>,
}

// ---------- Glyph sets for the ASCII engine ----------
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Circle,
    Square,
    Diamond,
    Cross,
    Hatch,
    Ring,
}
pub enum GlyphSet {
    Chars { label: &'static str, chars: &'static str, emoji: bool, rain: bool },
    Shape { label: &'static str, shape: Shape, levels: usize },
}
impl GlyphSet {
    pub fn label(&self) -> &'static str {
        match self {
            GlyphSet::Chars { label, .. } | GlyphSet::Shape { label, .. } => label,
        }
    }
}
pub const GLYPH_SETS: [(&str, GlyphSet); 13] = [
    ("classic", GlyphSet::Chars { label: "Classic ASCII", chars: " .:-=+*#%@", emoji: false, rain: false }),
    ("detailed", GlyphSet::Chars { label: "Detailed ASCII", chars: " .'`^\",:;Il!i~+_-?]}1)|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@", emoji: false, rain: false }),
    ("blocks", GlyphSet::Chars { label: "Blocks", chars: " ░▒▓█", emoji: false, rain: false }),
    ("braille", GlyphSet::Chars { label: "Braille", chars: " ⠁⠂⠃⠉⠋⠛⠟⠿⡿⣿", emoji: false, rain: false }),
    ("binary", GlyphSet::Chars { label: "Binary", chars: " .01", emoji: false, rain: false }),
    ("rain", GlyphSet::Chars { label: "Katakana rain", chars: "ｦｱｳｴｵｶｷｹｺｻｼｽｾｿﾀﾂﾃﾅﾆﾇﾈﾊﾋﾎﾏﾐﾑﾒﾓﾔﾕﾗﾘﾜ0123456789", emoji: false, rain: true }),
    ("moon", GlyphSet::Chars { label: "Moon phases", chars: "🌑🌘🌗🌖🌕", emoji: true, rain: false }),
    ("dots", GlyphSet::Shape { label: "Halftone dots", shape: Shape::Circle, levels: 12 }),
    ("squares", GlyphSet::Shape { label: "Pixel squares", shape: Shape::Square, levels: 12 }),
    ("diamonds", GlyphSet::Shape { label: "Diamonds", shape: Shape::Diamond, levels: 12 }),
    ("crosses", GlyphSet::Shape { label: "Crosses", shape: Shape::Cross, levels: 10 }),
    ("hatch", GlyphSet::Shape { label: "Hatch lines", shape: Shape::Hatch, levels: 10 }),
    ("rings", GlyphSet::Shape { label: "Rings", shape: Shape::Ring, levels: 10 }),
];
pub fn glyph_set(id: &str) -> &'static GlyphSet {
    &GLYPH_SETS.iter().find(|(k, _)| *k == id).unwrap_or(&GLYPH_SETS[0]).1
}

pub struct Gradient {
    pub label: &'static str,
    pub stops: &'static [&'static str],
    pub noise: bool,
}
pub const GRADIENT_MAPS: [(&str, Gradient); 5] = [
    ("thermal", Gradient { label: "Thermal", stops: &["#07001a", "#2b0a5c", "#a1158d", "#ff4d2e", "#ffd23f", "#ffffff"], noise: false }),
    ("night", Gradient { label: "Night vision", stops: &["#000800", "#0b3d0b", "#2fbf2f", "#c8ffc8"], noise: true }),
    ("infrared", Gradient { label: "Infrared", stops: &["#12001f", "#5c1f73", "#ff3d8b", "#ffd1e6", "#ffffff"], noise: false }),
    ("ocean", Gradient { label: "Deep sea", stops: &["#000814", "#003566", "#0099ff", "#90e0ef"], noise: false }),
    ("ember", Gradient { label: "Ember", stops: &["#0d0000", "#5a0b0b", "#ff5a1f", "#ffd9a0"], noise: false }),
];
pub fn gradient_map(id: &str) -> &'static Gradient {
    &GRADIENT_MAPS.iter().find(|(k, _)| *k == id).unwrap_or(&GRADIENT_MAPS[0]).1
}

pub const PALETTES: [(&str, &str, &[&str]); 5] = [
    ("1bit", "1-bit", &["#000000", "#ffffff"]),
    ("gameboy", "Game Boy", &["#0f380f", "#306230", "#8bac0f", "#9bbc0f"]),
    ("cga", "CGA", &["#000000", "#55ffff", "#ff55ff", "#ffffff"]),
    ("pico8", "PICO-8", &["#000000", "#1d2b53", "#7e2553", "#008751", "#ab5236", "#5f574f", "#c2c3c7", "#fff1e8", "#ff004d", "#ffa300", "#ffec27", "#00e436", "#29adff", "#83769c", "#ff77a8", "#ffccaa"]),
    ("sunset", "Sunset", &["#1b0f2e", "#6a4cf5", "#d44df0", "#ff5577", "#ff7a3d", "#ffd9a0"]),
];
pub fn palette(id: &str) -> &'static [&'static str] {
    PALETTES.iter().find(|(k, _, _)| *k == id).unwrap_or(&PALETTES[0]).2
}

pub const DOT_PALETTES: [(&str, &str, &[&str]); 4] = [
    ("ice", "Ice blue", &["#0b1d4a", "#1d4ed8", "#3b82f6", "#93c5fd", "#ffffff"]),
    ("amber", "Amber", &["#2a1200", "#b45309", "#f59e0b", "#fde68a", "#fffbeb"]),
    ("phosphor", "Phosphor green", &["#021a06", "#15803d", "#22c55e", "#bbf7d0", "#f0fff4"]),
    ("sunset", "Sunset", &["#2b0a3d", "#6a4cf5", "#d44df0", "#ff7a3d", "#ffd9a0"]),
];
pub fn dot_palette(id: &str) -> &'static [&'static str] {
    DOT_PALETTES.iter().find(|(k, _, _)| *k == id).unwrap_or(&DOT_PALETTES[0]).2
}

// ---------- Parameter helpers ----------
pub(crate) fn range(key: &'static str, label: &'static str, min: f64, max: f64, step: f64, unit: &'static str) -> ParamDef {
    ParamDef { key, label, kind: ParamKind::Range { min, max, step, unit } }
}
pub(crate) fn select(key: &'static str, label: &'static str, options: Vec<(&'static str, &'static str)>) -> ParamDef {
    ParamDef { key, label, kind: ParamKind::Select(options) }
}
pub(crate) fn color(key: &'static str, label: &'static str) -> ParamDef {
    ParamDef { key, label, kind: ParamKind::Color }
}
pub(crate) fn toggle(key: &'static str, label: &'static str) -> ParamDef {
    ParamDef { key, label, kind: ParamKind::Toggle }
}

pub fn n(v: f64) -> ParamValue {
    ParamValue::Num(v)
}
pub fn s(v: &str) -> ParamValue {
    ParamValue::Str(v.into())
}
pub fn b(v: bool) -> ParamValue {
    ParamValue::Bool(v)
}
pub(crate) fn params(list: &[(&str, ParamValue)]) -> Params {
    list.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}
fn merge(base: &[(&str, ParamValue)], over: &[(&str, ParamValue)]) -> Params {
    let mut p = params(base);
    for (k, v) in over {
        p.insert(k.to_string(), v.clone());
    }
    p
}

fn ascii_params() -> Vec<ParamDef> {
    vec![
        select("set", "Glyphs", GLYPH_SETS.iter().map(|(k, v)| (*k, v.label())).collect()),
        range("cell", "Cell size", 4.0, 48.0, 1.0, "px"),
        select("color", "Colour", vec![("source", "Original"), ("mono", "White"), ("tint", "Tint")]),
        color("tint", "Tint"),
        select("bg", "Background", vec![("black", "Black"), ("dim", "Dimmed video"), ("none", "None")]),
        range("contrast", "Contrast", 0.5, 3.0, 0.05, "×"),
        toggle("invert", "Invert"),
    ]
}
fn dot_params() -> Vec<ParamDef> {
    let mut colors = vec![("white", "White"), ("source", "Original")];
    colors.extend(DOT_PALETTES.iter().map(|(k, l, _)| (*k, *l)));
    vec![
        select("pattern", "Pattern", vec![("halftone", "Rounded halftone"), ("braille", "Braille dot grid")]),
        range("cell", "Cell size", 4.0, 48.0, 1.0, "px"),
        range("size", "Dot size", 0.3, 1.0, 0.01, ""),
        select("color", "Colour", colors),
        select("bg", "Background", vec![("black", "Black"), ("midnight", "Midnight"), ("none", "None")]),
        range("contrast", "Contrast", 0.5, 3.0, 0.05, "×"),
        range("glow", "Glow", 0.0, 1.0, 0.01, ""),
        toggle("invert", "Invert"),
    ]
}
fn ascii(id: &'static str, name: &'static str, over: &[(&str, ParamValue)]) -> EffectDef {
    let base = [("set", s("classic")), ("cell", n(12.0)), ("color", s("source")), ("tint", s("#22c55e")), ("bg", s("black")), ("contrast", n(1.3)), ("invert", b(false))];
    EffectDef { id, name, group: EffectGroup::Stylize, engine: Engine::Ascii, css: None, defaults: merge(&base, over), params: ascii_params() }
}
fn dots(id: &'static str, name: &'static str, over: &[(&str, ParamValue)]) -> EffectDef {
    let base = [("pattern", s("halftone")), ("cell", n(14.0)), ("size", n(0.92)), ("color", s("white")), ("bg", s("black")), ("contrast", n(1.15)), ("glow", n(0.0)), ("invert", b(false))];
    EffectDef { id, name, group: EffectGroup::Stylize, engine: Engine::Dots, css: None, defaults: merge(&base, over), params: dot_params() }
}
fn def(id: &'static str, name: &'static str, group: EffectGroup, engine: Engine, defaults: &[(&str, ParamValue)], p: Vec<ParamDef>) -> EffectDef {
    EffectDef { id, name, group, engine, css: None, defaults: params(defaults), params: p }
}
fn css(id: &'static str, name: &'static str, f: fn(f64) -> String) -> EffectDef {
    EffectDef { id, name, group: EffectGroup::Color, engine: Engine::Css, css: Some(f), defaults: Params::new(), params: vec![] }
}

pub fn effects() -> &'static Vec<EffectDef> {
    static E: OnceLock<Vec<EffectDef>> = OnceLock::new();
    E.get_or_init(|| {
        use EffectGroup::*;
        use Engine as En;
        let dither_params = vec![
            select("palette", "Palette", PALETTES.iter().map(|(k, l, _)| (*k, *l)).collect()),
            range("pixel", "Pixel size", 1.0, 12.0, 1.0, "px"),
            range("spread", "Spread", 0.0, 2.0, 0.05, "×"),
        ];
        let mirror_params = vec![
            select("mode", "Mode", vec![("left", "Left onto right"), ("right", "Right onto left"), ("top", "Top onto bottom"), ("quad", "Four-way"), ("kaleido", "Kaleidoscope")]),
            range("segments", "Segments", 3.0, 16.0, 1.0, ""),
            range("spin", "Spin", -1.0, 1.0, 0.01, "/s"),
        ];
        let map_params = || vec![select("map", "Look", GRADIENT_MAPS.iter().map(|(k, v)| (*k, v.label)).collect()), range("contrast", "Contrast", 0.5, 2.5, 0.05, "×")];
        let edge_params = || vec![select("mode", "Style", vec![("sketch", "Pencil sketch"), ("neon", "Neon edges"), ("white", "White lines")]), range("strength", "Strength", 0.5, 4.0, 0.05, "×")];
        let wave_params = || {
            vec![
                range("amplitude", "Amplitude", 0.0, 80.0, 1.0, "px"),
                range("frequency", "Frequency", 0.5, 12.0, 0.5, ""),
                range("speed", "Speed", 0.0, 10.0, 0.1, ""),
                select("direction", "Direction", vec![("horizontal", "Horizontal"), ("vertical", "Vertical")]),
            ]
        };
        let mut list = vec![
            // Stylize
            ascii("ascii", "ASCII", &[("set", s("classic")), ("color", s("mono"))]),
            ascii("ascii-color", "ASCII colour", &[("set", s("detailed")), ("cell", n(9.0))]),
            ascii("blocks", "Blocks", &[("set", s("blocks")), ("cell", n(10.0))]),
            dots("rounded-dots", "Rounded dots", &[]),
            dots("braille", "Braille", &[("pattern", s("braille")), ("cell", n(9.0)), ("size", n(0.86)), ("color", s("ice")), ("bg", s("midnight")), ("contrast", n(1.35)), ("glow", n(0.45))]),
            ascii("rain", "Code rain", &[("set", s("rain")), ("cell", n(14.0)), ("color", s("tint")), ("tint", s("#22c55e")), ("contrast", n(1.6))]),
            ascii("halftone", "Halftone", &[("set", s("dots")), ("cell", n(10.0)), ("contrast", n(1.1))]),
            ascii("pixel-squares", "Pixel squares", &[("set", s("squares")), ("cell", n(14.0)), ("contrast", n(1.0))]),
            ascii("hatch", "Hatching", &[("set", s("hatch")), ("cell", n(10.0)), ("color", s("tint")), ("tint", s("#ffffff")), ("bg", s("black"))]),
            ascii("moon", "Moon phases", &[("set", s("moon")), ("cell", n(22.0)), ("contrast", n(1.4))]),
            def("pixelate", "Pixelate", Stylize, En::Pixelate, &[("size", n(16.0))], vec![range("size", "Pixel size", 2.0, 80.0, 1.0, "px")]),
            def("dither", "Dither", Retro, En::Dither, &[("palette", s("1bit")), ("pixel", n(3.0)), ("spread", n(1.0))], dither_params.clone()),
            def("gameboy", "Game Boy", Retro, En::Dither, &[("palette", s("gameboy")), ("pixel", n(5.0)), ("spread", n(0.8))], dither_params),
            def("rgbsplit", "RGB split", Retro, En::RgbSplit, &[("offset", n(10.0)), ("jitter", b(false))], vec![range("offset", "Offset", 0.0, 60.0, 1.0, "px"), toggle("jitter", "Jitter")]),
            def("glitch", "Glitch", Retro, En::Glitch, &[("intensity", n(0.5)), ("slices", n(14.0)), ("speed", n(12.0))],
                vec![range("intensity", "Intensity", 0.0, 1.0, 0.01, ""), range("slices", "Slices", 2.0, 40.0, 1.0, ""), range("speed", "Speed", 1.0, 30.0, 1.0, "/s")]),
            def("posterize", "Posterize", Stylize, En::Posterize, &[("levels", n(4.0))], vec![range("levels", "Levels", 2.0, 12.0, 1.0, "")]),
            def("duotone", "Duotone", Stylize, En::Duotone, &[("dark", s("#1b0f2e")), ("light", s("#ff7a3d")), ("contrast", n(1.1))],
                vec![color("dark", "Shadows"), color("light", "Highlights"), range("contrast", "Contrast", 0.5, 2.5, 0.05, "×")]),
            // Colour & look
            css("bw", "Black & white", |a| format!("grayscale({a})")),
            css("sepia", "Sepia", |a| format!("sepia({a})")),
            css("warm", "Warm", |a| format!("sepia({}) saturate({}) hue-rotate({}deg)", 0.35 * a, 1.0 + 0.4 * a, -12.0 * a)),
            css("cool", "Cool", |a| format!("saturate({}) hue-rotate({}deg) brightness({})", 1.0 - 0.2 * a, 18.0 * a, 1.0 + 0.05 * a)),
            css("vivid", "Vivid", |a| format!("saturate({}) contrast({})", 1.0 + a, 1.0 + 0.15 * a)),
            css("faded", "Faded film", |a| format!("contrast({}) brightness({}) saturate({})", 1.0 - 0.3 * a, 1.0 + 0.1 * a, 1.0 - 0.3 * a)),
            css("noir", "Noir", |a| format!("grayscale({a}) contrast({}) brightness({})", 1.0 + 0.6 * a, 1.0 - 0.1 * a)),
            css("blur", "Soft blur", |a| format!("blur({}px)", a * 8.0)),
            css("dream", "Dream", |a| format!("blur({}px) brightness({}) saturate({})", a * 2.0, 1.0 + 0.2 * a, 1.0 + 0.3 * a)),
            css("hue", "Hue shift", |a| format!("hue-rotate({}deg)", a * 180.0)),
            css("invert", "Invert", |a| format!("invert({a})")),
            // More stylize
            def("sketch", "Pencil sketch", Stylize, En::Edges, &[("mode", s("sketch")), ("strength", n(1.6))], edge_params()),
            def("neon-edges", "Neon edges", Stylize, En::Edges, &[("mode", s("neon")), ("strength", n(2.2))], edge_params()),
            def("emboss", "Emboss", Stylize, En::Emboss, &[("strength", n(1.4))], vec![range("strength", "Strength", 0.5, 4.0, 0.05, "×")]),
            def("thermal", "Thermal", Stylize, En::GradientMap, &[("map", s("thermal")), ("contrast", n(1.2))], map_params()),
            def("night-vision", "Night vision", Stylize, En::GradientMap, &[("map", s("night")), ("contrast", n(1.4))], map_params()),
            def("infrared", "Infrared", Stylize, En::GradientMap, &[("map", s("infrared")), ("contrast", n(1.1))], map_params()),
            def("threshold", "Threshold", Stylize, En::Threshold, &[("level", n(0.5)), ("dark", s("#000000")), ("light", s("#ffffff"))],
                vec![range("level", "Level", 0.05, 0.95, 0.01, ""), color("dark", "Dark"), color("light", "Light")]),
            def("solarize", "Solarize", Stylize, En::Solarize, &[("threshold", n(0.5))], vec![range("threshold", "Threshold", 0.0, 1.0, 0.01, "")]),
            def("glow", "Glow", Stylize, En::Glow, &[("strength", n(0.7)), ("radius", n(18.0))], vec![range("strength", "Strength", 0.0, 1.5, 0.01, ""), range("radius", "Radius", 2.0, 60.0, 1.0, "px")]),
            // Retro
            def("vhs", "VHS", Retro, En::Vhs, &[("intensity", n(0.6))], vec![range("intensity", "Intensity", 0.0, 1.0, 0.01, "")]),
            def("crt", "CRT", Retro, En::Crt, &[("pitch", n(4.0)), ("glow", n(0.35))], vec![range("pitch", "Phosphor size", 3.0, 12.0, 1.0, "px"), range("glow", "Glow", 0.0, 1.0, 0.01, "")]),
            // Distort & camera
            def("shake", "Camera shake", Distort, En::Shake, &[("amount", n(0.4)), ("speed", n(1.0))], vec![range("amount", "Amount", 0.0, 1.5, 0.01, ""), range("speed", "Speed", 0.2, 4.0, 0.05, "×")]),
            def("mirror", "Mirror", Distort, En::Mirror, &[("mode", s("left")), ("segments", n(6.0)), ("spin", n(0.15))], mirror_params.clone()),
            def("kaleidoscope", "Kaleidoscope", Distort, En::Mirror, &[("mode", s("kaleido")), ("segments", n(8.0)), ("spin", n(0.15))], mirror_params),
            def("wave", "Wave", Distort, En::Wave, &[("amplitude", n(18.0)), ("frequency", n(3.0)), ("speed", n(2.0)), ("direction", s("horizontal"))], wave_params()),
            def("ripple", "Ripple", Distort, En::Wave, &[("amplitude", n(8.0)), ("frequency", n(9.0)), ("speed", n(5.0)), ("direction", s("vertical"))], wave_params()),
            def("zoom-blur", "Zoom blur", Distort, En::ZoomBlur, &[("amount", n(0.5))], vec![range("amount", "Amount", 0.0, 1.0, 0.01, "")]),
            def("tilt-shift", "Tilt-shift", Distort, En::TiltShift, &[("focus", n(0.55)), ("band", n(0.16)), ("blur", n(12.0)), ("saturate", b(true))],
                vec![range("focus", "Focus line", 0.0, 1.0, 0.01, ""), range("band", "Sharp band", 0.02, 0.5, 0.01, ""), range("blur", "Blur", 2.0, 40.0, 1.0, "px"), toggle("saturate", "Miniature colour")]),
        ];
        // Instagram-style filters: strength is the effect's amount, like the colour looks.
        list.extend(super::looks::LOOKS.iter().map(|l| EffectDef { id: l.id, name: l.name, group: Filters, engine: En::Look, css: None, defaults: Params::new(), params: vec![] }));
        list.extend(super::remocn::effect_defs());
        list
    })
}

pub fn effect_def(id: &str) -> Option<&'static EffectDef> {
    effects().iter().find(|e| e.id == id)
}

static UID: AtomicU64 = AtomicU64::new(1);
pub fn make_clip_effect(id: &str, amount: f64) -> Option<ClipEffect> {
    let d = effect_def(id)?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    Some(ClipEffect { uid: format!("fx{now:x}{}", UID.fetch_add(1, Ordering::Relaxed)), id: id.into(), enabled: true, amount, params: d.defaults.clone() })
}

pub fn num(p: &Params, k: &str, d: f64) -> f64 {
    match p.get(k) {
        Some(ParamValue::Num(v)) => *v,
        _ => d,
    }
}
pub fn text<'a>(p: &'a Params, k: &str, d: &'a str) -> &'a str {
    match p.get(k) {
        Some(ParamValue::Str(v)) => v,
        _ => d,
    }
}
pub fn flag(p: &Params, k: &str) -> bool {
    matches!(p.get(k), Some(ParamValue::Bool(true)))
}
