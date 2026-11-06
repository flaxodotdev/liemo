// remocn's CSS transitions (MIT, https://github.com/Remocn/remocn-studio), ported from their
// presentation components. Each one styles the exiting scene and the entering scene as remocn's
// TransitionSeries does (entering on top unless noted), and several raise a Paper shader field
// between the two. Timings, easings and defaults are remocn's. CSS pixels are 1280-wide
// composition pixels.

use super::engines::{apply_css_filter, blur_of};
use super::paper::render_with;
use crate::render::canvas::Canvas;
use serde_json::{json, Value};
use std::borrow::Cow;
use std::f32::consts::PI;
use tiny_skia::Pixmap;

// ---------- remotion's interpolate and easings ----------
#[derive(Clone, Copy)]
enum Ease {
    Linear,
    Bezier(f32, f32, f32, f32),
    InCubic,
    OutCubic,
    InQuad,
    OutQuad,
}
use Ease::*;
const STANDARD: Ease = Bezier(0.42, 0.0, 0.58, 1.0);

fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let curve = |a1: f32, a2: f32, t: f32| 3.0 * a1 * t * (1.0 - t).powi(2) + 3.0 * a2 * t * t * (1.0 - t) + t * t * t;
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if curve(x1, x2, mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    curve(y1, y2, (lo + hi) / 2.0)
}
fn ease(e: Ease, t: f32) -> f32 {
    match e {
        Linear => t,
        Bezier(a, b, c, d) => bezier(a, b, c, d, t),
        InCubic => t * t * t,
        OutCubic => 1.0 - (1.0 - t).powi(3),
        InQuad => t * t,
        OutQuad => 1.0 - (1.0 - t) * (1.0 - t),
    }
}
/// remotion's interpolate, clamped at both ends; the easing applies per segment.
fn ip(p: f32, xs: &[f32], ys: &[f32], e: Ease) -> f32 {
    let n = xs.len();
    if p <= xs[0] {
        return ys[0];
    }
    if p >= xs[n - 1] {
        return ys[n - 1];
    }
    let i = (0..n - 1).find(|&i| p <= xs[i + 1]).unwrap_or(n - 2);
    let t = (p - xs[i]) / (xs[i + 1] - xs[i]).max(1e-6);
    ys[i] + (ys[i + 1] - ys[i]) * ease(e, t)
}

// ---------- layers ----------
/// A scene's CSS: opacity, scale (about `origin`), translate in px, rotate, blur and brightness.
struct Style {
    opacity: f32,
    sx: f32,
    sy: f32,
    dx: f32,
    dy: f32,
    rotate: f32,
    origin: (f32, f32),
    blur: f32,
    bright: f32,
}
impl Default for Style {
    fn default() -> Self {
        Style { opacity: 1.0, sx: 1.0, sy: 1.0, dx: 0.0, dy: 0.0, rotate: 0.0, origin: (0.5, 0.5), blur: 0.0, bright: 1.0 }
    }
}
fn st(opacity: f32, scale: f32, blur: f32) -> Style {
    Style { opacity, sx: scale, sy: scale, blur, ..Default::default() }
}

fn layer(c: &mut Canvas, src: &Pixmap, s: Style) {
    if s.opacity <= 0.002 {
        return;
    }
    let mut img = Cow::Borrowed(src);
    if s.blur > 0.3 {
        img = Cow::Owned(blur_of(src, s.blur));
    }
    if (s.bright - 1.0).abs() > 0.004 {
        let mut m = img.into_owned();
        apply_css_filter(&mut m, &format!("brightness({})", s.bright));
        img = Cow::Owned(m);
    }
    let (w, h) = (c.w(), c.h());
    let (ox, oy) = (s.origin.0 * w, s.origin.1 * h);
    c.save();
    c.set_alpha(c.alpha() * s.opacity.min(1.0));
    c.translate(s.dx + ox, s.dy + oy);
    if s.rotate != 0.0 {
        c.rotate(s.rotate.to_radians());
    }
    c.scale(s.sx, s.sy);
    c.translate(-ox, -oy);
    c.draw_image_scaled(&img, 0.0, 0.0, w, h);
    c.restore();
}
fn exit(c: &mut Canvas, a: Option<&Pixmap>, s: Style) {
    if let Some(a) = a {
        layer(c, a, s);
    }
}
/// A Paper field from one of remocn's shader components, faded in at `opacity`.
fn field(c: &mut Canvas, component: &str, props: &[(&str, Value)], t: f32, opacity: f32) {
    if opacity <= 0.002 {
        return;
    }
    if let Some(pm) = render_with(component, props, t, c.width(), c.height()) {
        layer(c, &pm, st(opacity, 1.0, 0.0));
    }
}

const VIOLET: [&str; 3] = ["#3a3a52", "#4a4a68", "#8f88ae"];
const BACK: &str = "#141318";

// ---------- shader-field transitions ----------
pub fn grain_dissolve(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, t: f32) {
    exit(c, a, st(ip(p, &[0.14, 0.34], &[1.0, 0.0], Linear), 1.0, ip(p, &[0.06, 0.34], &[0.0, 10.0], Linear)));
    let k = ip(p, &[0.08, 0.92], &[0.0, 1.0], STANDARD);
    let props = [
        ("colors", json!(VIOLET)),
        ("colorBack", json!(BACK)),
        ("shape", json!("blob")),
        ("noise", json!(0.3)),
        ("intensity", json!(k)),
        ("softness", json!(k)),
        ("scale", json!(2.0 * ip(p, &[0.0, 1.0], &[0.9, 1.15], Linear))),
    ];
    field(c, "shader-grain-gradient", &props, t, ip(p, &[0.0, 0.26], &[0.0, 1.0], STANDARD));
    layer(c, b, st(ip(p, &[0.7, 0.86], &[0.0, 1.0], Linear), ip(p, &[0.7, 1.0], &[1.05, 1.0], OutCubic), ip(p, &[0.7, 0.92], &[10.0, 0.0], Linear)));
}

