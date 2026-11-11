// Media files: probing, video frames, decoded audio and photos, all through FFmpeg (libav*).

pub mod audio;
pub mod import;
pub mod proxy;
pub mod video;

use std::sync::Once;

pub fn init() {
    static I: Once = Once::new();
    I.call_once(|| {
        let _ = ffmpeg_next::init();
        ffmpeg_next::log::set_level(ffmpeg_next::log::Level::Error);
    });
}
