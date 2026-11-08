// Video frames on iOS: the app decodes them with AVFoundation and hands them over through the
// media host (see the mobile crate's `media` module). Same API as the desktop's FFmpeg pool.

use super::compositor::Frames;
use crate::model::assets::asset;
use crate::model::types::Clip;
use std::sync::Arc;
use tiny_skia::Pixmap;

pub struct DecoderPool {
    max: (u32, u32),
}

impl DecoderPool {
    pub fn new(max_w: u32, max_h: u32) -> Self {
        DecoderPool { max: (max_w, max_h) }
    }
}

impl Frames for DecoderPool {
    fn video(&mut self, clip: &Clip, src_t: f64) -> Option<Arc<Pixmap>> {
        let path = asset(&clip.asset)?.media.as_ref()?.path.clone();
        crate::media::video_frame(&path, src_t.max(0.0), self.max.0, self.max.1).map(Arc::new)
    }
}
