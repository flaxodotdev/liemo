// Instagram-style filters. Each look is a colour-filter chain (the CSS filter functions), then
// layers composited with blend modes (flat colour, radial or linear gradients), then optional
// faded blacks, split toning, vignette and grain. The classic feed filters follow CSSgram's
// recreations (MIT, Una Kravets); the Stories filters and the rest are approximations.

use super::engines::apply_css_filter;
use crate::model::time::rng;
use crate::render::canvas::{hex, hex_rgb, map_pixels, Canvas};
use tiny_skia::{BlendMode, Pixmap};

#[derive(Clone, Copy)]
pub enum Layer {
    /// A flat colour ("#rrggbb" or "#rrggbbaa").
    Solid(&'static str, BlendMode, f32),
    /// Circle from (cx, cy) (fractions of the frame) out to the farthest corner; colour stops at fractions.
    Radial(f32, f32, &'static [(f32, &'static str)], BlendMode, f32),
    /// Top to bottom (`true`) or left to right.
    Linear(bool, &'static [(f32, &'static str)], BlendMode, f32),
}

pub struct Look {
    pub id: &'static str,
    pub name: &'static str,
    pub css: &'static str,
    pub layers: &'static [Layer],
    /// Lift blacks (0–1).
    pub fade: f32,
    /// Shadows tint, highlights tint, strength.
    pub split: Option<(&'static str, &'static str, f32)>,
    pub vignette: f32,
    pub grain: f32,
}

use BlendMode::*;
use Layer::*;

const fn look(id: &'static str, name: &'static str, css: &'static str, layers: &'static [Layer]) -> Look {
    Look { id, name, css, layers, fade: 0.0, split: None, vignette: 0.0, grain: 0.0 }
}
const fn film(id: &'static str, name: &'static str, css: &'static str, layers: &'static [Layer], fade: f32, split: Option<(&'static str, &'static str, f32)>, vignette: f32, grain: f32) -> Look {
    Look { id, name, css, layers, fade, split, vignette, grain }
}