pub fn dither_dissolve(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, t: f32) {
    exit(c, a, st(ip(p, &[0.28, 0.36], &[1.0, 0.0], Linear), 1.0, 0.0));
    layer(c, b, st(ip(p, &[0.58, 0.66], &[0.0, 1.0], Linear), 1.0, 0.0));
    let props = [
        ("colorBack", json!(BACK)),
        ("colorFront", json!("#8f88ae")),
        ("shape", json!("simplex")),
        ("speed", json!(1.5)),
        ("size", json!(ip(p, &[0.0, 1.0], &[1.6, 2.8], Linear))),
    ];
    field(c, "shader-dithering", &props, t, ip(p, &[0.0, 0.3, 0.65, 1.0], &[0.0, 1.0, 1.0, 0.0], Linear));
}

pub fn perlin_dissolve(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, t: f32) {
    exit(c, a, st(ip(p, &[0.06, 0.13], &[1.0, 0.0], Linear), 1.0, 0.0));
    let props = [("colorBack", json!(BACK)), ("colorFront", json!("#8f88ae")), ("softness", json!(0.1)), ("proportion", json!(ip(p, &[0.0, 0.5], &[0.0, 1.0], STANDARD)))];
    field(c, "shader-perlin-noise", &props, t, 1.0);
    layer(c, b, st(ip(p, &[0.48, 0.6], &[0.0, 1.0], Linear), 1.0, 0.0));
}

pub fn smoke_dissolve(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, t: f32) {
    exit(c, a, st(ip(p, &[0.06, 0.13], &[1.0, 0.0], Linear), 1.0, 0.0));
    let props = [("colorBack", json!(BACK)), ("colors", json!(["#8f88ae"])), ("radius", json!(p)), ("scale", json!(1.0 + p)), ("thickness", json!(1.0 + 0.5 * p))];
    field(c, "shader-smoke-ring", &props, t, 1.0);
    layer(c, b, st(ip(p, &[0.55, 0.68], &[0.0, 1.0], Linear), ip(p, &[0.55, 1.0], &[0.3, 1.0], OutCubic), ip(p, &[0.55, 0.75], &[10.0, 0.0], Linear)));
}

pub fn swirl_dissolve(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, t: f32) {
    exit(c, a, st(ip(p, &[0.06, 0.13], &[1.0, 0.0], Linear), 1.0, 0.0));
    let props = [
        ("colors", json!(["#1f1d29", "#413d56", "#8f88ae"])),
        ("colorBack", json!(BACK)),
        ("bandCount", json!(10)),
        ("softness", json!(0.35)),
        ("twist", json!(ip(p, &[0.1, 0.42, 0.64, 1.0], &[1.0, 0.0, 0.0, 1.0], STANDARD))),
    ];
    field(c, "shader-swirl", &props, t, 1.0);
    layer(c, b, st(ip(p, &[0.77, 0.87], &[0.0, 1.0], Linear), ip(p, &[0.77, 1.0], &[0.3, 1.0], OutCubic), ip(p, &[0.77, 0.93], &[10.0, 0.0], Linear)));
}

pub fn warp_dissolve(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, t: f32) {
    exit(c, a, st(ip(p, &[0.2, 0.38], &[1.0, 0.0], Linear), ip(p, &[0.0, 0.38], &[1.0, 1.16], InCubic), ip(p, &[0.04, 0.38], &[0.0, 18.0], Linear)));
    let props = [
        ("colors", json!(["#141318", "#3a3a5c", "#1f1d29", "#8f88ae"])),
        ("distortion", json!(ip(p, &[0.0, 0.5, 1.0], &[0.24, 0.8, 0.32], Linear))),
        ("swirl", json!(0.6)),
        ("softness", json!(1.0)),
    ];
    field(c, "shader-warp", &props, t, ip(p, &[0.0, 0.32], &[0.0, 1.0], STANDARD));
    layer(c, b, st(ip(p, &[0.66, 0.82], &[0.0, 1.0], Linear), ip(p, &[0.66, 1.0], &[1.18, 1.0], OutCubic), ip(p, &[0.66, 0.92], &[16.0, 0.0], Linear)));
}

pub fn wave_wipe(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, t: f32) {
    let h = c.h();
    exit(c, a, Style { opacity: ip(p, &[0.3, 0.5], &[1.0, 0.0], STANDARD), dy: ip(p, &[0.0, 0.7], &[0.0, -0.7], InCubic) * h, ..Default::default() });
    let props = [
        ("shape", json!("wave")),
        ("colors", json!(VIOLET)),
        ("colorBack", json!(BACK)),
        ("intensity", json!(0.2)),
        ("softness", json!(0.7)),
        ("noise", json!(0.4)),
        ("scale", json!(1.16)),
        ("offsetY", json!(ip(p, &[0.0, 1.0], &[0.0, 0.7], Bezier(0.45, 0.0, 0.55, 1.0)))),
    ];
    field(c, "shader-grain-gradient", &props, t, ip(p, &[0.18, 0.45], &[0.0, 1.0], STANDARD));
    layer(c, b, Style { dy: ip(p, &[0.4, 0.82, 1.0], &[1.0, -0.035, 0.0], OutCubic) * h, ..Default::default() });
}

