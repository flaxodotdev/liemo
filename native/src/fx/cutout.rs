// Cutouts: "Remove background" (the subject, found by the device's AI) and chroma key (a green
// or blue screen), with the finishing touches phone editors offer: soft or tight edges, an
// outline around the subject, a drop shadow, and what goes behind (nothing, a colour, a blurred
// copy of the shot, or the shot dimmed).

use super::engines::blur_of;
use super::registry::{flag, num, text};
use crate::model::types::Params;
use crate::render::canvas::{hex_rgba, pixmap, resize_bilinear, Canvas};
use parking_lot::RwLock;
use std::sync::OnceLock;
use tiny_skia::{BlendMode, Pixmap};

/// Finds the subject in a frame: returns a mask (0–255 per pixel, row by row) at any size.
pub type Segmenter = Box<dyn Fn(&Pixmap) -> Option<(Vec<u8>, u32, u32)> + Send + Sync>;

fn slot() -> &'static RwLock<Option<Segmenter>> {
    static S: OnceLock<RwLock<Option<Segmenter>>> = OnceLock::new();
    S.get_or_init(|| RwLock::new(None))
}
/// The app plugs in what finds subjects on this device (Apple's Vision on iPhone, iPad and Mac).
pub fn set_segmenter(f: Segmenter) {
    *slot().write() = Some(f);
}

/// The subject mask at the frame's size, as an alpha-only pixmap.
fn subject_mask(src: &Pixmap) -> Option<Pixmap> {
    let s = slot().read();
    let (m, mw, mh) = (s.as_ref()?)(src)?;
    if m.len() < (mw * mh) as usize || mw == 0 || mh == 0 {
        return None;
    }
    let mut small = pixmap(mw, mh);
    for (px, a) in small.data_mut().chunks_exact_mut(4).zip(m.iter()) {
        px.copy_from_slice(&[*a, *a, *a, *a]);
    }
    Some(if (mw, mh) == (src.width(), src.height()) { small } else { resize_bilinear(&small, src.width(), src.height()) })
}

/// A chroma-key mask: how far each pixel's colour is from the key colour.
fn key_mask(src: &Pixmap, p: &Params) -> Pixmap {
    let [kr, kg, kb, _] = hex_rgba(text(p, "color", "#00ff00"));
    let sim = num(p, "similarity", 0.4) as f32;
    let smooth = (num(p, "smoothness", 0.08) as f32).max(0.001);
    // Compare in chroma (colour without brightness), as keyers do, so shadows on the screen key too.
    let chroma = |r: f32, g: f32, b: f32| {
        let y = 0.299 * r + 0.587 * g + 0.114 * b;
        ((b - y) * 0.565, (r - y) * 0.713)
    };
    let (kcb, kcr) = chroma(kr as f32 / 255.0, kg as f32 / 255.0, kb as f32 / 255.0);
    let mut m = pixmap(src.width(), src.height());
    for (o, i) in m.data_mut().chunks_exact_mut(4).zip(src.data().chunks_exact(4)) {
        let a = i[3] as f32 / 255.0;
        let a_div = a.max(1e-3) * 255.0;
        let (cb, cr) = chroma(i[0] as f32 / a_div, i[1] as f32 / a_div, i[2] as f32 / a_div);
        let d = ((cb - kcb).powi(2) + (cr - kcr).powi(2)).sqrt() / 0.7;
        let keep = ((d - sim * 0.6) / smooth).clamp(0.0, 1.0) * a;
        let v = (keep * 255.0) as u8;
        o.copy_from_slice(&[v, v, v, v]);
    }
    m
}

/// Soften, then shrink or grow, the mask's edge.
fn shape_edge(mask: Pixmap, feather: f32, edge: f32) -> Pixmap {
    let mut m = if feather > 0.2 { blur_of(&mask, feather) } else { mask };
    if edge.abs() > 0.01 || feather > 0.2 {
        // Moving the threshold of a soft edge moves the edge itself: positive grows, negative shrinks.
        let t = (0.5 - edge * 0.45).clamp(0.02, 0.98);
        let w = if feather > 0.2 { 0.5 } else { 0.08 };
        for px in m.data_mut().chunks_exact_mut(4) {
            let a = px[3] as f32 / 255.0;
            let v = (((a - t) / w) + 0.5).clamp(0.0, 1.0);
            let v = (v * 255.0) as u8;
            px.copy_from_slice(&[v, v, v, v]);
        }
    }
    m
}