pub const LOOKS: &[Look] = &[
    // ---------- Instagram feed filters ----------
    look("ig-clarendon", "Clarendon", "contrast(1.2) saturate(1.35)", &[Solid("#7fbbe333", Overlay, 1.0)]),
    look("ig-gingham", "Gingham", "brightness(1.05) hue-rotate(-10deg)", &[Solid("#e6e6fa", SoftLight, 1.0)]),
    look("ig-moon", "Moon", "grayscale(1) contrast(1.1) brightness(1.1)", &[Solid("#a0a0a0", SoftLight, 1.0), Solid("#383838", Lighten, 1.0)]),
    look("ig-lark", "Lark", "contrast(0.9)", &[Solid("#22253f", ColorDodge, 1.0), Solid("#f2f2f2cc", Darken, 1.0)]),
    look("ig-reyes", "Reyes", "sepia(0.22) brightness(1.1) contrast(0.85) saturate(0.75)", &[Solid("#efcdad", SoftLight, 0.5)]),
    look("ig-juno", "Juno", "contrast(1.15) brightness(1.15) saturate(1.8) sepia(0.35)", &[Solid("#7f00ff0d", Overlay, 1.0)]),
    look("ig-slumber", "Slumber", "saturate(0.66) brightness(1.05)", &[Solid("#45290c66", Lighten, 1.0), Solid("#7d691880", SoftLight, 1.0)]),
    look("ig-crema", "Crema", "sepia(0.5) contrast(1.25) brightness(1.15) saturate(0.9) hue-rotate(-2deg)", &[Solid("#7d691833", Multiply, 1.0)]),
    look("ig-ludwig", "Ludwig", "sepia(0.25) contrast(1.05) brightness(1.05) saturate(2)", &[Solid("#7d69181a", Overlay, 1.0)]),
    look("ig-aden", "Aden", "hue-rotate(-20deg) contrast(0.9) saturate(0.85) brightness(1.2)", &[Linear(false, &[(0.0, "#420a0e33"), (1.0, "#420a0e00")], Darken, 1.0)]),
    look("ig-perpetua", "Perpetua", "", &[Linear(true, &[(0.0, "#005b9a"), (1.0, "#e6c13d")], SoftLight, 0.5)]),
    look("ig-amaro", "Amaro", "hue-rotate(-10deg) contrast(0.9) brightness(1.1) saturate(1.5)", &[Solid("#38383833", Screen, 1.0)]),
    look("ig-mayfair", "Mayfair", "contrast(1.1) saturate(1.1)", &[Radial(0.4, 0.4, &[(0.0, "#ffffffcc"), (0.3, "#ffc8c899"), (0.6, "#111111")], Overlay, 0.4)]),
    look("ig-rise", "Rise", "brightness(1.05) sepia(0.2) contrast(0.9) saturate(0.9)", &[
        Radial(0.5, 0.5, &[(0.55, "#eccda926"), (1.0, "#321e0766")], Multiply, 1.0),
        Radial(0.5, 0.5, &[(0.0, "#e8c598cc"), (0.9, "#e8c59800")], Overlay, 0.6),
    ]),
    look("ig-hudson", "Hudson", "brightness(1.2) contrast(0.9) saturate(1.1)", &[Radial(0.5, 0.5, &[(0.5, "#a6b1ff"), (1.0, "#342134")], Multiply, 0.5)]),
    look("ig-valencia", "Valencia", "contrast(1.08) brightness(1.08) sepia(0.08)", &[Solid("#3a0339", Exclusion, 0.5)]),
    look("ig-xpro2", "X-Pro II", "sepia(0.3)", &[Radial(0.5, 0.5, &[(0.4, "#e6e7e0"), (1.1, "#2b2aa199")], ColorBurn, 1.0)]),
    look("ig-sierra", "Sierra", "contrast(0.8) saturate(1.2) sepia(0.15)", &[Radial(0.5, 0.5, &[(0.0, "#80785033"), (1.0, "#00000099")], SoftLight, 1.0)]),
    look("ig-willow", "Willow", "grayscale(0.5) contrast(0.95) brightness(0.9)", &[Radial(0.5, 0.5, &[(0.55, "#d4a9af"), (1.5, "#000000")], Overlay, 1.0), Solid("#d8cdcb", Color, 1.0)]),
    look("ig-lofi", "Lo-Fi", "saturate(1.1) contrast(1.5)", &[Radial(0.5, 0.5, &[(0.7, "#22222200"), (1.5, "#222222")], Multiply, 1.0)]),
    look("ig-inkwell", "Inkwell", "sepia(0.3) contrast(1.1) brightness(1.1) grayscale(1)", &[]),
    look("ig-hefe", "Hefe", "contrast(1.5) saturate(1.3) sepia(0.15)", &[Radial(0.5, 0.5, &[(0.0, "#00000000"), (1.0, "#0000004d")], Multiply, 1.0)]),
    look("ig-nashville", "Nashville", "sepia(0.2) contrast(1.2) brightness(1.05) saturate(1.2)", &[Solid("#f7b0998f", Darken, 1.0), Solid("#00469666", Lighten, 1.0)]),
    look("ig-stinson", "Stinson", "contrast(0.75) saturate(0.85) brightness(1.15)", &[Solid("#f0958033", SoftLight, 1.0)]),
    look("ig-vesper", "Vesper", "sepia(0.35) contrast(1.15) brightness(1.2) saturate(1.3)", &[Solid("#7d691833", Overlay, 1.0)]),
    look("ig-earlybird", "Earlybird", "contrast(0.9) sepia(0.2)", &[Radial(0.5, 0.5, &[(0.2, "#d0ba8e"), (0.85, "#360309"), (1.0, "#1d0210")], Overlay, 1.0)]),
    look("ig-brannan", "Brannan", "sepia(0.5) contrast(1.4)", &[Solid("#a12cc74f", Lighten, 1.0)]),
    look("ig-sutro", "Sutro", "sepia(0.4) contrast(1.2) brightness(0.9) saturate(1.4) hue-rotate(-10deg)", &[Radial(0.5, 0.5, &[(0.5, "#00000000"), (0.9, "#000000cc")], Darken, 1.0)]),
    look("ig-toaster", "Toaster", "contrast(1.5) brightness(0.9)", &[Radial(0.5, 0.5, &[(0.0, "#804e0f"), (1.0, "#3b003b")], Screen, 1.0)]),
    look("ig-walden", "Walden", "brightness(1.1) hue-rotate(-10deg) sepia(0.3) saturate(1.6)", &[Solid("#0044cc", Screen, 0.3)]),
    look("ig-1977", "1977", "contrast(1.1) brightness(1.1) saturate(1.3)", &[Solid("#f36abc4d", Screen, 1.0)]),
    look("ig-kelvin", "Kelvin", "", &[Solid("#382c34", ColorDodge, 1.0), Solid("#b77d21", Overlay, 1.0)]),
    look("ig-maven", "Maven", "sepia(0.25) brightness(0.95) contrast(0.95) saturate(1.5)", &[Solid("#03e61a33", Hue, 1.0)]),
    look("ig-ginza", "Ginza", "sepia(0.25) contrast(1.15) brightness(1.2) saturate(1.35) hue-rotate(-5deg)", &[Solid("#7d691826", Darken, 1.0)]),
    look("ig-skyline", "Skyline", "saturate(1.4) contrast(1.15) brightness(1.15) hue-rotate(-5deg)", &[]),
    look("ig-dogpatch", "Dogpatch", "sepia(0.35) saturate(1.1) contrast(1.5)", &[]),
    look("ig-brooklyn", "Brooklyn", "contrast(0.9) brightness(1.1)", &[Radial(0.5, 0.5, &[(0.7, "#a8dfc166"), (1.0, "#c4b7c8")], Overlay, 1.0)]),
    look("ig-helena", "Helena", "sepia(0.5) contrast(1.05) brightness(1.05) saturate(1.35)", &[Solid("#9e2b1e1a", Overlay, 1.0)]),
    look("ig-ashby", "Ashby", "sepia(0.5) contrast(1.2) saturate(1.8)", &[Solid("#7d691859", Lighten, 1.0)]),
    look("ig-charmes", "Charmes", "sepia(0.25) contrast(1.25) brightness(1.25) saturate(1.35) hue-rotate(-5deg)", &[Solid("#7d691840", Darken, 1.0)]),
    look("ig-poprocket", "Poprocket", "sepia(0.15) brightness(1.2)", &[Radial(0.5, 0.5, &[(0.4, "#ce274575"), (0.8, "#000000")], Screen, 1.0)]),

    // ---------- Instagram Stories city filters ----------
    film("ig-paris", "Paris", "brightness(1.08) contrast(0.92) saturate(0.95)", &[Solid("#ffd6e01f", SoftLight, 1.0)], 0.06, None, 0.0, 0.0),
    film("ig-los-angeles", "Los Angeles", "saturate(1.35) contrast(1.05) brightness(1.05) sepia(0.12)", &[Solid("#ffb35a26", SoftLight, 1.0)], 0.0, None, 0.0, 0.0),
    film("ig-oslo", "Oslo", "saturate(0.8) contrast(0.95) brightness(1.08) hue-rotate(5deg)", &[Solid("#9ec8ff26", SoftLight, 1.0)], 0.12, None, 0.0, 0.0),
    film("ig-lagos", "Lagos", "sepia(0.3) saturate(1.2) contrast(1.05)", &[Solid("#c4573a26", Overlay, 1.0)], 0.04, None, 0.0, 0.0),
    film("ig-melbourne", "Melbourne", "sepia(0.2) saturate(0.9) brightness(1.06)", &[Solid("#f0c08a2e", SoftLight, 1.0)], 0.1, None, 0.0, 0.0),
    film("ig-jakarta", "Jakarta", "saturate(1.3) contrast(1.15) brightness(1.08)", &[], 0.0, None, 0.0, 0.0),
    film("ig-abu-dhabi", "Abu Dhabi", "sepia(0.25) saturate(1.25) brightness(1.08)", &[Solid("#ffcc6626", Overlay, 1.0)], 0.0, None, 0.0, 0.0),
    film("ig-buenos-aires", "Buenos Aires", "contrast(1.25) saturate(0.95)", &[Solid("#4a6fa526", SoftLight, 1.0)], 0.0, None, 0.15, 0.0),
    film("ig-new-york", "New York", "saturate(0.75) contrast(1.1) sepia(0.15)", &[], 0.06, None, 0.1, 0.0),
    film("ig-jaipur", "Jaipur", "saturate(1.2) brightness(1.05)", &[Solid("#ff6fae2e", SoftLight, 1.0)], 0.0, None, 0.0, 0.0),
    film("ig-cairo", "Cairo", "sepia(0.3) saturate(0.85) contrast(0.95)", &[Solid("#9db38b26", SoftLight, 1.0)], 0.1, None, 0.0, 0.0),
    film("ig-tokyo", "Tokyo", "saturate(0.9) contrast(1.05) brightness(1.04) hue-rotate(8deg)", &[Solid("#6aa6ff2e", SoftLight, 1.0)], 0.05, None, 0.0, 0.0),
    film("ig-rio", "Rio de Janeiro", "saturate(1.45) contrast(1.1) brightness(1.1)", &[Solid("#ffd34d1f", Overlay, 1.0)], 0.0, None, 0.0, 0.0),

    // ---------- Film stocks and moods ----------
    film("ig-portra", "Portra 400", "saturate(0.9) contrast(0.95) brightness(1.05)", &[], 0.06, Some(("#3a4a5a", "#ffd8b0", 0.18)), 0.0, 0.06),
    film("ig-ektar", "Ektar 100", "saturate(1.4) contrast(1.15)", &[], 0.0, Some(("#202a40", "#ffc890", 0.12)), 0.0, 0.04),
    film("ig-kodachrome", "Kodachrome", "saturate(1.3) contrast(1.25) sepia(0.12)", &[Solid("#ff602015", Overlay, 1.0)], 0.0, None, 0.12, 0.05),
    film("ig-velvia", "Velvia", "saturate(1.75) contrast(1.2) brightness(0.98)", &[], 0.0, None, 0.08, 0.03),
    film("ig-superia", "Superia", "saturate(1.15) contrast(1.05) hue-rotate(6deg)", &[Solid("#4fa36b1a", SoftLight, 1.0)], 0.04, None, 0.0, 0.06),
    film("ig-trix", "Tri-X", "grayscale(1) contrast(1.45) brightness(0.95)", &[], 0.0, None, 0.2, 0.12),
    film("ig-hp5", "HP5", "grayscale(1) contrast(1.15) brightness(1.05)", &[], 0.08, None, 0.1, 0.1),
    film("ig-cinestill", "Cinestill 800T", "saturate(1.1) contrast(1.1) hue-rotate(-8deg)", &[Solid("#4a7aff26", SoftLight, 1.0)], 0.04, Some(("#0e2a4a", "#ff8a5c", 0.15)), 0.1, 0.08),
    film("ig-polaroid", "Polaroid", "sepia(0.2) saturate(0.85) contrast(0.9) brightness(1.1)", &[Solid("#9fd7c41f", SoftLight, 1.0)], 0.14, None, 0.18, 0.05),
    film("ig-instax", "Instax", "saturate(1.1) brightness(1.12) contrast(0.92)", &[Solid("#fff0e01f", SoftLight, 1.0)], 0.08, None, 0.0, 0.03),
    film("ig-teal-orange", "Teal & orange", "saturate(1.15) contrast(1.15)", &[], 0.0, Some(("#00636e", "#ff9a3c", 0.32)), 0.0, 0.0),
    film("ig-bleach", "Bleach bypass", "saturate(0.45) contrast(1.4) brightness(1.02)", &[], 0.0, None, 0.12, 0.04),
    film("ig-day-night", "Day for night", "brightness(0.65) saturate(0.6) contrast(1.2) hue-rotate(10deg)", &[Solid("#1a3a8a40", Multiply, 1.0)], 0.0, None, 0.25, 0.04),
    film("ig-matte", "Matte", "contrast(0.88) saturate(0.9)", &[], 0.16, None, 0.0, 0.0),
    film("ig-pastel", "Pastel", "saturate(0.7) brightness(1.15) contrast(0.85)", &[Solid("#ffd8ec1f", SoftLight, 1.0)], 0.12, None, 0.0, 0.0),
    film("ig-golden", "Golden hour", "sepia(0.25) saturate(1.3) brightness(1.06)", &[Radial(0.7, 0.2, &[(0.0, "#ffb44a59"), (0.8, "#ffb44a00")], Screen, 1.0)], 0.0, None, 0.0, 0.0),
    film("ig-blue-hour", "Blue hour", "saturate(0.95) brightness(0.95) hue-rotate(12deg)", &[Solid("#3d5afe26", SoftLight, 1.0)], 0.04, Some(("#13234a", "#b9c8ff", 0.15)), 0.1, 0.0),
    film("ig-moody", "Moody", "saturate(0.75) contrast(1.2) brightness(0.9)", &[], 0.06, Some(("#1d2a2a", "#c9a27e", 0.15)), 0.22, 0.04),
    film("ig-cross", "Cross process", "contrast(1.3) saturate(1.4) hue-rotate(-15deg)", &[Solid("#e8ff7a26", Overlay, 1.0)], 0.0, Some(("#00305a", "#ffe066", 0.2)), 0.1, 0.0),
    film("ig-lomo", "Lomo", "saturate(1.5) contrast(1.35)", &[], 0.0, None, 0.45, 0.06),
    film("ig-analog", "Analog", "sepia(0.18) saturate(0.95) contrast(1.05)", &[], 0.1, Some(("#2b3b3a", "#f3d3a8", 0.12)), 0.12, 0.12),
    film("ig-vintage", "Vintage", "sepia(0.45) saturate(0.85) contrast(0.95) brightness(1.05)", &[], 0.14, None, 0.25, 0.1),
    film("ig-70s", "Retro 70s", "sepia(0.35) saturate(1.2) hue-rotate(-8deg)", &[Solid("#ff9f4026", Overlay, 1.0)], 0.1, None, 0.15, 0.08),
    film("ig-sunkissed", "Sun-kissed", "saturate(1.2) brightness(1.08) sepia(0.15)", &[Radial(0.15, 0.1, &[(0.0, "#ffd08a66"), (0.9, "#ffd08a00")], Screen, 1.0)], 0.0, None, 0.0, 0.0),
    film("ig-arctic", "Arctic", "saturate(0.7) brightness(1.12) hue-rotate(10deg)", &[Solid("#cfe8ff33", SoftLight, 1.0)], 0.06, None, 0.0, 0.0),
    film("ig-desert", "Desert", "sepia(0.35) saturate(1.1) contrast(1.05)", &[Solid("#d9925a26", Overlay, 1.0)], 0.06, None, 0.08, 0.04),
    film("ig-forest", "Forest", "saturate(1.1) contrast(1.05) hue-rotate(-6deg)", &[], 0.0, Some(("#0f2a1c", "#e2e6a8", 0.18)), 0.12, 0.0),
    film("ig-autumn", "Autumn", "sepia(0.3) saturate(1.35) hue-rotate(-10deg)", &[], 0.0, Some(("#3a1a10", "#ffb060", 0.15)), 0.1, 0.0),
    film("ig-neon", "Neon nights", "saturate(1.6) contrast(1.25) brightness(0.92)", &[Linear(false, &[(0.0, "#ff2fd04d"), (1.0, "#2fd8ff4d")], Overlay, 1.0)], 0.0, None, 0.2, 0.0),
    film("ig-cyberpunk", "Cyberpunk", "saturate(1.35) contrast(1.2) hue-rotate(-20deg)", &[], 0.0, Some(("#1c1a6b", "#ff4fd8", 0.3)), 0.15, 0.0),
    film("ig-vaporwave", "Vaporwave", "saturate(1.25) brightness(1.06)", &[Linear(true, &[(0.0, "#ff71ce40"), (1.0, "#01cdfe40")], SoftLight, 1.0)], 0.08, None, 0.0, 0.0),
    film("ig-noir", "Noir film", "grayscale(1) contrast(1.6) brightness(0.9)", &[], 0.0, None, 0.35, 0.1),
    film("ig-silver", "Silver", "grayscale(1) contrast(1.1) brightness(1.08)", &[Solid("#c9d6e01f", SoftLight, 1.0)], 0.06, None, 0.0, 0.03),
    film("ig-sepia-tone", "Sepia tone", "sepia(0.85) contrast(1.05)", &[], 0.06, None, 0.15, 0.06),
    film("ig-cream", "Cream", "sepia(0.2) saturate(0.85) brightness(1.1) contrast(0.92)", &[Solid("#fff2d633", SoftLight, 1.0)], 0.1, None, 0.0, 0.0),
    film("ig-lavender", "Lavender", "saturate(0.9) brightness(1.06) hue-rotate(6deg)", &[Solid("#c9a8ff33", SoftLight, 1.0)], 0.08, None, 0.0, 0.0),
    film("ig-rose", "Rosé", "saturate(1.05) brightness(1.04)", &[Solid("#ff8fb033", SoftLight, 1.0)], 0.06, Some(("#3a1a2a", "#ffd0dc", 0.1)), 0.0, 0.0),
    film("ig-honey", "Honey", "sepia(0.3) saturate(1.25) brightness(1.05)", &[Solid("#ffbf4033", SoftLight, 1.0)], 0.04, None, 0.0, 0.0),
    film("ig-ocean", "Ocean", "saturate(1.1) hue-rotate(8deg)", &[], 0.0, Some(("#062a4a", "#9be8ff", 0.25)), 0.0, 0.0),
    film("ig-dusk", "Dusk", "saturate(1.1) brightness(0.95) contrast(1.05)", &[Linear(true, &[(0.0, "#ff7a5c40"), (1.0, "#3d2a6b40")], SoftLight, 1.0)], 0.0, None, 0.15, 0.0),
    film("ig-dawn", "Dawn", "saturate(0.95) brightness(1.1) contrast(0.95)", &[Linear(true, &[(0.0, "#ffd1a840"), (1.0, "#a8c5ff33")], SoftLight, 1.0)], 0.06, None, 0.0, 0.0),
    film("ig-punchy", "Punchy", "saturate(1.45) contrast(1.3)", &[], 0.0, None, 0.0, 0.0),
    film("ig-airy", "Airy", "brightness(1.18) contrast(0.85) saturate(0.9)", &[], 0.1, None, 0.0, 0.0),
    film("ig-dramatic", "Dramatic", "contrast(1.45) saturate(0.85) brightness(0.92)", &[], 0.0, None, 0.35, 0.03),
    film("ig-chrome", "Chrome", "saturate(1.25) contrast(1.2) brightness(1.04)", &[], 0.0, Some(("#1a2030", "#f0f4ff", 0.1)), 0.0, 0.0),
    film("ig-mono-warm", "Warm mono", "grayscale(1) sepia(0.25) contrast(1.15)", &[], 0.04, None, 0.15, 0.06),
];