pub fn ripple_zoom(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, t: f32) {
    exit(c, a, st(ip(p, &[0.35, 0.55], &[1.0, 0.0], STANDARD), ip(p, &[0.3, 0.62], &[1.0, 1.6], InCubic), 0.0));
    let props = [
        ("shape", json!("ripple")),
        ("colors", json!(VIOLET)),
        ("colorBack", json!(BACK)),
        ("intensity", json!(0.5)),
        ("softness", json!(0.5)),
        ("noise", json!(0.5)),
        ("scale", json!(ip(p, &[0.32, 1.0], &[0.2, 4.0], Bezier(0.7, 0.0, 0.3, 1.0)))),
    ];
    field(c, "shader-grain-gradient", &props, t, ip(p, &[0.08, 0.32], &[0.0, 1.0], STANDARD));
    layer(c, b, st(ip(p, &[0.45, 0.62], &[0.0, 1.0], Linear), ip(p, &[0.42, 0.97], &[0.2, 1.0], Bezier(0.33, 1.0, 0.68, 1.0)), ip(p, &[0.42, 0.8], &[8.0, 0.0], Linear)));
}

// ---------- camera transitions ----------
pub fn focus_pull(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    exit(
        c,
        a,
        Style {
            bright: ip(p, &[0.1, 0.55], &[1.0, 1.3], Linear),
            ..st(ip(p, &[0.42, 0.68], &[1.0, 0.0], STANDARD), ip(p, &[0.0, 0.6], &[1.0, 1.05], STANDARD), ip(p, &[0.05, 0.55], &[0.0, 16.0], InQuad))
        },
    );
    layer(
        c,
        b,
        Style {
            bright: ip(p, &[0.4, 0.85], &[1.25, 1.0], Linear),
            ..st(ip(p, &[0.32, 0.52], &[0.0, 1.0], Linear), ip(p, &[0.35, 1.0], &[0.97, 1.0], OutCubic), ip(p, &[0.35, 0.9], &[16.0, 0.0], OutQuad))
        },
    );
}

pub fn push_through(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    exit(c, a, st(ip(p, &[0.4, 0.62], &[1.0, 0.0], STANDARD), ip(p, &[0.0, 0.65], &[1.0, 2.4], InCubic), ip(p, &[0.15, 0.6], &[0.0, 14.0], Linear)));
    layer(c, b, st(ip(p, &[0.3, 0.5], &[0.0, 1.0], Linear), ip(p, &[0.3, 0.88, 1.0], &[0.68, 1.02, 1.0], OutCubic), ip(p, &[0.3, 0.75], &[9.8, 0.0], Linear)));
}

pub fn smear_pan(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let w = c.w();
    let travel = ip(p, &[0.0, 1.0], &[0.0, 1.0], Bezier(0.7, 0.0, 0.2, 1.0));
    let v = (PI * travel).sin();
    let smear = |dx: f32| Style { dx, sx: 1.0 + v * 0.12, blur: v * 24.0, ..Default::default() };
    exit(c, a, smear(-travel * 1.1 * w));
    layer(c, b, smear(-(travel - 1.0) * 1.1 * w));
}

pub fn zoom_blur(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    exit(c, a, st(1.0 - p, 1.0 + p * 0.12, p * 16.0));
    layer(c, b, st(p, 0.9 + p * 0.1, (1.0 - p) * 16.0));
}

/// The outgoing page lifts away in a few held poses, over the incoming one.
pub fn page_turn(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let poses = 8.0;
    let eased = p.clamp(0.0, 1.0).powi(3);
    let pose = (eased * poses).floor().min(poses - 1.0) / (poses - 1.0);
    let h = c.h();
    layer(c, b, Style::default());
    exit(c, a, Style { dy: -pose * h * 1.28, rotate: -7.0 * pose, origin: (0.18, 1.0), ..Default::default() });
}

// ---------- multi-pass shader transitions ----------
fn smoother(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}
fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// remocn's seam: a still material field rises over the outgoing scene, the cut happens under
/// it, and it dissolves away off the incoming one.
pub fn seam(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    use super::gpu::{gpu, U};
    use super::paper::flip_rows;
    let presence = smoother((p / 0.34).clamp(0.0, 1.0));
    let dissolve = smoother(((p - 0.46) / 0.54).clamp(0.0, 1.0));
    if p >= 0.4 {
        layer(c, b, Style::default());
    } else {
        exit(c, a, Style::default());
    }
    if p <= 0.0 || p >= 1.0 {
        return;
    }
    let (w, h) = (c.width(), c.height());
    let u = [
        ("iResolution", U::V2([w as f32, h as f32])),
        ("iTime", U::F(0.0)),
        ("uLightMode", U::F(0.0)),
        ("uDarkBackground", U::V3([9.0 / 255.0; 3])),
        ("uLightBackground", U::V3([1.0; 3])),
        ("uPixelRatio", U::F(w.max(h) as f32 / 1280.0)),
        ("uTilt", U::V2([0.0, 0.0])),
        ("uPresence", U::F(presence)),
        ("uDissolve", U::F(dissolve)),
        ("uSoftness", U::F(0.18)),
        ("uDetail", U::F(0.65)),
    ];
    let none = std::collections::HashMap::new();
    let Some(g) = gpu() else { return };
    let Some(full) = g.run("remocn:seam-field", include_str!("../../assets/remocn/shader-seam-1.frag"), &none, w, h, &[], &u) else { return };
    let full = flip_rows(&full);
    let Some(material) = g.run("remocn:seam-material", include_str!("../../assets/remocn/shader-seam-2.frag"), &none, w, h, &[("tScene", &full)], &u) else { return };
    let material = flip_rows(&material);
    if let Some(out) = g.run_premul("remocn:seam-composite", include_str!("../../assets/remocn/shader-seam-3.frag"), &none, w, h, &[("tScene", &material), ("tField", &full)], &u) {
        c.save();
        c.draw_image(&out, 0.0, 0.0);
        c.restore();
    }
}

