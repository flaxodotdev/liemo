// Transitions between two clips on the same track. `a` is the outgoing clip (or None when the
// clip transitions in from what's below), `b` the incoming one; both are already-rendered frames
// the size of the target. `p` runs 0 → 1 across the transition.

use crate::model::time::rng;
use crate::render::canvas::{hex, resample_area, resize_bilinear, resize_nearest, rgba, Canvas};
use std::f32::consts::PI;
use tiny_skia::Pixmap;

pub type DrawFn = fn(&mut Canvas, Option<&Pixmap>, &Pixmap, f32, f32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionGroup {
    Dissolve,
    Motion,
    Shape,
    Stylized,
    Shader,
}
pub const TRANSITION_GROUPS: [(TransitionGroup, &str); 5] = [
    (TransitionGroup::Dissolve, "Dissolves"),
    (TransitionGroup::Motion, "Motion"),
    (TransitionGroup::Shape, "Wipes & shapes"),
    (TransitionGroup::Stylized, "Stylized"),
    (TransitionGroup::Shader, "Shader transitions"),
];

pub struct TransitionDef {
    pub id: &'static str,
    pub name: &'static str,
    pub group: TransitionGroup,
    pub draw: DrawFn,
}

fn ease(p: f32) -> f32 {
    if p < 0.5 { 4.0 * p * p * p } else { 1.0 - (-2.0 * p + 2.0).powi(3) / 2.0 }
}

fn with_alpha(c: &mut Canvas, alpha: f32, f: impl FnOnce(&mut Canvas)) {
    if alpha <= 0.0 {
        return;
    }
    c.save();
    let a = c.alpha() * alpha.min(1.0);
    c.set_alpha(a);
    f(c);
    c.restore();
}
fn clip_to(c: &mut Canvas, path: impl FnOnce(&mut Canvas), f: impl FnOnce(&mut Canvas)) {
    c.save();
    c.begin_path();
    path(c);
    c.clip();
    f(c);
    c.restore();
}
fn draw(c: &mut Canvas, f: Option<&Pixmap>, x: f32, y: f32) {
    if let Some(f) = f {
        c.draw_image(f, x, y);
    }
}
fn draw_sized(c: &mut Canvas, f: Option<&Pixmap>, x: f32, y: f32, w: f32, h: f32) {
    if let Some(f) = f {
        c.draw_image_scaled(f, x, y, w, h);
    }
}

pub fn blurred(f: &Pixmap, amount: f32) -> Pixmap {
    let (w, h) = (f.width(), f.height());
    let k = 1.0 + amount * 18.0;
    let small = resample_area(f, ((w as f32 / k).round() as u32).max(1), ((h as f32 / k).round() as u32).max(1));
    resize_bilinear(&small, w, h)
}
fn pixelated(f: &Pixmap, cells: f32) -> Pixmap {
    let (w, h) = (f.width(), f.height());
    let cols = cells.round().max(1.0) as u32;
    let rows = ((cells * h as f32) / w as f32).round().max(1.0) as u32;
    resize_nearest(&resample_area(f, cols, rows), w, h)
}

fn push(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, dx: f32, dy: f32) {
    let (w, h, e) = (c.w(), c.h(), ease(p));
    draw(c, a, -dx * w * e, -dy * h * e);
    draw(c, Some(b), dx * w * (1.0 - e), dy * h * (1.0 - e));
}

fn dissolve(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    draw(c, a, 0.0, 0.0);
    with_alpha(c, p, |c| draw(c, Some(b), 0.0, 0.0));
}
fn dip_black(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h) = (c.w(), c.h());
    if p < 0.5 {
        with_alpha(c, 1.0 - p * 2.0, |c| draw(c, a, 0.0, 0.0));
    } else {
        with_alpha(c, p * 2.0 - 1.0, |c| draw(c, Some(b), 0.0, 0.0));
    }
    c.save();
    c.set_alpha(1.0 - (p * 2.0 - 1.0).abs());
    c.fill_rect_color(0.0, 0.0, w, h, hex("#000"));
    c.restore();
}
fn dip_white(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h) = (c.w(), c.h());
    if p < 0.5 { draw(c, a, 0.0, 0.0) } else { draw(c, Some(b), 0.0, 0.0) }
    c.save();
    c.set_alpha(1.0 - (p * 2.0 - 1.0).abs());
    c.fill_rect_color(0.0, 0.0, w, h, hex("#fff"));
    c.restore();
}
fn blur(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    if let Some(a) = a {
        let ba = blurred(a, p);
        with_alpha(c, 1.0 - p, |c| c.draw_image(&ba, 0.0, 0.0));
    }
    let bb = blurred(b, 1.0 - p);
    with_alpha(c, p, |c| c.draw_image(&bb, 0.0, 0.0));
}
fn push_left(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    push(c, a, b, p, 1.0, 0.0)
}
fn push_right(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    push(c, a, b, p, -1.0, 0.0)
}
fn push_up(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    push(c, a, b, p, 0.0, 1.0)
}
fn push_down(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    push(c, a, b, p, 0.0, -1.0)
}
fn whip(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    // A fast push with smeared copies for motion blur.
    let (w, e) = (c.w(), ease(p));
    let smear = (p * PI).sin() * w * 0.06;
    for i in 0..5 {
        let o = (i as f32 - 2.0) * smear * 0.5;
        with_alpha(c, if i == 2 { 1.0 } else { 0.25 }, |c| {
            draw(c, a, -w * e + o, 0.0);
            draw(c, Some(b), w * (1.0 - e) + o, 0.0);
        });
    }
}
fn zoom(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h, e) = (c.w(), c.h(), ease(p));
    if a.is_some() {
        with_alpha(c, 1.0 - e, |c| {
            let k = 1.0 + e * 0.6;
            draw_sized(c, a, (w - w * k) / 2.0, (h - h * k) / 2.0, w * k, h * k);
        });
    }
    with_alpha(c, e, |c| {
        let k = 0.7 + e * 0.3;
        draw_sized(c, Some(b), (w - w * k) / 2.0, (h - h * k) / 2.0, w * k, h * k);
    });
}
fn spin(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h, e) = (c.w(), c.h(), ease(p));
    let at = |c: &mut Canvas, f: Option<&Pixmap>, rot: f32, k: f32, alpha: f32| {
        with_alpha(c, alpha, |c| {
            c.translate(w / 2.0, h / 2.0);
            c.rotate(rot);
            c.scale(k, k);
            c.translate(-w / 2.0, -h / 2.0);
            draw(c, f, 0.0, 0.0);
        })
    };
    at(c, a, e * 0.6, 1.0 + e, 1.0 - e);
    at(c, Some(b), (e - 1.0) * 0.6, 2.0 - e, e);
}
fn wipe(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h) = (c.w(), c.h());
    let x = ease(p) * w;
    draw(c, a, 0.0, 0.0);
    clip_to(c, |g| g.rect(0.0, 0.0, x, h), |c| draw(c, Some(b), 0.0, 0.0));
}
/// The next clip arrives in thirds: the top band slides in, then the bottom from the other side,
/// and the middle last completes it.
fn thirds(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h) = (c.w(), c.h());
    draw(c, a, 0.0, 0.0);
    // (band, from, to, direction): overlapping a little so it reads as one move.
    for (band, from, to, dir) in [(0.0f32, 0.0f32, 0.42f32, -1.0f32), (2.0, 0.29, 0.71, 1.0), (1.0, 0.58, 1.0, -1.0)] {
        let q = ((p - from) / (to - from)).clamp(0.0, 1.0);
        if q <= 0.0 {
            continue;
        }
        let e = 1.0 - (1.0 - q).powi(3);
        let y = band * h / 3.0;
        clip_to(c, |g| g.rect(0.0, y, w, h / 3.0 + 1.0), |c| draw(c, Some(b), dir * (1.0 - e) * w, 0.0));
    }
}
fn soft_wipe(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h) = (c.w(), c.h());
    let edge = w * 0.25;
    let x = -edge + ease(p) * (w + edge * 2.0);
    let mut m = Canvas::from_pixmap(b.clone());
    let g = m.linear(x - edge, 0.0, x + edge, 0.0, &[(0.0, hex("#000")), (1.0, rgba(0, 0, 0, 0.0))]);
    m.set_op("destination-in");
    m.set_fill(g);
    m.fill_rect(0.0, 0.0, w, h);
    draw(c, a, 0.0, 0.0);
    c.draw_image(&m.into_pixmap(), 0.0, 0.0);
}
fn barn(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h) = (c.w(), c.h());
    let half = ease(p) * w / 2.0;
    draw(c, a, 0.0, 0.0);
    clip_to(c, |g| g.rect(w / 2.0 - half, 0.0, half * 2.0, h), |c| draw(c, Some(b), 0.0, 0.0));
}
fn blinds(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h) = (c.w(), c.h());
    let n = 9;
    let band = h / n as f32;
    let open = ease(p) * band;
    draw(c, a, 0.0, 0.0);
    clip_to(
        c,
        |g| {
            for i in 0..n {
                g.rect(0.0, i as f32 * band, w, open);
            }
        },
        |c| draw(c, Some(b), 0.0, 0.0),
    );
}
fn clock(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h) = (c.w(), c.h());
    let r = w.hypot(h);
    draw(c, a, 0.0, 0.0);
    clip_to(
        c,
        |g| {
            g.move_to(w / 2.0, h / 2.0);
            g.arc(w / 2.0, h / 2.0, r, -PI / 2.0, -PI / 2.0 + ease(p) * PI * 2.0);
            g.close_path();
        },
        |c| draw(c, Some(b), 0.0, 0.0),
    );
}
fn iris(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let (w, h) = (c.w(), c.h());
    let r = ease(p) * w.hypot(h) / 2.0;
    draw(c, a, 0.0, 0.0);
    clip_to(c, |g| g.circle(w / 2.0, h / 2.0, r.max(0.1)), |c| draw(c, Some(b), 0.0, 0.0));
}
fn pixelate(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
    let w = c.w();
    let k = 1.0 - (p * 2.0 - 1.0).abs();
    let cells = (w / (1.0 + k * 60.0)).max(6.0);
    let f = if p < 0.5 { a } else { Some(b) };
    if let Some(f) = f {
        if k > 0.02 {
            let px = pixelated(f, cells);
            c.draw_image(&px, 0.0, 0.0);
        } else {
            c.draw_image(f, 0.0, 0.0);
        }
    }
}
fn glitch(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, t: f32) {
    let (w, h) = (c.w(), c.h());
    let mut r = rng((t * 30.0).floor() as i64 + 3);
    let k = 1.0 - (p * 2.0 - 1.0).abs();
    let (base, other) = if p < 0.5 { (a, Some(b)) } else { (Some(b), a) };
    draw(c, base, 0.0, 0.0);
    for _ in 0..18 {
        if r.next() > 0.25 + k * 0.7 {
            continue;
        }
        let y = r.next() * h;
        let sh = (r.next() * h * 0.09).max(2.0);
        let dx = (r.next() - 0.5) * w * 0.25 * k;
        let src = if r.next() < k * 0.6 { other } else { base };
        if let Some(src) = src {
            c.draw_image_sub(src, 0.0, y, w, sh, dx, y, w, sh);
        }
    }
    if k > 0.2 {
        c.save();
        c.set_op("lighter");
        c.set_alpha(k * 0.35);
        draw(c, base, 8.0 * k, 0.0);
        c.restore();
    }
}