pub fn look_def(id: &str) -> Option<&'static Look> {
    LOOKS.iter().find(|l| l.id == id)
}

fn layer_fill(c: &Canvas, l: &Layer) -> (crate::render::canvas::Fill, BlendMode, f32) {
    let (w, h) = (c.w(), c.h());
    let stops = |s: &[(f32, &str)], k: f32| s.iter().map(|(p, col)| ((p / k).clamp(0.0, 1.0), hex(col))).collect::<Vec<_>>();
    match *l {
        Solid(col, b, o) => (crate::render::canvas::Fill::Solid(hex(col)), b, o),
        Radial(cx, cy, s, b, o) => {
            let (x, y) = (cx * w, cy * h);
            // CSS "circle" sizes to the farthest corner.
            let r = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)].iter().map(|(px, py)| (px - x).hypot(py - y)).fold(0.0, f32::max);
            // Stops past 100% extend the circle.
            let k = s.iter().map(|(p, _)| *p).fold(1.0, f32::max);
            (c.radial(x, y, 0.0, x, y, r * k, &stops(s, k)), b, o)
        }
        Linear(vertical, s, b, o) => (if vertical { c.linear(0.0, 0.0, 0.0, h, &stops(s, 1.0)) } else { c.linear(0.0, 0.0, w, 0.0, &stops(s, 1.0)) }, b, o),
    }
}