/// remocn's spiral pass: dive into the light tunnel, and the incoming scene opens out of its
/// centre through a widening round aperture.
pub fn spiral_pass(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let p = p.clamp(0.0, 1.0);
    let dive = ((p - 0.08) / 0.84).clamp(0.0, 1.0);
    let zoom = (14.0f32.ln() * dive * dive).exp();
    let cover = smooth(0.0, 0.18, p);
    let aperture = smooth(0.64, 1.0, p).powi(2);
    if p < 0.2 {
        exit(c, a, st(1.0, 1.0 + 0.5 * smooth(0.0, 0.22, p), 0.0));
    }
    let (w, h) = (c.width(), c.height());
    if p > 0.0 && p < 1.0 && cover > 0.002 {
        let time = p * (1.0 + 0.8 * p * p);
        if let Some(t) = super::paper::light_tunnel_with(time, w, h, 0.7, 3.0, zoom) {
            layer(c, &t, st(cover, 1.0, 0.0));
        }
    }
    if p >= 0.2 {
        let opacity = if p < 1.0 { smooth(0.64, 0.72, p) } else { 1.0 };
        // The incoming scene, zoomed, cut to a soft circle that grows from the centre.
        let mut scene = Canvas::new(w, h);
        layer(&mut scene, b, st(1.0, 1.0 + 0.12 * (1.0 - smooth(0.55, 1.0, p)), 0.0));
        let mut scene = scene.into_pixmap();
        if p < 1.0 {
            let (wf, hf) = (w as f32, h as f32);
            let feather = wf.min(hf) * 0.12;
            let radius = ((wf * wf + hf * hf).sqrt() / 2.0 + feather) * aperture;
            let edge = feather.min(radius * 0.12);
            let inner = (radius - edge).max(0.0);
            for (i, px) in scene.data_mut().chunks_exact_mut(4).enumerate() {
                let (x, y) = ((i as u32 % w) as f32 + 0.5 - wf / 2.0, (i as u32 / w) as f32 + 0.5 - hf / 2.0);
                let d = (x * x + y * y).sqrt();
                let m = if d <= inner { 1.0 } else if d >= radius { 0.0 } else { 1.0 - (d - inner) / (radius - inner).max(1e-3) };
                if m < 1.0 {
                    for v in px.iter_mut() {
                        *v = (*v as f32 * m).round() as u8;
                    }
                }
            }
        }
        layer(c, &scene, st(opacity, 1.0, 0.0));
    }
}

// ---------- typographic & motion transitions ----------

/// A tall text caret sweeps the frame; behind it the incoming scene is typed in, ahead of it
/// the outgoing scene is backspaced away.
pub fn caret_wipe(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h) = (c.w(), c.h());
    let k = w.max(h) / 1280.0;
    let x = ip(p, &[0.0, 1.0], &[0.0, 1.0], Bezier(0.65, 0.0, 0.35, 1.0)) * w;
    let clipped = |c: &mut Canvas, x0: f32, x1: f32, f: &dyn Fn(&mut Canvas)| {
        c.save();
        c.begin_path();
        c.rect(x0, 0.0, (x1 - x0).max(0.0), h);
        c.clip();
        f(c);
        c.restore();
    };
    if let Some(a) = a {
        clipped(c, x, w, &|c| layer(c, a, Style { dy: -6.0 * p * k, blur: ip(p, &[0.1, 1.0], &[0.0, 4.0], Linear), ..Default::default() }));
    }
    clipped(c, 0.0, x, &|c| layer(c, b, Style { dy: (3.0 - 3.0 * p) * k, blur: ip(p, &[0.0, 0.8], &[2.0, 0.0], Linear), ..Default::default() }));
    // The caret, with its glow.
    let alpha = ip(p, &[0.0, 0.08, 0.92, 1.0], &[0.0, 1.0, 1.0, 0.0], Linear);
    if alpha > 0.0 {
        let (cw, ch) = (3.0 * k, h * 0.5);
        let mut glow = Canvas::new(c.width(), c.height());
        glow.set_fill_color(crate::render::canvas::hex("#C3E88D"));
        glow.begin_path();
        glow.round_rect(x - cw / 2.0, h * 0.25, cw, ch, cw);
        glow.fill();
        let bar = glow.into_pixmap();
        let halo = blur_of(&bar, 12.0);
        c.save();
        c.set_alpha(alpha);
        c.draw_image(&halo, 0.0, 0.0);
        c.draw_image(&halo, 0.0, 0.0);
        c.draw_image(&bar, 0.0, 0.0);
        c.restore();
    }
}