// remocn's GPU transitions (see remocn.rs).
macro_rules! shader_transition {
    ($f:ident, $id:literal) => {
        fn $f(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _: f32) {
            super::remocn::transition(c, $id, a, b, p)
        }
    };
}
shader_transition!(displacement, "remocn-displacement");
shader_transition!(ember_burn, "remocn-ember-burn");
shader_transition!(glitch_cut, "remocn-glitch-cut");
shader_transition!(grid_ripple, "remocn-grid-wave");
shader_transition!(grid_sweep, "remocn-grid-sweep");
shader_transition!(particles, "remocn-particles");
shader_transition!(particle_drift, "remocn-particles-up");
shader_transition!(shatter, "remocn-shatter");

pub const TRANSITIONS: [TransitionDef; 51] = {
    use TransitionGroup::*;
    [
        TransitionDef { id: "dissolve", name: "Dissolve", group: Dissolve, draw: dissolve },
        TransitionDef { id: "dip-black", name: "Dip to black", group: Dissolve, draw: dip_black },
        TransitionDef { id: "dip-white", name: "Dip to white", group: Dissolve, draw: dip_white },
        TransitionDef { id: "blur", name: "Blur", group: Dissolve, draw: blur },
        TransitionDef { id: "push-left", name: "Push left", group: Motion, draw: push_left },
        TransitionDef { id: "push-right", name: "Push right", group: Motion, draw: push_right },
        TransitionDef { id: "push-up", name: "Push up", group: Motion, draw: push_up },
        TransitionDef { id: "push-down", name: "Push down", group: Motion, draw: push_down },
        TransitionDef { id: "whip", name: "Whip pan", group: Motion, draw: whip },
        TransitionDef { id: "zoom", name: "Zoom", group: Motion, draw: zoom },
        TransitionDef { id: "spin", name: "Spin zoom", group: Motion, draw: spin },
        TransitionDef { id: "wipe", name: "Wipe", group: Shape, draw: wipe },
        TransitionDef { id: "soft-wipe", name: "Soft wipe", group: Shape, draw: soft_wipe },
        TransitionDef { id: "barn", name: "Barn doors", group: Shape, draw: barn },
        TransitionDef { id: "blinds", name: "Blinds", group: Shape, draw: blinds },
        TransitionDef { id: "clock", name: "Clock wipe", group: Shape, draw: clock },
        TransitionDef { id: "iris", name: "Iris", group: Shape, draw: iris },
        TransitionDef { id: "pixelate", name: "Pixelate", group: Stylized, draw: pixelate },
        TransitionDef { id: "glitch", name: "Glitch", group: Stylized, draw: glitch },
        // Drawn by the compositor through the title over the cut (see compositor::text_window);
        // with no title there it's a plain zoom.
        TransitionDef { id: "text-zoom", name: "Text zoom", group: Stylized, draw: zoom },
        TransitionDef { id: "thirds", name: "Thirds", group: Motion, draw: thirds },
        TransitionDef { id: "remocn-displacement", name: "Displacement", group: Shader, draw: displacement },
        TransitionDef { id: "remocn-ember-burn", name: "Ember burn", group: Shader, draw: ember_burn },
        TransitionDef { id: "remocn-glitch-cut", name: "Glitch cut", group: Shader, draw: glitch_cut },
        TransitionDef { id: "remocn-grid-wave", name: "Grid ripple", group: Shader, draw: grid_ripple },
        TransitionDef { id: "remocn-grid-sweep", name: "Grid sweep", group: Shader, draw: grid_sweep },
        TransitionDef { id: "remocn-particles", name: "Particle dissolve", group: Shader, draw: particles },
        TransitionDef { id: "remocn-particles-up", name: "Particle drift", group: Shader, draw: particle_drift },
        TransitionDef { id: "remocn-grain-dissolve", name: "Grain dissolve", group: Shader, draw: super::remocn_tr::grain_dissolve },
        TransitionDef { id: "remocn-dither-dissolve", name: "Dither dissolve", group: Shader, draw: super::remocn_tr::dither_dissolve },
        TransitionDef { id: "remocn-perlin-dissolve", name: "Perlin dissolve", group: Shader, draw: super::remocn_tr::perlin_dissolve },
        TransitionDef { id: "remocn-smoke-dissolve", name: "Smoke dissolve", group: Shader, draw: super::remocn_tr::smoke_dissolve },
        TransitionDef { id: "remocn-swirl-dissolve", name: "Swirl dissolve", group: Shader, draw: super::remocn_tr::swirl_dissolve },
        TransitionDef { id: "remocn-warp-dissolve", name: "Warp dissolve", group: Shader, draw: super::remocn_tr::warp_dissolve },
        TransitionDef { id: "remocn-wave-wipe", name: "Wave wipe", group: Shader, draw: super::remocn_tr::wave_wipe },
        TransitionDef { id: "remocn-ripple-zoom", name: "Ripple zoom", group: Shader, draw: super::remocn_tr::ripple_zoom },
        TransitionDef { id: "remocn-focus-pull", name: "Focus pull", group: Motion, draw: super::remocn_tr::focus_pull },
        TransitionDef { id: "remocn-push-through", name: "Push through", group: Motion, draw: super::remocn_tr::push_through },
        TransitionDef { id: "remocn-smear-pan", name: "Smear pan", group: Motion, draw: super::remocn_tr::smear_pan },
        TransitionDef { id: "remocn-zoom-blur", name: "Zoom blur", group: Motion, draw: super::remocn_tr::zoom_blur },
        TransitionDef { id: "remocn-caret-wipe", name: "Caret wipe", group: Shape, draw: super::remocn_tr::caret_wipe },
        TransitionDef { id: "remocn-slide-swap", name: "Slide swap", group: Motion, draw: super::remocn_tr::slide_swap },
        TransitionDef { id: "remocn-spring-settle", name: "Spring settle", group: Motion, draw: super::remocn_tr::spring_settle },
        TransitionDef { id: "remocn-icon-scatter", name: "Icon scatter", group: Stylized, draw: super::remocn_tr::icon_scatter },
        TransitionDef { id: "remocn-lens-zoom", name: "Lens zoom", group: Shader, draw: super::remocn_tr::lens_zoom },
        TransitionDef { id: "remocn-ascii-dissolve", name: "ASCII dissolve", group: Stylized, draw: super::remocn_tr::ascii_dissolve },
        TransitionDef { id: "remocn-page-turn", name: "Page turn", group: Motion, draw: super::remocn_tr::page_turn },
        TransitionDef { id: "remocn-seam", name: "Seam", group: Shader, draw: super::remocn_tr::seam },
        TransitionDef { id: "remocn-spiral-pass", name: "Spiral pass", group: Shader, draw: super::remocn_tr::spiral_pass },
        TransitionDef { id: "remocn-shatter", name: "Shatter shift", group: Shader, draw: shatter },
        TransitionDef { id: "grid-flash", name: "Grid flash", group: Stylized, draw: grid_flash },
    ]
};

