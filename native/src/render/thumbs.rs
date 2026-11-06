// Thumbnails for the library, timeline and project cards.

use super::canvas::{pixmap, resample_area, rgba, Canvas};
use super::compositor::{render_frame, Frames, Scene};
use super::textclip::draw_text;
use crate::fx::engines::run_effects;
use crate::fx::registry::{effect_def, make_clip_effect, ParamKind};
use crate::fx::transitions::transition_def;
use crate::model::assets::{asset, Asset, SCENE_DRAWS};
use crate::model::time::{H, W};
use crate::model::types::*;
use tiny_skia::{Pixmap, Transform};

fn scaled(draw: impl FnOnce(&mut Canvas), w: u32) -> Pixmap {
    // Library pictures are always landscape, whatever project is open.
    crate::model::time::with_canvas((1280.0, 720.0), || scaled16(draw, w))
}
fn scaled16(draw: impl FnOnce(&mut Canvas), w: u32) -> Pixmap {
    let mut c = Canvas::new(W() as u32, H() as u32);
    draw(&mut c);
    resample_area(&c.into_pixmap(), w, (w as f32 * 9.0 / 16.0).round() as u32)
}

/// A library card picture for a built-in or imported asset.
pub fn asset_thumb(a: &Asset) -> Option<Pixmap> {
    if let Some(t) = &a.thumb {
        return Some((**t).clone());
    }
    if let Some(id) = a.shader {
        return crate::fx::paper::render(id, 2.0, 320, 180);
    }
    match a.kind {
        AssetType::Scene | AssetType::Image => {
            let d = a.draw?;
            Some(scaled(|c| d(c, 1.2), 320))
        }
        AssetType::Overlay => {
            let d = a.draw?;
            let blend = a.blend.unwrap_or("source-over");
            Some(scaled(
                |c| {
                    SCENE_DRAWS[3](c, 1.2);
                    c.set_op(blend);
                    d(c, 1.2);
                },
                320,
            ))
        }
        AssetType::Text => {
            let mut t = a.text.clone()?;
            t.anim = TextAnim::None;
            Some(scaled(
                |c| {
                    SCENE_DRAWS[0](c, 1.5);
                    c.fill_rect_color(0.0, 0.0, W(), H(), rgba(0, 0, 0, 0.35));
                    draw_text(c, &t, 2.0, 1.0);
                },
                320,
            ))
        }
        _ => None,
    }
}

pub fn asset_thumb_id(id: &str) -> Option<Pixmap> {
    asset_thumb(&*asset(id)?)
}

/// Thumbnail photographs (Unsplash licence): a portrait for effects, two landscapes for transitions.
const PHOTOS: [&[u8]; 3] = [
    include_bytes!("../../assets/photos/portrait.jpg"),
    include_bytes!("../../assets/photos/beach.jpg"),
    include_bytes!("../../assets/photos/road.jpg"),
];
fn photo(i: usize) -> Pixmap {
    static P: std::sync::OnceLock<Vec<Pixmap>> = std::sync::OnceLock::new();
    P.get_or_init(|| {
        PHOTOS
            .iter()
            .map(|b| {
                let img = image::load_from_memory(b).expect("bundled photo").to_rgba8();
                let (w, h) = img.dimensions();
                resample_area(&crate::render::canvas::from_straight(img.as_raw(), w, h), 384, 216)
            })
            .collect()
    })[i]
        .clone()
}

#[allow(dead_code)]
fn small_scene(scene: usize, t: f32) -> Pixmap {
    let mut c = Canvas::new(384, 216);
    c.set_transform(Transform::from_scale(384.0 / W(), 216.0 / H()));
    SCENE_DRAWS[scene](&mut c, t);
    c.into_pixmap()
}

/// An effect's library card: the coastline scene with the effect at full strength.
pub fn effect_thumb(id: &str) -> Pixmap {
    let src = photo(0);
    let Some(mut fx) = make_clip_effect(id, if id == "blur" { 0.25 } else { 1.0 }) else { return src };
    // Thumbnails are small, so enlarge cells and pixels to keep the look readable.
    if let Some(def) = effect_def(id) {
        for pd in &def.params {
            if let ParamKind::Range { unit: "px", .. } = pd.kind {
                if let Some(ParamValue::Num(v)) = fx.params.get_mut(pd.key) {
                    *v *= 2.2;
                }
            }
        }
    }
    // The power-off card is caught mid-collapse.
    let dur = if id == "remocn-tv-off" { 1.4 + 0.6 * 0.45 } else { 3.0 };
    run_effects(&src, &[fx], 1.4, dur, false)
}

/// A transition's card: two sample scenes, caught halfway through.
pub fn transition_thumb(id: &str) -> Pixmap {
    let p = match id {
        "pixelate" => 0.42,
        "dip-black" | "dip-white" => 0.3,
        _ => 0.5,
    };
    transition_at(id, p)
}
/// A transition's card frame at progress `p`.
pub fn transition_at(id: &str, p: f32) -> Pixmap {
    let mut out = Canvas::new(384, 216);
    out.fill_all(tiny_skia::Color::BLACK);
    if let Some(def) = transition_def(id) {
        let (a, b) = (photo(1), photo(2));
        (def.draw)(&mut out, Some(&a), &b, p, 1.2);
    }
    out.into_pixmap()
}

/// What a format looks like: a photograph (Unsplash licence) cropped to that shape, `px_h` tall.
pub fn format_preview(w: f32, h: f32, px_h: u32) -> Pixmap {
    static LAKE: std::sync::OnceLock<Pixmap> = std::sync::OnceLock::new();
    let src = LAKE.get_or_init(|| {
        let img = image::load_from_memory(include_bytes!("../../assets/photos/lake.jpg")).expect("bundled photo").to_rgba8();
        let (iw, ih) = img.dimensions();
        crate::render::canvas::from_straight(img.as_raw(), iw, ih)
    });
    let pw = ((px_h as f32) * w / h).round().max(1.0) as u32;
    // Crop to the shape first, then shrink with an area filter so it stays crisp.
    let (sw, sh) = (src.width() as f32, src.height() as f32);
    let k = (w / h) / (sw / sh);
    let (cw, ch) = if k > 1.0 { (sw, sw / (w / h)) } else { (sh * (w / h), sh) };
    let crop = crate::render::canvas::crop(src, (sw - cw) / 2.0, (sh - ch) / 2.0, cw, ch);
    resample_area(&crop, pw, px_h)
}

/// A small picture of the edit a quarter of the way in, for project cards.
pub fn stage_snapshot(clips: &[Clip], tracks: &[Track], frames: &mut dyn Frames, w: u32) -> Option<Pixmap> {
    let end = crate::model::store::project_end(clips);
    if end <= 0.0 {
        return None;
    }
    let mut c = Canvas::from_pixmap(pixmap(W() as u32, H() as u32));
    let t = (end - 1.0 / 30.0).min((end * 0.25).max(1.0));
    render_frame(&mut c, &Scene { clips, tracks }, t, frames);
    // Project cards are landscape: other shapes are cropped to fill.
    Some(crate::media::import::cover_thumb(&c.into_pixmap(), w, (w as f32 * 9.0 / 16.0).round() as u32))
}
