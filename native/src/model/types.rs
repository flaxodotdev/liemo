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
    /// Springs in from small with an overshoot: punchy word-by-word captions.
    Pop,
    /// Drops in from above and bounces to rest.
    Bounce,
    /// Shrinks in from twice its size.
    Zoom,
    /// Comes into focus from a blur.
    Blur,
    /// Letters rise in one after another.
    Wave,
    /// Jitters in with red and cyan ghosts.
    Glitch,
    /// Flickers on like a failing sign.
    Flicker,
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
    /// Outline colour (#rrggbb) and width (canvas units, around 2–12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<String>,
    #[serde(default = "outline_width", skip_serializing_if = "is_outline_width", rename = "outlineWidth")]
    pub outline_width: f64,
    /// A soft drop shadow in this colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<String>,
    /// A neon glow in this colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glow: Option<String>,
    /// A rounded highlight behind each line, in this colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<String>,
    /// Turned in 3D, in degrees: tipped back or forward (around the horizontal axis) and swung
    /// left or right (around the vertical one), seen in perspective.
    #[serde(default, skip_serializing_if = "is_zero", rename = "tiltX")]
    pub tilt_x: f64,
    #[serde(default, skip_serializing_if = "is_zero", rename = "tiltY")]
    pub tilt_y: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

fn outline_width() -> f64 {
    6.0
}
fn is_outline_width(v: &f64) -> bool {
    *v == 6.0
}

/// How a visual clip runs through its source and enters and leaves the frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Motion {
    /// Source seconds per timeline second, 0.25–4.
    pub speed: f64,
    pub reverse: bool,
    pub flip_h: bool,
    pub flip_v: bool,
    /// Entrance and exit: "" (none), fade, zoom-in, zoom-out, slide-left/right/up/down, spin, pop.
    pub anim_in: String,
    pub anim_in_dur: f64,
    pub anim_out: String,
    pub anim_out_dur: f64,
    /// Runs for the whole clip: "" (none), breathe, float, wiggle, pulse, spin, shake, sway.
    pub anim_loop: String,
    pub anim_loop_speed: f64,
}
impl Default for Motion {
    fn default() -> Self {
        Motion {
            speed: 1.0,
            reverse: false,
            flip_h: false,
            flip_v: false,
            anim_in: String::new(),
            anim_in_dur: 0.5,
            anim_out: String::new(),
            anim_out_dur: 0.5,
            anim_loop: String::new(),
            anim_loop_speed: 1.0,
        }
    }
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

/// How an audio clip's sound is reworked: speed (tape-style, or keeping the pitch), reverb, bass,
/// a low-pass "muffle" and an 8D pan that circles the head. Defaults leave the sound untouched.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioFx {
    /// Playback rate: 0.8 is "slowed", 1.25 "sped up".
    pub speed: f64,
    /// Change speed without changing pitch (otherwise pitch follows speed, like a record).
    pub keep_pitch: bool,
    /// Reverb wet level, 0–1.
    pub reverb: f64,
    /// Reverb room size, 0–1 (small room to cathedral).
    pub room: f64,
    /// Low-end boost, 0–1 (up to about +12 dB at 80 Hz).
    pub bass: f64,
    /// Low-pass amount, 0–1 ("from the next room", "underwater").
    pub muffle: f64,
    /// 8D audio: how far the sound circles left and right, 0–1.
    pub pan8d: f64,
}
impl Default for AudioFx {
    fn default() -> Self {
        AudioFx { speed: 1.0, keep_pitch: false, reverb: 0.0, room: 0.6, bass: 0.0, muffle: 0.0, pan8d: 0.0 }
    }
}
impl AudioFx {
    /// Whether this leaves the sound exactly as it was.
    pub fn is_identity(&self) -> bool {
        (self.speed - 1.0).abs() < 1e-6 && self.reverb <= 0.0 && self.bass <= 0.0 && self.muffle <= 0.0 && self.pan8d <= 0.0
    }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<AudioFx>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
}

impl Clip {
    pub fn visual(&self) -> bool {
        !self.kind.is_audio()
    }
    /// Source seconds per timeline second: audio clips by their sound settings, picture clips
    /// by their motion.
    pub fn speed(&self) -> f64 {
        if self.kind == AssetType::Audio {
            return self.sound.as_ref().map(|s| s.speed.clamp(0.25, 4.0)).unwrap_or(1.0);
        }
        self.motion.as_ref().map(|m| m.speed.clamp(0.25, 4.0)).unwrap_or(1.0)
    }
    /// Seconds into the source (past `in`) at `lt` seconds into the clip: faster, slower or
    /// backwards with the clip's motion.
    pub fn src_offset(&self, lt: f64) -> f64 {
        match &self.motion {
            Some(m) if self.kind != AssetType::Audio => {
                let s = m.speed.clamp(0.25, 4.0);
                if m.reverse { (self.dur - lt).max(0.0) * s } else { lt * s }
            }
            _ => lt,
        }
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