pub fn transition_def(id: &str) -> Option<&'static TransitionDef> {
    TRANSITIONS.iter().find(|t| t.id == id).or_else(|| {
        // Parameterized ids: grid-flash-4x4-random and friends all run the grid flash.
        id.starts_with("grid-flash").then(|| TRANSITIONS.iter().find(|t| t.id == "grid-flash").unwrap())
    })
}

// ---------- Grid flash ----------

/// The order a grid flash lights its cells in: reading down, back up, or shuffled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridOrder {
    Down,
    Up,
    Random,
}

/// A `grid-flash[-COLSxROWS[-down|up|random]]` id: the grid's size and its cell order.
/// Bare `grid-flash` is the default, 3×3 (9 squares) reading from the first cell to the last.
pub fn grid_flash_of(id: &str) -> Option<(u32, u32, GridOrder)> {
    let rest = id.strip_prefix("grid-flash")?;
    let (mut cols, mut rows, mut order) = (3u32, 3u32, GridOrder::Down);
    for part in rest.trim_start_matches('-').split('-') {
        if let Some((c, r)) = part.split_once('x') {
            if let (Ok(c), Ok(r)) = (c.parse::<u32>(), r.parse::<u32>()) {
                if (1..=12).contains(&c) && (1..=12).contains(&r) {
                    (cols, rows) = (c, r);
                }
            }
        } else if part == "up" {
            order = GridOrder::Up;
        } else if part == "random" {
            order = GridOrder::Random;
        }
    }
    Some((cols, rows, order))
}