/// remotion's spring from 0 to 1 (no initial velocity), at `t` seconds.
fn spring(t: f32, mass: f32, stiffness: f32, damping: f32) -> f32 {
    let w0 = (stiffness / mass).sqrt();
    let z = damping / (2.0 * (stiffness * mass).sqrt());
    let x = if z < 1.0 {
        let wd = w0 * (1.0 - z * z).sqrt();
        1.0 - (-z * w0 * t).exp() * ((wd * t).cos() + z * w0 / wd * (wd * t).sin())
    } else if z == 1.0 {
        1.0 - (-w0 * t).exp() * (1.0 + w0 * t)
    } else {
        let wd = w0 * (z * z - 1.0).sqrt();
        1.0 - (-z * w0 * t).exp() * ((wd * t).cosh() + z * w0 / wd * (wd * t).sinh())
    };
    x
}

/// remocn's slide swap: the outgoing scene eases off to the left and fades, then the incoming
/// one springs in from the right. 22 + 30 frames at 30 fps, fitted to the transition.
pub fn slide_swap(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let w = c.w();
    let f = p * 52.0;
    if f < 22.0 {
        let t = f / 22.0;
        exit(c, a, Style { dx: -0.1 * w * t.powi(5), opacity: 1.0 - t * t, ..Default::default() });
    } else {
        let e = f - 22.0 + 1.0;
        let s = spring(e / 30.0, 0.5, 300.0, 60.0).min(1.0);
        let fade = (e / 24.0).clamp(0.0, 1.0);
        layer(c, b, Style { dx: 0.28 * w * (1.0 - s), opacity: 1.0 - (1.0 - fade) * (1.0 - fade), ..Default::default() });
    }
}

/// remocn's spring settle: the outgoing scene drops back and vanishes, then the incoming one
/// lands from slightly too big and settles on a spring.
pub fn spring_settle(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let f = p * 30.0;
    if f < 6.0 {
        let e = (f / 6.0).powi(5);
        exit(c, a, st(1.0 - e, 1.0 - 0.16 * e, 0.0));
    } else if f >= 7.0 {
        let local = f - 7.0;
        let s = spring(local / 30.0, 1.0, 320.0, 30.0);
        layer(c, b, st((local / 5.0).clamp(0.0, 1.0), 1.24 - 0.24 * s, 0.0));
    }
}

// Lucide icon outlines remocn scatters (ISC licence), in a 24-unit box.
const ICONS: [&str; 11] = [
    "M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z",
    "M2 9.5a5.5 5.5 0 0 1 9.591-3.676.56.56 0 0 0 .818 0A5.49 5.49 0 0 1 22 9.5c0 2.29-1.5 4-3 5.5l-5.492 5.313a2 2 0 0 1-3 .019L5 15c-1.5-1.5-3-3.2-3-5.5",
    "M4 14a1 1 0 0 1-.78-1.63l9.9-10.2a.5.5 0 0 1 .86.46l-1.92 6.02A1 1 0 0 0 13 10h7a1 1 0 0 1 .78 1.63l-9.9 10.2a.5.5 0 0 1-.86-.46l1.92-6.02A1 1 0 0 0 11 14z",
    "M11.017 2.814a1 1 0 0 1 1.966 0l1.051 5.558a2 2 0 0 0 1.594 1.594l5.558 1.051a1 1 0 0 1 0 1.966l-5.558 1.051a2 2 0 0 0-1.594 1.594l-1.051 5.558a1 1 0 0 1-1.966 0l-1.051-5.558a2 2 0 0 0-1.594-1.594l-5.558-1.051a1 1 0 0 1 0-1.966l5.558-1.051a2 2 0 0 0 1.594-1.594z",
    "M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401",
    "M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z",
    "M11.562 3.266a.5.5 0 0 1 .876 0L15.39 8.87a1 1 0 0 0 1.516.294L21.183 5.5a.5.5 0 0 1 .798.519l-2.834 10.246a1 1 0 0 1-.956.734H5.81a1 1 0 0 1-.957-.734L2.02 6.02a.5.5 0 0 1 .798-.519l4.276 3.664a1 1 0 0 0 1.516-.294z",
    "M12 2 A10 10 0 0 1 22 12 A10 10 0 0 1 12 22 A10 10 0 0 1 2 12 A10 10 0 0 1 12 2",
    "M7 4h10a3 3 0 0 1 3 3v10a3 3 0 0 1-3 3H7a3 3 0 0 1-3-3V7a3 3 0 0 1 3-3z",
    "M13.73 4a2 2 0 0 0-3.46 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3z",
    "M12 5v14 M5 12h14",
];

