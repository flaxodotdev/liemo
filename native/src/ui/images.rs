// Pixmaps → GPUI images, plus caches of thumbnails made off the UI thread.

use crate::render::canvas::to_bgra;
use gpui::RenderImage;
use parking_lot::Mutex;
use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tiny_skia::Pixmap;

pub fn to_image(pm: &Pixmap) -> Arc<RenderImage> {
    let buf = image::RgbaImage::from_raw(pm.width(), pm.height(), to_bgra(pm)).expect("image size");
    let frames: SmallVec<[image::Frame; 1]> = SmallVec::from_elem(image::Frame::new(buf), 1);
    Arc::new(RenderImage::new(frames))
}

type Cache = Mutex<HashMap<String, Arc<RenderImage>>>;
fn cache() -> &'static Cache {
    static C: OnceLock<Cache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}
pub fn cached(key: &str) -> Option<Arc<RenderImage>> {
    cache().lock().get(key).cloned()
}
pub fn put(key: &str, img: Arc<RenderImage>) {
    cache().lock().insert(key.to_string(), img);
}
pub fn forget(key: &str) {
    cache().lock().remove(key);
}