/// Apply a look to a frame. Transparent areas stay transparent (layers are kept to the picture).
pub fn apply_look(src: &Pixmap, l: &Look, t: f32) -> Pixmap {
    let mut pm = src.clone();
    if !l.css.is_empty() {
        apply_css_filter(&mut pm, l.css);
    }
    if !l.layers.is_empty() {
        let mut c = Canvas::from_pixmap(pm);
        for layer in l.layers {
            let (fill, blend, opacity) = layer_fill(&c, layer);
            c.save();
            c.set_blend(blend);
            c.set_alpha(opacity);
            c.set_fill(fill);
            let (w, h) = (c.w(), c.h());
            c.fill_rect(0.0, 0.0, w, h);
            c.restore();
        }
        pm = c.into_pixmap();
        // Blend layers paint over transparent pixels too; cut back to the source's coverage.
        for (o, s) in pm.data_mut().chunks_exact_mut(4).zip(src.data().chunks_exact(4)) {
            let (a, sa) = (o[3] as f32, s[3] as f32);
            if sa < 255.0 && a > 0.0 {
                let k = sa / a;
                for v in o.iter_mut() {
                    *v = (*v as f32 * k).round() as u8;
                }
            }
        }
    }
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    let split = l.split.map(|(s, hi, k)| (hex_rgb(s), hex_rgb(hi), k));
    let mut r = rng((t * 24.0).floor() as i64 + 31);
    let (fade, vig, grain) = (l.fade, l.vignette, l.grain);
    if fade > 0.0 || split.is_some() || vig > 0.0 || grain > 0.0 {
        let width = pm.width() as usize;
        map_pixels(&mut pm, |i, v| {
            if let Some((sh, hi, k)) = split {
                let lum = (0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2]) / 255.0;
                let (ws, wh) = ((1.0 - lum).powi(2) * k, lum.powi(2) * k);
                for c in 0..3 {
                    v[c] += (sh[c] - v[c]) * ws + (hi[c] - v[c]) * wh;
                }
            }
            if fade > 0.0 {
                for c in v.iter_mut().take(3) {
                    *c = *c * (1.0 - fade * 0.6) + 255.0 * fade * 0.35;
                }
            }
            if vig > 0.0 {
                let (x, y) = ((i % width) as f32 / w - 0.5, (i / width) as f32 / h - 0.5);
                let d = ((x * x + y * y).sqrt() / 0.7071).clamp(0.0, 1.0);
                let k = 1.0 - vig * d.powf(2.2);
                for c in v.iter_mut().take(3) {
                    *c *= k;
                }
            }
            if grain > 0.0 {
                let n = (r.next() - 0.5) * 255.0 * grain * 0.5;
                for c in v.iter_mut().take(3) {
                    *c += n;
                }
            }
        });
    }
    pm
}