/// Flatten an SVG path into polylines (one per subpath).
fn flatten(d: &str) -> Vec<Vec<(f32, f32)>> {
    let mut nums: Vec<String> = vec![];
    let mut toks: Vec<(char, Vec<f32>)> = vec![];
    let mut cmd = ' ';
    let flush = |cmd: char, nums: &mut Vec<String>, toks: &mut Vec<(char, Vec<f32>)>| {
        if cmd != ' ' {
            toks.push((cmd, nums.drain(..).filter_map(|n| n.parse().ok()).collect()));
        }
    };
    let mut cur = String::new();
    let chars: Vec<char> = d.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch.is_ascii_alphabetic() && ch != 'e' {
            if !cur.is_empty() {
                nums.push(std::mem::take(&mut cur));
            }
            flush(cmd, &mut nums, &mut toks);
            cmd = ch;
        } else if ch == '-' && !cur.is_empty() && !cur.ends_with('e') {
            nums.push(std::mem::take(&mut cur));
            cur.push(ch);
        } else if ch == '.' && cur.contains('.') {
            nums.push(std::mem::take(&mut cur));
            cur.push(ch);
        } else if ch == ',' || ch.is_whitespace() {
            if !cur.is_empty() {
                nums.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(ch);
        }
        i += 1;
    }
    if !cur.is_empty() {
        nums.push(cur);
    }
    flush(cmd, &mut nums, &mut toks);

    let mut out: Vec<Vec<(f32, f32)>> = vec![];
    let (mut x, mut y, mut sx, mut sy) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    let mut line: Vec<(f32, f32)> = vec![];
    for (c, v) in toks {
        let rel = c.is_ascii_lowercase();
        let (ox, oy) = if rel { (x, y) } else { (0.0, 0.0) };
        match c.to_ascii_uppercase() {
            'M' => {
                for (j, pt) in v.chunks_exact(2).enumerate() {
                    let (nx, ny) = (pt[0] + if rel { x } else { 0.0 }, pt[1] + if rel { y } else { 0.0 });
                    x = nx;
                    y = ny;
                    if j == 0 {
                        if line.len() > 1 {
                            out.push(std::mem::take(&mut line));
                        }
                        line.clear();
                        sx = x;
                        sy = y;
                    }
                    line.push((x, y));
                }
            }
            'L' => {
                for pt in v.chunks_exact(2) {
                    x = pt[0] + if rel { x } else { 0.0 };
                    y = pt[1] + if rel { y } else { 0.0 };
                    line.push((x, y));
                }
            }
            'H' => {
                for n in v {
                    x = n + if rel { x } else { 0.0 };
                    line.push((x, y));
                }
            }
            'V' => {
                for n in v {
                    y = n + if rel { y } else { 0.0 };
                    line.push((x, y));
                }
            }
            'C' => {
                for q in v.chunks_exact(6) {
                    let (ox, oy) = if rel { (x, y) } else { (0.0, 0.0) };
                    let (x1, y1, x2, y2, x3, y3) = (q[0] + ox, q[1] + oy, q[2] + ox, q[3] + oy, q[4] + ox, q[5] + oy);
                    for s in 1..=12 {
                        let t = s as f32 / 12.0;
                        let u = 1.0 - t;
                        line.push((
                            u * u * u * x + 3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t * x3,
                            u * u * u * y + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y3,
                        ));
                    }
                    x = x3;
                    y = y3;
                }
            }
            'A' => {
                for q in v.chunks_exact(7) {
                    let (ox, oy) = if rel { (x, y) } else { (0.0, 0.0) };
                    let (x2, y2) = (q[5] + ox, q[6] + oy);
                    arc_points(&mut line, (x, y), (x2, y2), q[0], q[1], q[2], q[3] != 0.0, q[4] != 0.0);
                    x = x2;
                    y = y2;
                }
            }
            'Z' => {
                line.push((sx, sy));
                x = sx;
                y = sy;
            }
            _ => {}
        }
        let _ = (ox, oy);
    }
    if line.len() > 1 {
        out.push(line);
    }
    out
}

/// SVG endpoint arc to points (spec F.6.5).
#[allow(clippy::too_many_arguments)]
fn arc_points(out: &mut Vec<(f32, f32)>, (x1, y1): (f32, f32), (x2, y2): (f32, f32), rx: f32, ry: f32, rot: f32, large: bool, sweep: bool) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-6 || ry < 1e-6 {
        out.push((x2, y2));
        return;
    }
    let phi = rot.to_radians();
    let (cp, sp) = (phi.cos(), phi.sin());
    let (dx, dy) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
    let (x1p, y1p) = (cp * dx + sp * dy, -sp * dx + cp * dy);
    let l = x1p * x1p / (rx * rx) + y1p * y1p / (ry * ry);
    if l > 1.0 {
        rx *= l.sqrt();
        ry *= l.sqrt();
    }
    let num = (rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p).max(0.0);
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut co = (num / den.max(1e-9)).sqrt();
    if large == sweep {
        co = -co;
    }
    let (cxp, cyp) = (co * rx * y1p / ry, -co * ry * x1p / rx);
    let (cx, cy) = (cp * cxp - sp * cyp + (x1 + x2) / 2.0, sp * cxp + cp * cyp + (y1 + y2) / 2.0);
    let ang = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        a
    };
    let t1 = ang(1.0, 0.0, (x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut dt = ang((x1p - cxp) / rx, (y1p - cyp) / ry, (-x1p - cxp) / rx, (-y1p - cyp) / ry);
    if !sweep && dt > 0.0 {
        dt -= 2.0 * PI;
    } else if sweep && dt < 0.0 {
        dt += 2.0 * PI;
    }
    let n = ((dt.abs() / (PI / 16.0)).ceil() as usize).max(2);
    for s in 1..=n {
        let t = t1 + dt * s as f32 / n as f32;
        let (ex, ey) = (rx * t.cos(), ry * t.sin());
        out.push((cp * ex - sp * ey + cx, sp * ex + cp * ey + cy));
    }
}

fn icon_lines() -> &'static Vec<Vec<Vec<(f32, f32)>>> {
    static L: std::sync::OnceLock<Vec<Vec<Vec<(f32, f32)>>>> = std::sync::OnceLock::new();
    L.get_or_init(|| ICONS.iter().map(|d| flatten(d)).collect())
}

