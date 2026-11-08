// Liemo's engine for iOS. The engine's sources are the desktop app's own (../src), compiled with
// `cfg(liemo_mobile)`; `media` is this crate's AVFoundation-backed stand-in for the desktop's
// FFmpeg layer, and `api` is what Swift calls.

uniffi::setup_scaffolding!();

pub mod api;
pub mod media;

#[path = "../../src/model/mod.rs"]
pub mod model;
#[path = "../../src/render/mod.rs"]
pub mod render;
#[path = "../../src/fx/mod.rs"]
pub mod fx;
#[path = "../../src/storage.rs"]
pub mod storage;

pub mod audio;
pub mod mcp;
