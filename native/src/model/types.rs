// Project data. Field names serialise the same way as the browser version, so a clip copied from
// one pastes into the other and project files stay readable.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetType {
    Scene,
    Video,
    Image,
    Photo,
    Overlay,
    Text,
    Music,
    Audio,
}
impl AssetType {
    pub fn is_audio(self) -> bool {
        matches!(self, AssetType::Music | AssetType::Audio)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Text,
    Overlay,
    Video,
    Audio,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub name: String,
    pub kind: TrackKind,
    pub hidden: bool,
    pub muted: bool,
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAnim {
    None,
    Fade,
    Rise,
    Slide,
    Type,
}

/// How text blends into what's under it. Invert (difference) flips every channel to the opposite
/// of the footage underneath, so the text is complete contrast wherever it lands — always readable
/// and the colour follows the picture; Exclusion does the same but softer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TextBlend {
    #[default]
    Normal,
    Invert,
    Exclusion,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextProps {
    pub content: String,
    pub font: String,
    pub size: f64,
    pub weight: f64,
    pub color: String,
    #[serde(default)]
    pub blend: TextBlend,
    pub align: TextAlign,
    pub anim: TextAnim,
    #[serde(rename = "box")]
    pub boxed: bool,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Bool(bool),
    Num(f64),
    Str(String),
}
pub type Params = BTreeMap<String, ParamValue>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipEffect {
    pub uid: String,
    pub id: String,
    pub enabled: bool,
    pub amount: f64,
    pub params: Params,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransitionRef {
    pub id: String,
    pub dur: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FxScope {
    Clip,
    Frame,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: String,
    pub asset: String,
    #[serde(rename = "type")]
    pub kind: AssetType,
    pub name: String,
    pub track: String,
    pub start: f64,
    pub dur: f64,
    #[serde(rename = "in")]
    pub in_: f64,
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub rot: f64,
    pub opacity: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<TransitionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fx_scope: Option<FxScope>,
    #[serde(default)]
    pub effects: Vec<ClipEffect>,
    pub fade_in: f64,
    pub fade_out: f64,
    pub volume: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextProps>,
}

impl Clip {
    pub fn visual(&self) -> bool {
        !self.kind.is_audio()
    }
}

/// Fade multiplier at timeline time `t`.
pub fn fade_at(start: f64, dur: f64, fade_in: f64, fade_out: f64, t: f64) -> f64 {
    let lt = t - start;
    let mut f: f64 = 1.0;
    if fade_in > 0.0 {
        f = f.min(lt / fade_in);
    }
    if fade_out > 0.0 {
        f = f.min((dur - lt) / fade_out);
    }
    f.clamp(0.0, 1.0)
}
pub fn clip_fade(c: &Clip, t: f64) -> f64 {
    fade_at(c.start, c.dur, c.fade_in, c.fade_out, t)
}

/// Whether a clip's effects stay on its own shape rather than the whole frame.
pub fn effects_on_object(c: &Clip) -> bool {
    if let Some(s) = c.fx_scope {
        return s == FxScope::Clip;
    }
    c.kind == AssetType::Text || c.scale < 0.999 || c.x != 0.0 || c.y != 0.0 || c.rot != 0.0
}