/// Stroke the first `frac` of an icon's length (SVG's dash-offset draw-on).
fn stroke_part(c: &mut Canvas, lines: &[Vec<(f32, f32)>], frac: f32) {
    let len = |l: &Vec<(f32, f32)>| l.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).sum::<f32>();
    let total: f32 = lines.iter().map(len).sum();
    let mut left = total * frac;
    c.begin_path();
    for l in lines {
        if left <= 0.0 {
            break;
        }
        c.move_to(l[0].0, l[0].1);
        for w in l.windows(2) {
            let seg = ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt();
            if seg >= left {
                let t = left / seg.max(1e-6);
                c.line_to(w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t);
                left = 0.0;
                break;
            }
            c.line_to(w[1].0, w[1].1);
            left -= seg;
        }
    }
    c.stroke();
}

/// Outline icons draw on across a dark cover, then fly apart as the incoming scene arrives.
pub fn icon_scatter(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h) = (c.w(), c.h());
    let k = w.max(h) / 1280.0;
    exit(c, a, st(ip(p, &[0.28, 0.5], &[1.0, 0.0], Linear), ip(p, &[0.0, 0.5], &[1.0, 0.94], InCubic), ip(p, &[0.1, 0.45], &[0.0, 10.0], Linear)));
    layer(c, b, st(ip(p, &[0.5, 0.68], &[0.0, 1.0], Linear), ip(p, &[0.5, 1.0], &[1.06, 1.0], OutCubic), ip(p, &[0.5, 0.72], &[8.0, 0.0], Linear)));
    let cover = ip(p, &[0.0, 0.3, 0.62, 0.9], &[0.0, 1.0, 1.0, 0.0], Linear) * 0.92;
    if cover > 0.0 {
        c.save();
        c.set_alpha(cover);
        c.fill_rect_color(0.0, 0.0, w, h, crate::render::canvas::hex("#0a0a0a"));
        c.restore();
    }
    let count: usize = 15;
    let cols = ((count as f32 * 16.0 / 9.0).sqrt()).ceil() as usize;
    let rows = count.div_ceil(cols);
    let mut r = crate::model::time::rng(91);
    let lines = icon_lines();
    for i in 0..count {
        let (cw, chh) = (100.0 / cols as f32, 100.0 / rows as f32);
        let xp = cw * ((i % cols) as f32 + 0.5) + (r.next() - 0.5) * cw * 0.7;
        let yp = chh * ((i / cols) as f32 + 0.5) + (r.next() - 0.5) * chh * 0.7;
        let icon = &lines[(r.next() * ICONS.len() as f32) as usize % ICONS.len()];
        let size = 34.0 + r.next() * 40.0;
        let spin = (r.next() - 0.5) * 320.0;
        let dir = (r.next() - 0.5) * 1.2;
        let stroke_op = 0.5 + r.next() * 0.5;
        let p_start = r.next() * 0.16;
        let p_out = 0.52 + r.next() * 0.16;
        let draw = ip(p, &[p_start, p_start + 0.34], &[0.0, 1.0], OutCubic);
        // Easing.out(Easing.back(1.6))
        let tb = ((p - p_start) / 0.3).clamp(0.0, 1.0);
        let back = { let u = 1.0 - tb; 1.0 - u * u * ((1.6 + 1.0) * u - 1.6) };
        let scale_in = 0.4 + 0.6 * back;
        let out_p = ip(p, &[p_out, 1.0], &[0.0, 1.0], InCubic);
        let ang = (yp - 50.0).atan2(xp - 50.0) + dir;
        let fly = out_p * 260.0 * k;
        let turn = spin * ip(p, &[p_start, 1.0], &[0.0, 1.0], Linear);
        let opacity = draw.min(1.0 - out_p) * stroke_op;
        if opacity <= 0.001 {
            continue;
        }
        c.save();
        c.translate(xp / 100.0 * w + ang.cos() * fly, yp / 100.0 * h + ang.sin() * fly);
        let s = size * k / 24.0 * scale_in * (1.0 - out_p * 0.3);
        c.scale(s, s);
        c.rotate(turn.to_radians());
        c.translate(-12.0, -12.0);
        c.set_alpha(opacity);
        c.set_stroke_color(crate::render::canvas::hex("#fafafa"));
        c.set_line_width(2.0);
        c.set_line_cap(tiny_skia::LineCap::Round);
        stroke_part(c, icon, draw);
        c.restore();
    }
}

