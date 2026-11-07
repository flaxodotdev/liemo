// Embedded icons (Lucide, ISC licence), served to GPUI as rayna/icons/<name>.svg. Anything else
// falls through to gpui-component's own icon set.

use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

const ICONS: &[(&str, &[u8])] = &[
    ("alert-triangle", include_bytes!("../../assets/icons/alert-triangle.svg")),
    ("audio-lines", include_bytes!("../../assets/icons/audio-lines.svg")),
    ("blend", include_bytes!("../../assets/icons/blend.svg")),
    ("check", include_bytes!("../../assets/icons/check.svg")),
    ("chevron-down", include_bytes!("../../assets/icons/chevron-down.svg")),
    ("chevron-left", include_bytes!("../../assets/icons/chevron-left.svg")),
    ("chevron-right", include_bytes!("../../assets/icons/chevron-right.svg")),
    ("chevron-up", include_bytes!("../../assets/icons/chevron-up.svg")),
    ("clapperboard", include_bytes!("../../assets/icons/clapperboard.svg")),
    ("cloud-alert", include_bytes!("../../assets/icons/cloud-alert.svg")),
    ("copy", include_bytes!("../../assets/icons/copy.svg")),
    ("download", include_bytes!("../../assets/icons/download.svg")),
    ("eye-off", include_bytes!("../../assets/icons/eye-off.svg")),
    ("eye", include_bytes!("../../assets/icons/eye.svg")),
    ("film", include_bytes!("../../assets/icons/film.svg")),
    ("image", include_bytes!("../../assets/icons/image.svg")),
    ("layers", include_bytes!("../../assets/icons/layers.svg")),
    ("loader-2", include_bytes!("../../assets/icons/loader-2.svg")),
    ("lock-open", include_bytes!("../../assets/icons/lock-open.svg")),
    ("lock", include_bytes!("../../assets/icons/lock.svg")),
    ("magnet", include_bytes!("../../assets/icons/magnet.svg")),
    ("more-horizontal", include_bytes!("../../assets/icons/more-horizontal.svg")),
    ("music-2", include_bytes!("../../assets/icons/music-2.svg")),
    ("pause-fill", include_bytes!("../../assets/icons/pause-fill.svg")),
    ("pause", include_bytes!("../../assets/icons/pause.svg")),
    ("pencil", include_bytes!("../../assets/icons/pencil.svg")),
    ("play-fill", include_bytes!("../../assets/icons/play-fill.svg")),
    ("play", include_bytes!("../../assets/icons/play.svg")),
    ("plus", include_bytes!("../../assets/icons/plus.svg")),
    ("redo-2", include_bytes!("../../assets/icons/redo-2.svg")),
    ("repeat", include_bytes!("../../assets/icons/repeat.svg")),
    ("scan", include_bytes!("../../assets/icons/scan.svg")),
    ("scissors", include_bytes!("../../assets/icons/scissors.svg")),
    ("search", include_bytes!("../../assets/icons/search.svg")),
    ("skip-back", include_bytes!("../../assets/icons/skip-back.svg")),
    ("skip-forward", include_bytes!("../../assets/icons/skip-forward.svg")),
    ("sparkles", include_bytes!("../../assets/icons/sparkles.svg")),
    ("trash-2", include_bytes!("../../assets/icons/trash-2.svg")),
    ("type", include_bytes!("../../assets/icons/type.svg")),
    ("undo-2", include_bytes!("../../assets/icons/undo-2.svg")),
    ("upload", include_bytes!("../../assets/icons/upload.svg")),
    ("volume-2", include_bytes!("../../assets/icons/volume-2.svg")),
    ("volume-x", include_bytes!("../../assets/icons/volume-x.svg")),
    ("x", include_bytes!("../../assets/icons/x.svg")),
    ("zoom-in", include_bytes!("../../assets/icons/zoom-in.svg")),
    ("zoom-out", include_bytes!("../../assets/icons/zoom-out.svg")),
    ("panel-left", include_bytes!("../../assets/icons/panel-left.svg")),
    ("panel-right", include_bytes!("../../assets/icons/panel-right.svg")),
    ("panel-bottom", include_bytes!("../../assets/icons/panel-bottom.svg")),
    ("arrow-up", include_bytes!("../../assets/icons/arrow-up.svg")),
    ("message-circle", include_bytes!("../../assets/icons/message-circle.svg")),
    ("liemo-mark", include_bytes!("../../assets/icons/liemo-mark.svg")),
    // Agent marks for the Connect AI button (Simple Icons, CC0; trademarks of their owners).
    ("brand-claude", include_bytes!("../../assets/icons/brand-claude.svg")),
    ("brand-openai", include_bytes!("../../assets/icons/brand-openai.svg")),
    ("brand-cursor", include_bytes!("../../assets/icons/brand-cursor.svg")),
    ("brand-githubcopilot", include_bytes!("../../assets/icons/brand-githubcopilot.svg")),
    ("brand-qwen", include_bytes!("../../assets/icons/brand-qwen.svg")),
    ("brand-opencode", include_bytes!("../../assets/icons/brand-opencode.svg")),
    ("brand-googlegemini", include_bytes!("../../assets/icons/brand-googlegemini.svg")),
];

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(name) = path.strip_prefix("rayna/icons/").and_then(|p| p.strip_suffix(".svg")) {
            return Ok(ICONS.iter().find(|(n, _)| *n == name).map(|(_, b)| Cow::Borrowed(*b)));
        }
        gpui_kit_assets::Assets.load(path)
    }
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit_assets::Assets.list(path)
    }
}
