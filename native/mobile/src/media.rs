// Media on iOS. The shared engine calls the same functions as on the desktop (`media::import`,
// `media::audio`); here they're answered by the app's media host, which uses AVFoundation and
// Image I/O, instead of FFmpeg.

use crate::api::{MediaFrame, MediaHost};
use parking_lot::RwLock;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tiny_skia::Pixmap;

fn host_slot() -> &'static RwLock<Option<Arc<dyn MediaHost>>> {
    static H: OnceLock<RwLock<Option<Arc<dyn MediaHost>>>> = OnceLock::new();
    H.get_or_init(|| RwLock::new(None))
}
pub fn set_host(h: Arc<dyn MediaHost>) {
    *host_slot().write() = Some(h);
}
fn host() -> Option<Arc<dyn MediaHost>> {
    host_slot().read().clone()
}

/// A straight-alpha RGBA picture from the host, as a premultiplied pixmap.
fn to_pixmap(f: MediaFrame) -> Option<Pixmap> {
    if f.width == 0 || f.height == 0 || f.rgba.len() < (f.width * f.height * 4) as usize {
        return None;
    }
    Some(crate::render::canvas::from_straight(&f.rgba, f.width, f.height))
}

/// The video picture at `t` seconds, at most `max_w`×`max_h`.
pub fn video_frame(path: &Path, t: f64, max_w: u32, max_h: u32) -> Option<Pixmap> {
    host()?.video_frame(path.display().to_string(), t, max_w, max_h).and_then(to_pixmap)
}

pub mod import {
    use super::{host, to_pixmap};
    use crate::model::assets::{register, Asset, MediaInfo};
    use crate::model::types::AssetType;
    use crate::render::canvas::{from_straight, pixmap, resample_area, Canvas};
    use crate::render::compositor::draw_cover;
    use crate::storage::MediaRecord;
    use anyhow::{anyhow, Result};
    use std::path::Path;
    use std::sync::Arc;
    use tiny_skia::Pixmap;

    const VIDEO: &[&str] = &["mp4", "mov", "m4v", "3gp", "hevc"];
    const AUDIO: &[&str] = &["mp3", "wav", "m4a", "aac", "flac", "aif", "aiff", "caf"];
    const IMAGE: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "heic", "heif"];

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
    pub fn new_media_id() -> String {
        let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        format!("u{}{}", radix36(ms as u64), radix36(rand_u32() as u64 % 1679616))
    }

    /// Decode a picture: the image crate, or the host (HEIC and anything else Image I/O reads).
    pub fn decode_photo(path: &Path, max: u32) -> Result<Pixmap> {
        let pm = match image::open(path) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                from_straight(rgba.as_raw(), w, h)
            }
            Err(_) => host()
                .and_then(|h| h.decode_image(path.display().to_string(), max))
                .and_then(to_pixmap)
                .ok_or_else(|| anyhow!("{} couldn't be read as an image", file_name(path)))?,
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

    pub fn file_name(p: &Path) -> String {
        p.file_name().and_then(|s| s.to_str()).unwrap_or("That file").to_string()
    }

    /// Measure a file and make its thumbnail.
    pub fn read_file(path: &Path, id: String) -> Result<Imported> {
        let kind = kind_of(path).ok_or_else(|| anyhow!("{} isn't a video, audio or image file", file_name(path)))?;
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string();
        let sub = format!("{:.1} MB · imported", size as f64 / 1048576.0);
        let mut record = MediaRecord { id, kind, name, sub, dur: 5.0, path: path.to_path_buf(), width: 0, height: 0, has_audio: false };
        match kind {
            AssetType::Photo => {
                let bmp = decode_photo(path, 3840)?;
                record.width = bmp.width();
                record.height = bmp.height();
                let thumb = cover_thumb(&bmp, 320, 180);
                Ok(Imported { record, thumb: Some(thumb), bitmap: Some(bmp) })
            }
            _ => {
                let h = host().ok_or_else(|| anyhow!("No media host"))?;
                let p = h.probe(path.display().to_string()).ok_or_else(|| anyhow!("{} couldn't be read", file_name(path)))?;
                record.dur = if p.duration.is_finite() && p.duration > 0.0 { p.duration } else { 5.0 };
                record.width = p.width;
                record.height = p.height;
                record.has_audio = p.has_audio;
                let mut thumb = None;
                if kind == AssetType::Video {
                    if !p.has_video {
                        return Err(anyhow!("{} has no picture", file_name(path)));
                    }
                    if let Some(f) = super::video_frame(path, (record.dur / 3.0).min(1.0), 640, 360) {
                        thumb = Some(cover_thumb(&f, 320, 180));
                    }
                } else if !p.has_audio {
                    return Err(anyhow!("{} has no sound", file_name(path)));
                }
                Ok(Imported { record, thumb, bitmap: None })
            }
        }
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
            AssetType::Video | AssetType::Audio if rec.has_audio || rec.kind == AssetType::Audio => match super::audio::decode(&rec.path) {
                Some(b) => {
                    super::audio::set_buffer(&rec.id, b);
                    (None, true)
                }
                None => (None, false),
            },
            _ => (None, false),
        }
    }
}

pub mod audio {
    use super::host;
    use parking_lot::RwLock;
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::{Arc, OnceLock};

    pub const RATE: u32 = 48000;

    /// Interleaved stereo samples at 48 kHz.
    pub struct AudioBuf {
        pub samples: Vec<f32>,
    }
    impl AudioBuf {
        pub fn frames(&self) -> usize {
            self.samples.len() / 2
        }
        /// Linear-interpolated stereo sample at `pos` seconds.
        #[inline]
        pub fn at(&self, pos: f64) -> (f32, f32) {
            let x = pos * RATE as f64;
            if x < 0.0 {
                return (0.0, 0.0);
            }
            let i = x as usize;
            let n = self.frames();
            if i + 1 >= n {
                return if i < n { (self.samples[i * 2], self.samples[i * 2 + 1]) } else { (0.0, 0.0) };
            }
            let f = (x - i as f64) as f32;
            let s = &self.samples;
            (s[i * 2] + (s[i * 2 + 2] - s[i * 2]) * f, s[i * 2 + 1] + (s[i * 2 + 3] - s[i * 2 + 1]) * f)
        }
    }

    /// The file's sound as 48 kHz interleaved stereo, decoded by the host.
    pub fn decode(path: &Path) -> Option<AudioBuf> {
        let samples = host()?.decode_audio(path.display().to_string())?;
        (!samples.is_empty()).then_some(AudioBuf { samples })
    }

    fn registry() -> &'static RwLock<HashMap<String, Arc<AudioBuf>>> {
        static R: OnceLock<RwLock<HashMap<String, Arc<AudioBuf>>>> = OnceLock::new();
        R.get_or_init(|| RwLock::new(HashMap::new()))
    }
    pub fn buffer(asset_id: &str) -> Option<Arc<AudioBuf>> {
        registry().read().get(asset_id).cloned()
    }
    pub fn set_buffer(asset_id: &str, b: AudioBuf) {
        registry().write().insert(asset_id.to_string(), Arc::new(b));
    }
}