/// remocn's lens zoom: the outgoing scene is sucked into a wide-angle lens (magnifying, spinning,
/// zoom-blurred), and the incoming one is thrown back out of it, with an RGB scale split and a
/// handheld shake around the cut.
pub fn lens_zoom(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, t: f32) {
    use super::gpu::{gpu, U};
    let exiting = p < 0.5;
    let Some(scene) = (if exiting { a } else { Some(b) }) else { return };
    let q = if exiting { ip(p, &[0.0, 0.5], &[0.0, 1.0], Bezier(0.54, 0.0, 0.82, 0.8)) } else { ip(p, &[0.5, 1.0], &[0.0, 1.0], Bezier(0.18, 0.2, 0.46, 1.0)) };
    let l = |a: f32, b: f32| a + (b - a) * q;
    let (scale, fov, blur, rot) = if exiting { (l(100.0, 150.0), l(0.0, 160.0), l(0.0, 60.0), l(0.0, -15.0)) } else { (l(135.0, 100.0), l(135.0, 0.0), l(45.0, 0.0), l(345.0, 360.0)) };
    // Shake: two octaves of value noise, enveloped around the cut.
    let env = ip(p, &[0.5 - 9.0 / 22.0, 0.5 - 3.0 / 22.0, 0.5 + 4.0 / 22.0, 0.5 + 8.0 / 22.0], &[0.0, 50.0, 50.0, 0.0], Linear) / 100.0;
    let hashed = |seed: u32, i: i64| {
        let mut x = ((i as u32) ^ seed).wrapping_mul(0x27d4eb2d);
        x ^= x >> 15;
        x = x.wrapping_mul(0x85ebca6b);
        x ^= x >> 13;
        x as f32 / 4294967296.0 * 2.0 - 1.0
    };
    let noise = |seed: u32, t: f32| {
        let i = t.floor();
        let f = t - i;
        let u = f * f * (3.0 - 2.0 * f);
        hashed(seed, i as i64) * (1.0 - u) + hashed(seed, i as i64 + 1) * u
    };
    let st_ = t * 1.4 * 8.0;
    let octave = |seed: u32| noise(seed, st_) + 0.45 * noise(seed + 977, st_ * 2.6);
    let (w, h) = (c.width(), c.height());
    let k = w.max(h) as f32 / 1280.0;
    let samples = if blur <= 0.05 { 1.0 } else { (7.0 * blur / 60.0).round().max(2.0) };
    let u = [
        ("u_res", U::V2([w as f32, h as f32])),
        ("u_base", U::F(scale / 100.0)),
        ("u_theta", U::F(fov.min(178.0) / 2.0 * PI / 180.0)),
        ("u_blur", U::F(blur)),
        ("u_samples", U::F(samples)),
        ("u_fade", U::F(1.5)),
        ("u_rot", U::F(rot.to_radians())),
        ("u_shake", U::V2([octave(11) * 73.0 * env * k, octave(23) * 73.0 * env * k])),
        ("u_shake_rot", U::F((octave(41) * 1.1 * env).to_radians())),
    ];
    if let Some(out) = gpu().and_then(|g| g.run("rayna:lens-zoom", include_str!("../../assets/rayna/lens-zoom.frag"), &Default::default(), w, h, &[("u_scene", scene)], &u)) {
        c.draw_image(&out, 0.0, 0.0);
    }
}

/// remocn's ascii dissolve: the outgoing scene fades as a field of monospace glyphs rises over
/// it, holds long enough to read as text, then drops back down the ramp off the incoming scene.
pub fn ascii_dissolve(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    use crate::render::text::{glyph_tile, TextStyle};
    const RAMP: [&str; 10] = [" ", ".", ":", "-", "=", "+", "*", "#", "%", "@"];
    let (w, h) = (c.w(), c.h());
    let k = w.max(h) / 1280.0;
    exit(c, a, st(ip(p, &[0.14, 0.3], &[1.0, 0.0], Linear), 1.0, ip(p, &[0.14, 0.36], &[0.0, 8.0], Linear)));
    let enter_blur = ip(p, &[0.62, 0.85], &[10.0, 0.0], Linear);
    layer(c, b, st(ip(p, &[0.62, 0.8], &[0.0, 1.0], Linear), ip(p, &[0.62, 1.0], &[1.04, 1.0], OutCubic), if enter_blur > 0.01 { enter_blur } else { 0.0 }));
    let field = ip(p, &[0.04, 0.2, 0.66, 0.94], &[0.0, 1.0, 1.0, 0.0], Linear);
    if field <= 0.001 {
        return;
    }
    let coverage = ip(p, &[0.06, 0.28, 0.6, 0.9], &[0.0, 1.0, 1.0, 0.0], Linear);
    c.save();
    c.set_alpha(field);
    c.fill_rect_color(0.0, 0.0, w, h, crate::render::canvas::hex("#0d0d10"));
    c.restore();
    // Glyph cells: rows of `cell` px, columns one mono advance wide.
    let cell = 22.0 * k;
    let size = cell * 0.86;
    let adv = size * 0.6;
    let tile = cell.ceil() as u32;
    let style = TextStyle { family: "JetBrains Mono", weight: 400.0, size, spacing: 0.0 };
    let tiles: Vec<Pixmap> = RAMP.iter().map(|g| glyph_tile(g, tile, &style, 0.0)).collect();
    let cols = (w / adv).ceil() as i32;
    let rows = (h / cell).ceil() as i32 + 1;
    let (cx, cy) = (cols as f32 / 2.0, rows as f32 / 2.0);
    let t = p * 5.0;
    c.save();
    // Translucent ink, as remocn's rgba(242,242,242,0.6).
    c.set_alpha(field * 0.6 * (242.0 / 255.0));
    for y in 0..rows {
        for x in 0..cols {
            let (xf, yf) = (x as f32, y as f32);
            let base = 0.5
                + ((xf * 0.33 + t * 1.7).sin() + (yf * 0.51 - t * 1.2).sin() + ((xf * 0.5 + yf) * 0.24 + t * 0.9).sin() + (((xf - cx).hypot((yf - cy) * 1.8)) * 0.42 - t * 2.1).sin()) / 8.0;
            let jitter = {
                let mut r = crate::model::time::rng((x as i64) * 7919 + (y as i64) * 104729 + 17);
                r.next();
                r.next()
            };
            let d = ((base * 0.72 + jitter * 0.28) * coverage * 1.2 - 0.08).clamp(0.0, 0.999);
            let i = (d * RAMP.len() as f32) as usize;
            if i == 0 {
                continue;
            }
            c.draw_image(&tiles[i], xf * adv + adv / 2.0 - tile as f32 / 2.0, yf * cell);
        }
    }
    c.restore();
}