/// Keep `src` only where `mask` is, then add the outline, shadow and background.
fn finish(src: &Pixmap, mask: &Pixmap, p: &Params) -> Pixmap {
    let (w, h) = (src.width(), src.height());
    let u = w.max(h) as f32 / 1280.0;
    let mut subject = Canvas::from_pixmap(src.clone());
    subject.composite(mask, 1.0, BlendMode::DestinationIn);
    let subject = subject.into_pixmap();

    let mut out = Canvas::from_pixmap(pixmap(w, h));
    match text(p, "background", "none") {
        "color" => out.fill_all(crate::render::canvas::hex(text(p, "bgColor", "#ffffff"))),
        "blur" => {
            let b = blur_of(src, 30.0);
            out.draw_image(&b, 0.0, 0.0);
        }
        "dim" => {
            out.draw_image(src, 0.0, 0.0);
            out.fill_rect_color(0.0, 0.0, w as f32, h as f32, crate::render::canvas::rgba(0, 0, 0, 0.6));
        }
        _ => {}
    }
    // The shadow: the subject's shape, dark and soft, dropped down and to the right.
    let shadow = num(p, "shadow", 0.0) as f32;
    if shadow > 0.0 {
        let soft = blur_of(mask, 10.0);
        let mut dark = Canvas::from_pixmap(pixmap(w, h));
        dark.fill_all(tiny_skia::Color::BLACK);
        dark.composite(&soft, 1.0, BlendMode::DestinationIn);
        out.save();
        out.set_alpha(shadow * 0.8);
        out.draw_image(&dark.into_pixmap(), 12.0 * u, 16.0 * u);
        out.restore();
    }
    // The outline: the shape grown by the width, in the outline colour, under the subject.
    let stroke = num(p, "outline", 0.0) as f32;
    if stroke > 0.0 {
        let grown = shape_edge(blur_of(mask, stroke * 0.7), 0.0, 0.9);
        let mut ring = Canvas::from_pixmap(pixmap(w, h));
        ring.fill_all(crate::render::canvas::hex(text(p, "outlineColor", "#ffffff")));
        ring.composite(&grown, 1.0, BlendMode::DestinationIn);
        out.composite(&ring.into_pixmap(), 1.0, BlendMode::SourceOver);
    }
    out.composite(&subject, 1.0, BlendMode::SourceOver);
    out.into_pixmap()
}

/// Run a cutout effect over `src`.
pub fn run(id: &str, src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    let mask = match id {
        "chroma-key" => Some(key_mask(src, p)),
        _ => subject_mask(src),
    };
    let Some(mask) = mask else {
        // No AI on this device: leave the picture as it is.
        *dst = src.clone();
        return;
    };
    let mask = shape_edge(mask, num(p, "feather", 1.0) as f32, num(p, "edge", 0.0) as f32);
    let mut out = finish(src, &mask, p);
    if id == "chroma-key" && num(p, "spill", 0.5) > 0.0 {
        despill(&mut out, p);
    }
    if flag(p, "invert") {
        // Keep the background instead of the subject.
        let mut inv = Canvas::from_pixmap(src.clone());
        let mut hole = mask.clone();
        for px in hole.data_mut().chunks_exact_mut(4) {
            let v = 255 - px[3];
            px.copy_from_slice(&[v, v, v, v]);
        }
        inv.composite(&hole, 1.0, BlendMode::DestinationIn);
        out = inv.into_pixmap();
    }
    *dst = out;
}

/// Take the key colour's cast off what's left (green fringes on hair and edges).
fn despill(pm: &mut Pixmap, p: &Params) {
    let [kr, kg, kb, _] = hex_rgba(text(p, "color", "#00ff00"));
    let amount = num(p, "spill", 0.5) as f32;
    // The key's dominant channel is pulled down to the others' level.
    let ch = if kg >= kr && kg >= kb { 1 } else if kb >= kr { 2 } else { 0 };
    for px in pm.data_mut().chunks_exact_mut(4) {
        let others = [0usize, 1, 2].iter().filter(|c| **c != ch).map(|c| px[*c] as f32).fold(0.0f32, f32::max);
        let v = px[ch] as f32;
        if v > others {
            px[ch] = (v - (v - others) * amount) as u8;
        }
    }
}