/// A stable seed for a clip: shuffled orders must be the same every frame, and every
/// clip gets its own shuffle.
pub fn id_seed(id: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.bytes() {
        h = (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

pub fn grid_flash(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, t: f32) {
    grid_flash_p(c, a, b, p, t, 3, 3, GridOrder::Down, 0)
}

/// The frame splits into a grid; each cell holds the outgoing clip until its turn, then a
/// white flash goes off in it and decays fast, revealing that square of the incoming clip.
/// With no outgoing clip the cells just flash in over what's below.
pub fn grid_flash_p(c: &mut Canvas, a: Option<&Pixmap>, b: &Pixmap, p: f32, _t: f32, cols: u32, rows: u32, order: GridOrder, seed: u64) {
    let (w, h) = (c.w(), c.h());
    let n = (cols * rows) as usize;
    let mut cells: Vec<usize> = (0..n).collect();
    match order {
        GridOrder::Down => {}
        GridOrder::Up => cells.reverse(),
        GridOrder::Random => {
            let mut r = rng(seed as i64);
            for i in (1..n).rev() {
                let j = (r.next() * (i + 1) as f32) as usize;
                cells.swap(i, j.min(i));
            }
        }
    }
    let (cw, ch) = (w / cols as f32, h / rows as f32);
    let cols = cols as usize;
    for (i, cell) in cells.iter().enumerate() {
        let (cx, cy) = ((*cell % cols) as f32 * cw, (*cell / cols) as f32 * ch);
        let s0 = i as f32 / n as f32;
        let s1 = (i + 1) as f32 / n as f32;
        clip_to(c, |c| c.rect(cx, cy, cw, ch), |c| {
            if p <= s0 {
                draw(c, a, 0.0, 0.0);
            } else {
                draw(c, Some(b), 0.0, 0.0);
                // The flash fires when the cell's turn comes and decays over its slot,
                // fast, so the square of the next clip shows through.
                let q = ((p - s0) / (s1 - s0)).clamp(0.0, 1.0);
                let flash = (1.0 - q).powi(2);
                if flash > 0.004 {
                    c.set_alpha(flash);
                    c.fill_rect_color(cx, cy, cw, ch, hex("#fff"));
                }
            }
        });
    }
}

