// Video pictures for the compositor: one decoder per clip, so two clips of the same file can sit
// at different source times (and transition into each other).

use super::compositor::Frames;
use crate::media::video::VideoDecoder;
use crate::model::assets::asset;
use crate::model::types::Clip;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tiny_skia::Pixmap;

pub struct DecoderPool {
    decs: HashMap<String, (String, Option<VideoDecoder>)>,
    max: (u32, u32),
    /// Decode editing proxies where they exist (the preview; never export).
    proxies: bool,
}

impl DecoderPool {
    pub fn new(max_w: u32, max_h: u32) -> Self {
        DecoderPool { decs: HashMap::new(), max: (max_w, max_h), proxies: false }
    }
    /// For the preview: plays proxies instead of big originals.
    pub fn preview(max_w: u32, max_h: u32) -> Self {
        DecoderPool { proxies: true, ..Self::new(max_w, max_h) }
    }
    /// Drop decoders for clips that no longer exist.
    pub fn retain(&mut self, clips: &[Clip]) {
        let live: HashSet<&str> = clips.iter().map(|c| c.id.as_str()).collect();
        self.decs.retain(|k, _| live.contains(k.as_str()));
    }
    pub fn clear(&mut self) {
        self.decs.clear();
    }
}

impl Frames for DecoderPool {
    fn video(&mut self, clip: &Clip, src_t: f64) -> Option<Arc<Pixmap>> {
        let entry = self.decs.entry(clip.id.clone()).or_insert_with(|| (String::new(), None));
        if entry.0 != clip.asset {
            let mut path = asset(&clip.asset)?.media.as_ref()?.path.clone();
            if self.proxies {
                path = crate::media::proxy::for_preview(&clip.asset, &path);
            }
            *entry = (clip.asset.clone(), VideoDecoder::open(&path).ok());
        }
        let dec = entry.1.as_mut()?;
        dec.set_max(self.max.0, self.max.1);
        dec.frame_at(src_t.max(0.0))
    }
}
