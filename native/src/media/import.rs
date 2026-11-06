// Bringing files into the media pool: decide what a file is, measure it, make its thumbnail and
// register it as a library asset. Files stay where they are; the project refers to them by path.

use super::video::{probe, VideoDecoder};
use crate::model::assets::{register, Asset, MediaInfo};
use crate::model::types::AssetType;
use crate::render::canvas::{from_straight, pixmap, resample_area, Canvas};
use crate::render::compositor::draw_cover;
use crate::storage::MediaRecord;
use anyhow::{anyhow, Result};
use std::path::Path;
use std::sync::Arc;
use tiny_skia::Pixmap;

const VIDEO: &[&str] = &["mp4", "mov", "m4v", "webm", "mkv", "avi", "mpg", "mpeg", "3gp", "mts", "m2ts", "ogv", "flv", "wmv"];
const AUDIO: &[&str] = &["mp3", "wav", "m4a", "aac", "ogg", "oga", "flac", "opus", "aif", "aiff", "caf", "wma"];
const IMAGE: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "heic", "heif", "avif"];

pub fn kind_of(path: &Path) -> Option<AssetType> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    if VIDEO.contains(&ext.as_str()) {
        Some(AssetType::Video)
    } else if AUDIO.contains(&ext.as_str()) {
        Some(AssetType::Audio)
    } else if IMAGE.contains(&ext.as_str()) {
        Some(AssetType::Photo)
    } else {
        None
    }
}

pub fn new_media_id() -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("u{}{}", radix36(ms as u64), radix36(rand_u32() as u64 % 1679616))
}
pub fn radix36(mut n: u64) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut s = vec![];
    while n > 0 {
        s.push(D[(n % 36) as usize]);
        n /= 36;
    }
    s.reverse();
    String::from_utf8(s).unwrap()
}
pub fn rand_u32() -> u32 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    h.finish() as u32
}

/// Decode a picture file (falls back to FFmpeg for formats the image crate lacks).
pub fn decode_photo(path: &Path, max: u32) -> Result<Pixmap> {
    let pm = match image::open(path) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            from_straight(rgba.as_raw(), w, h)
        }
        Err(_) => {
            let mut d = VideoDecoder::open(path)?;
            d.set_max(8192, 8192);
            (*d.frame_at(0.0).ok_or_else(|| anyhow!("no picture"))?).clone()
        }
    };
    let (w, h) = (pm.width(), pm.height());
    if w.max(h) > max {
        let k = max as f32 / w.max(h) as f32;
        return Ok(resample_area(&pm, ((w as f32 * k).round() as u32).max(1), ((h as f32 * k).round() as u32).max(1)));
    }
    Ok(pm)
}

pub fn cover_thumb(src: &Pixmap, w: u32, h: u32) -> Pixmap {
    let mut c = Canvas::from_pixmap(pixmap(w, h));
    draw_cover(&mut c, src, w as f32, h as f32);
    c.into_pixmap()
}

pub struct Imported {
    pub record: MediaRecord,
    pub thumb: Option<Pixmap>,
    pub bitmap: Option<Pixmap>,
}

/// Measure a file and make its thumbnail. Runs off the UI thread.
pub fn read_file(path: &Path, id: String) -> Result<Imported> {
    let kind = kind_of(path).ok_or_else(|| anyhow!("{} isn't a video, audio or image file", file_name(path)))?;
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string();
    let sub = format!("{:.1} MB · imported", size as f64 / 1048576.0);
    let mut record = MediaRecord { id, kind, name, sub, dur: 5.0, path: path.to_path_buf(), width: 0, height: 0, has_audio: false };
    match kind {
        AssetType::Photo => {
            let bmp = decode_photo(path, 3840).map_err(|_| anyhow!("{} couldn't be read as an image", file_name(path)))?;
            record.width = bmp.width();
            record.height = bmp.height();
            let thumb = cover_thumb(&bmp, 320, 180);
            Ok(Imported { record, thumb: Some(thumb), bitmap: Some(bmp) })
        }
        _ => {
            let p = probe(path).map_err(|_| anyhow!("{} couldn't be read. Try an MP4, MOV, WebM, MP3 or WAV file.", file_name(path)))?;
            record.dur = if p.duration.is_finite() && p.duration > 0.0 { p.duration } else { 5.0 };
            record.width = p.width;
            record.height = p.height;
            record.has_audio = p.has_audio;
            let mut thumb = None;
            if kind == AssetType::Video {
                if !p.has_video {
                    return Err(anyhow!("{} has no picture", file_name(path)));
                }
                if let Ok(mut d) = VideoDecoder::open(path) {
                    d.set_max(640, 360);
                    if let Some(f) = d.frame_at((record.dur / 3.0).min(1.0)) {
                        thumb = Some(cover_thumb(&f, 320, 180));
                    }
                }
            } else if !p.has_audio {
                return Err(anyhow!("{} has no sound", file_name(path)));
            }
            Ok(Imported { record, thumb, bitmap: None })
        }
    }
}

pub fn file_name(p: &Path) -> String {
    p.file_name().and_then(|s| s.to_str()).unwrap_or("That file").to_string()
}

/// Put a media record into the asset library.
pub fn register_media(rec: &MediaRecord, thumb: Option<Pixmap>, bitmap: Option<Pixmap>) {
    register(Asset {
        id: rec.id.clone(),
        kind: rec.kind,
        name: rec.name.clone(),
        sub: rec.sub.clone(),
        dur: rec.dur,
        draw: None,
        blend: None,
        text: None,
        bpm: 0.0,
        roots: [0; 4],
        quality: "maj",
        hue: "#ffffff",
        media: Some(MediaInfo { path: rec.path.clone(), width: rec.width, height: rec.height, has_video: rec.kind == AssetType::Video, has_audio: rec.has_audio }),
        thumb: thumb.map(Arc::new),
        bitmap: bitmap.map(Arc::new),
        shader: None,
    });
}

/// Load what an asset needs to play: the photo's pixels, or the decoded sound.
pub fn load_payload(rec: &MediaRecord) -> (Option<Pixmap>, bool) {
    match rec.kind {
        AssetType::Photo => (decode_photo(&rec.path, 3840).ok(), false),
        AssetType::Video | AssetType::Audio if rec.has_audio || rec.kind == AssetType::Audio => {
            match super::audio::decode(&rec.path) {
                Ok(b) => {
                    super::audio::set_buffer(&rec.id, b);
                    (None, true)
                }
                Err(_) => (None, false),
            }
        }
        _ => (None, false),
    }
}

