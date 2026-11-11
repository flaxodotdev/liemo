// The timeline's sound: the built-in music (synthesised beat by beat, the same voices and
// envelopes as the browser's Web Audio graph) plus imported audio and the sound of imported
// videos. Rendered in blocks, so live playback and export produce identical samples.

use crate::media::audio::buffer;
use crate::model::assets::{asset, chord, Asset};
use crate::model::types::*;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::{Arc, OnceLock};

fn mtof(m: f64) -> f64 {
    440.0 * 2f64.powf((m - 69.0) / 12.0)
}

#[derive(Clone, Copy)]
enum Wave {
    Sine,
    Triangle,
    Square,
}
fn osc(w: Wave, phase: f64) -> f64 {
    let x = phase.rem_euclid(1.0);
    match w {
        Wave::Sine => (2.0 * PI * x).sin(),
        Wave::Square => if x < 0.5 { 1.0 } else { -1.0 },
        Wave::Triangle => {
            if x < 0.25 { 4.0 * x } else if x < 0.75 { 2.0 - 4.0 * x } else { 4.0 * x - 4.0 }
        }
    }
}

/// Add one enveloped tone into `out` (stereo interleaved), for the part inside the block.
/// Envelope: 0 → gain over min(20 ms, len/4), exponential down to 0.0001 at `len`, stop at len + 50 ms.
#[allow(clippy::too_many_arguments)]
fn tone(out: &mut [f32], t0: f64, sr: f64, when: f64, freq: f64, len: f64, w: Wave, gain: f64, vol: f64) {
    let n = out.len() / 2;
    let stop = when + len + 0.05;
    let t1 = t0 + n as f64 / sr;
    if stop <= t0 || when >= t1 {
        return;
    }
    let atk = (len / 4.0).min(0.02);
    let i0 = (((when - t0) * sr).ceil().max(0.0)) as usize;
    let i1 = (((stop - t0) * sr).ceil().max(0.0) as usize).min(n);
    let decay_ratio = (0.0001 / gain).ln();
    for i in i0..i1 {
        let tau = t0 + i as f64 / sr - when;
        let env = if tau < atk {
            gain * tau / atk
        } else if tau < len {
            gain * (decay_ratio * (tau - atk) / (len - atk)).exp()
        } else {
            0.0001
        };
        let v = (osc(w, freq * tau) * env * vol) as f32;
        out[i * 2] += v;
        out[i * 2 + 1] += v;
    }
}

fn kick(out: &mut [f32], t0: f64, sr: f64, when: f64, vol: f64) {
    let n = out.len() / 2;
    let (f0, f1, tf) = (140.0f64, 40.0f64, 0.12f64);
    let r = (f1 / f0).ln();
    let i0 = (((when - t0) * sr).ceil().max(0.0)) as usize;
    let i1 = (((when + 0.2 - t0) * sr).ceil().max(0.0) as usize).min(n);
    for i in i0..i1 {
        let tau = t0 + i as f64 / sr - when;
        // Phase is the integral of an exponential sweep, then steady at 40 Hz.
        let phase = if tau < tf { f0 * tf * ((r * tau / tf).exp() - 1.0) / r } else { f0 * tf * ((r).exp() - 1.0) / r + f1 * (tau - tf) };
        let env = if tau < 0.18 { 0.5 * ((0.001f64 / 0.5).ln() * tau / 0.18).exp() } else { 0.001 };
        let v = ((2.0 * PI * phase).sin() * env * vol) as f32;
        out[i * 2] += v;
        out[i * 2 + 1] += v;
    }
}

/// 60 ms of white noise through a 7 kHz high-pass, per sample rate.
fn hat_noise(sr: f64) -> Arc<Vec<f32>> {
    static C: OnceLock<Mutex<HashMap<u32, Arc<Vec<f32>>>>> = OnceLock::new();
    let c = C.get_or_init(|| Mutex::new(HashMap::new()));
    let key = sr as u32;
    if let Some(v) = c.lock().get(&key) {
        return v.clone();
    }
    let len = (sr * 0.06) as usize;
    let mut rng = crate::model::time::rng(1234);
    // RBJ biquad high-pass, f0 = 7 kHz, Q ≈ 1.12 (Web Audio's default 1 dB).
    let w0 = 2.0 * PI * 7000.0 / sr;
    let alpha = w0.sin() / (2.0 * 1.122);
    let cw = w0.cos();
    let (b0, b1, b2) = ((1.0 + cw) / 2.0, -(1.0 + cw), (1.0 + cw) / 2.0);
    let (a0, a1, a2) = (1.0 + alpha, -2.0 * cw, 1.0 - alpha);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    let v: Vec<f32> = (0..len)
        .map(|_| {
            let x = rng.next() as f64 * 2.0 - 1.0;
            let y = (b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2) / a0;
            x2 = x1;
            x1 = x;
            y2 = y1;
            y1 = y;
            y as f32
        })
        .collect();
    let v = Arc::new(v);
    c.lock().insert(key, v.clone());
    v
}
fn hat(out: &mut [f32], t0: f64, sr: f64, when: f64, vol: f64) {
    let noise = hat_noise(sr);
    let n = out.len() / 2;
    let i0 = (((when - t0) * sr).ceil().max(0.0)) as usize;
    let i1 = (((when + 0.06 - t0) * sr).ceil().max(0.0) as usize).min(n);
    for i in i0..i1 {
        let tau = t0 + i as f64 / sr - when;
        let k = (tau * sr) as usize;
        if k >= noise.len() {
            break;
        }
        let env = if tau < 0.05 { 0.06 * ((0.001f64 / 0.06).ln() * tau / 0.05).exp() } else { 0.001 };
        let v = (noise[k] as f64 * env * vol) as f32;
        out[i * 2] += v;
        out[i * 2 + 1] += v;
    }
}

/// One beat of a music asset: pad on the bar, bass, arp, kick and hats.
pub fn play_beat(out: &mut [f32], t0: f64, sr: f64, a: &Asset, b: i64, when: f64, vol: f64) {
    let beat = 60.0 / a.bpm;
    let root = a.roots[((b / 4) % 4) as usize] as f64;
    let ch = chord(a.quality);
    if b % 4 == 0 {
        for &n in ch {
            tone(out, t0, sr, when, mtof(root + 12.0 + n as f64), beat * 4.0, Wave::Triangle, 0.05, vol);
        }
    }
    tone(out, t0, sr, when, mtof(root - 12.0), beat * 0.9, Wave::Sine, 0.22, vol);
    tone(out, t0, sr, when + beat / 2.0, mtof(root + 24.0 + ch[((b * 2 + 1) as usize) % ch.len()] as f64), beat * 0.4, Wave::Square, 0.018, vol);
    if b % 2 == 0 {
        kick(out, t0, sr, when, vol);
    }
    for off in [0.0, 0.5] {
        hat(out, t0, sr, when + off * beat, vol);
    }
}

/// The longest any beat's sound lasts after it starts (the bar pad), in beats.
const TAIL_BEATS: f64 = 4.0;

fn music_clip(out: &mut [f32], t0: f64, sr: f64, c: &Clip, a: &Asset, from: f64, until: f64, duck: &dyn Fn(f64) -> f64) {
    let beat = 60.0 / a.bpm;
    let n = out.len() / 2;
    let t1 = t0 + n as f64 / sr;
    let origin = c.start - c.in_;
    let first = (((t0 - TAIL_BEATS * beat - 0.1 - origin) / beat).ceil() as i64).max(0);
    let mut b = first;
    loop {
        let tt = origin + b as f64 * beat;
        if tt >= c.start + c.dur || tt >= t1 || tt >= until {
            break;
        }
        if tt >= c.start && tt >= from {
            play_beat(out, t0, sr, a, b, tt, c.volume_at(tt - c.start) * clip_fade(c, tt) * duck(tt));
        }
        b += 1;
    }
}

/// Mix every audible clip into `out` (stereo interleaved, zeroed by the caller) for the block
/// starting at timeline time `t0`. Beats starting before `from` or at/after `until` are skipped,
/// which is how live playback avoids replaying beats that started before Play.
/// The reworking a clip's sound gets: an audio clip's sound effects, or for a sped-up or slowed
/// video clip, its new speed with the pitch kept (as phone editors do).
fn sound_fx(c: &Clip) -> Option<crate::model::types::AudioFx> {
    match c.kind {
        AssetType::Audio => super::fx::active(c.sound.as_ref()).cloned(),
        // Ramped clips play their sound like tape, following the picture.
        AssetType::Video if c.ramped() => None,
        AssetType::Video => match &c.motion {
            Some(m) if !m.reverse && (m.speed - 1.0).abs() > 1e-3 => {
                Some(crate::model::types::AudioFx { speed: m.speed.clamp(0.25, 4.0), keep_pitch: true, ..Default::default() })
            }
            _ => None,
        },
        _ => None,
    }
}

/// The sound to caption: everything but the music (built-in tracks and the Music track), so
/// speech recognition hears the voice.
pub fn voice_range(clips: &[Clip], tracks: &[Track], start: f64, stop: f64) -> Option<Vec<f32>> {
    let voice: Vec<Clip> = clips
        .iter()
        .filter(|c| c.kind != AssetType::Music && c.track != "A1")
        .cloned()
        .map(|mut c| {
            c.duck = 0.0;
            c
        })
        .collect();
    render_range(&voice, tracks, start, stop, &|| false)
}

// ---------- Loudness ----------

/// The volume that brings a clip's sound to about −16 dB RMS (speech and music loudness for
/// online video) without peaks going over −1 dB. None when its sound isn't loaded or is silent.
pub fn normalize_volume(c: &Clip) -> Option<f64> {
    let buf = buffer(&c.asset)?;
    let rate = crate::media::audio::RATE as f64;
    let span = if c.kind == AssetType::Audio { c.dur * c.speed() } else { c.src_span() };
    let (a, b) = ((c.in_ * rate) as usize * 2, (((c.in_ + span) * rate) as usize * 2).min(buf.samples.len()));
    if b <= a {
        return None;
    }
    let part = &buf.samples[a..b];
    let peak = part.iter().fold(0f32, |m, v| m.max(v.abs())) as f64;
    if peak < 1e-4 {
        return None;
    }
    let rms = (part.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / part.len() as f64).sqrt();
    let want = 10f64.powf(-16.0 / 20.0) / rms.max(1e-6);
    let ceiling = 10f64.powf(-1.0 / 20.0) / peak;
    Some(((want.min(ceiling).clamp(0.0, MAX_VOLUME)) * 100.0).round() / 100.0)
}

// ---------- Ducking ----------

/// Loudness (RMS) of an imported sound in 20 ms steps, made once per asset.
const LEVEL_STEP: f64 = 0.02;
fn levels(asset: &str) -> Option<Arc<Vec<f32>>> {
    static C: OnceLock<Mutex<HashMap<String, (usize, Arc<Vec<f32>>)>>> = OnceLock::new();
    let buf = buffer(asset)?;
    let c = C.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((len, v)) = c.lock().get(asset) {
        if *len == buf.samples.len() {
            return Some(v.clone());
        }
    }
    let per = (crate::media::audio::RATE as f64 * LEVEL_STEP) as usize * 2;
    let v: Vec<f32> = buf.samples.chunks(per).map(|ch| (ch.iter().map(|x| x * x).sum::<f32>() / ch.len().max(1) as f32).sqrt()).collect();
    let v = Arc::new(v);
    c.lock().insert(asset.to_string(), (buf.samples.len(), v.clone()));
    Some(v)
}

/// Clips whose sound counts as voice for ducking: imported audio and video sound on other tracks
/// that don't duck themselves.
struct Voice {
    start: f64,
    end: f64,
    clip: Clip,
    levels: Arc<Vec<f32>>,
}
fn voices(clips: &[Clip], muted: &dyn Fn(&str) -> bool, t0: f64, t1: f64) -> Vec<Voice> {
    clips
        .iter()
        .filter(|c| matches!(c.kind, AssetType::Audio | AssetType::Video) && c.duck <= 0.0 && !muted(&c.track) && !c.frozen() && c.volume > 0.0)
        .filter(|c| c.start < t1 + 0.5 && c.start + c.dur > t0 - 0.5)
        .filter_map(|c| levels(&c.asset).map(|l| Voice { start: c.start, end: c.start + c.dur, clip: c.clone(), levels: l }))
        .collect()
}
/// Whether someone is audibly talking at timeline time `t`.
fn voiced(vs: &[Voice], t: f64) -> bool {
    vs.iter().any(|v| {
        if t < v.start || t >= v.end {
            return false;
        }
        let lt = t - v.start;
        let src = if v.clip.kind == AssetType::Audio { v.clip.in_ + lt * v.clip.speed() } else { v.clip.in_ + v.clip.src_offset(lt) };
        let i = (src / LEVEL_STEP) as usize;
        v.levels.get(i).map(|l| *l * v.clip.volume_at(lt) as f32 > 0.02).unwrap_or(false)
    })
}
/// How far ducking is pulled down at `t`, 0–1: it starts dipping 0.12 s before the voice and
/// comes back over 0.35 s after it stops.
fn duck_env(vs: &[Voice], t: f64) -> f64 {
    const ATTACK: f64 = 0.12;
    const RELEASE: f64 = 0.35;
    let mut best: f64 = 0.0;
    let mut tau = t - RELEASE;
    while tau <= t + ATTACK {
        if voiced(vs, tau) {
            let w = if tau >= t { 1.0 - (tau - t) / ATTACK } else { 1.0 - (t - tau) / RELEASE };
            best = best.max(w);
            if best >= 1.0 {
                break;
            }
        }
        tau += LEVEL_STEP;
    }
    best.clamp(0.0, 1.0)
}
/// The gain from ducking along a block, evaluated every 10 ms.
fn duck_curve(duck: f64, vs: &[Voice], t0: f64, t1: f64) -> Vec<(f64, f64)> {
    let mut v = vec![];
    let mut t = t0;
    loop {
        v.push((t, 1.0 - duck.clamp(0.0, 1.0) * 0.85 * duck_env(vs, t)));
        if t >= t1 {
            break;
        }
        t = (t + 0.01).min(t1);
    }
    v
}
fn curve_at(c: &[(f64, f64)], t: f64) -> f64 {
    if c.is_empty() {
        return 1.0;
    }
    let i = c.partition_point(|p| p.0 <= t);
    if i == 0 {
        return c[0].1;
    }
    if i >= c.len() {
        return c[c.len() - 1].1;
    }
    let (a, b) = (c[i - 1], c[i]);
    a.1 + (b.1 - a.1) * ((t - a.0) / (b.0 - a.0).max(1e-9))
}
/// Ducking gain for a clip at time `t` (used for beats of built-in music).
fn duck_at(c: &Clip, vs: &[Voice], t: f64) -> f64 {
    if c.duck <= 0.0 { 1.0 } else { 1.0 - c.duck.clamp(0.0, 1.0) * 0.85 * duck_env(vs, t) }
}

pub fn mix(out: &mut [f32], clips: &[Clip], tracks: &[Track], t0: f64, sr: f64, from: f64, until: f64, master: f32) {
    let n = out.len() / 2;
    let t1 = t0 + n as f64 / sr;
    let muted = |id: &str| tracks.iter().find(|t| t.id == id).map(|t| t.muted).unwrap_or(false);
    let vs = if clips.iter().any(|c| c.duck > 0.0) { voices(clips, &muted, t0, t1) } else { vec![] };
    for c in clips {
        let keyed_volume = c.keys.get("volume").filter(|k| !k.is_empty());
        if !matches!(c.kind, AssetType::Music | AssetType::Audio | AssetType::Video) || muted(&c.track) || (c.volume <= 0.0 && keyed_volume.is_none()) || c.frozen() {
            continue;
        }
        if c.kind == AssetType::Music {
            if let Some(a) = asset(&c.asset) {
                if a.bpm > 0.0 && c.start < t1 + 0.1 && c.start + c.dur + TAIL_BEATS * 60.0 / a.bpm > t0 {
                    music_clip(out, t0, sr, c, &a, from, until, &|t| duck_at(c, &vs, t));
                }
            }
            continue;
        }
        if c.start >= t1 || c.start + c.dur <= t0 {
            continue;
        }
        // Clips with sound effects play their reworked render, whose time runs at the clip's speed.
        let fx = sound_fx(c);
        let (buf, offset) = match fx.as_ref() {
            Some(f) => match super::fx::processed(&c.asset, f) {
                Some(p) => {
                    let o = c.in_ / p.speed - c.start;
                    (Src::Fx(p), o)
                }
                None => continue,
            },
            None => match buffer(&c.asset) {
                Some(b) => (Src::Raw(b), c.in_ - c.start),
                None => continue,
            },
        };
        let i0 = (((c.start - t0) * sr).ceil().max(0.0)) as usize;
        let i1 = (((c.start + c.dur - t0) * sr).ceil().max(0.0) as usize).min(n);
        let ducking = if c.duck > 0.0 { duck_curve(c.duck, &vs, t0, t1) } else { vec![] };
        for i in i0..i1 {
            let t = t0 + i as f64 / sr;
            if t >= until || t < from {
                continue;
            }
            let vol = match keyed_volume {
                Some(k) => crate::model::keys::value_at(k, t - c.start).unwrap_or(c.volume).clamp(0.0, MAX_VOLUME),
                None => c.volume,
            };
            let mut g = (vol * clip_fade(c, t)).clamp(0.0, MAX_VOLUME) as f32;
            if !ducking.is_empty() {
                g *= curve_at(&ducking, t) as f32;
            }
            // Reversed picture clips play their sound backwards (resampled).
            let pos = if c.kind == AssetType::Video && fx.is_none() { c.in_ + c.src_offset(t - c.start) } else { t + offset };
            let (l, r) = buf.get().at(pos);
            out[i * 2] += l * g;
            out[i * 2 + 1] += r * g;
        }
    }
    if master != 1.0 {
        for v in out.iter_mut() {
            *v *= master;
        }
    }
}

enum Src {
    Raw(Arc<crate::media::audio::AudioBuf>),
    Fx(Arc<super::fx::Processed>),
}
impl Src {
    fn get(&self) -> &crate::media::audio::AudioBuf {
        match self {
            Src::Raw(b) => b,
            Src::Fx(p) => &p.buf,
        }
    }
}

/// Library preview: 16 beats of a track from `t0` seconds into the preview.
pub fn preview(out: &mut [f32], a: &Asset, t0: f64, sr: f64) {
    let beat = 60.0 / a.bpm;
    let n = out.len() / 2;
    let t1 = t0 + n as f64 / sr;
    for b in 0..16i64 {
        let when = b as f64 * beat + 0.05;
        if when >= t1 {
            break;
        }
        if when + TAIL_BEATS * beat + 0.1 < t0 {
            continue;
        }
        play_beat(out, t0, sr, a, b, when, 0.8);
    }
    for v in out.iter_mut() {
        *v *= 0.8;
    }
}

/// Offline mix of [start, stop) at 48 kHz, for export. None when nothing is audible.
pub fn render_range(clips: &[Clip], tracks: &[Track], start: f64, stop: f64, cancel: &dyn Fn() -> bool) -> Option<Vec<f32>> {
    let muted = |id: &str| tracks.iter().find(|t| t.id == id).map(|t| t.muted).unwrap_or(false);
    let audible = clips.iter().any(|c| {
        matches!(c.kind, AssetType::Music | AssetType::Audio | AssetType::Video)
            && !muted(&c.track)
            && (c.volume > 0.0 || c.keys.contains_key("volume"))
            && !c.frozen()
            && c.start < stop
            && c.start + c.dur > start
            && (c.kind == AssetType::Music || buffer(&c.asset).is_some())
    });
    if !audible {
        return None;
    }
    // Render every clip's sound effects exactly before mixing (playback may use a stand-in).
    for c in clips.iter().filter(|c| c.start < stop && c.start + c.dur > start) {
        if let Some(f) = sound_fx(c) {
            super::fx::processed_now(&c.asset, &f);
        }
    }
    let sr = crate::media::audio::RATE as f64;
    let frames = ((stop - start) * sr).ceil().max(1.0) as usize;
    let mut out = vec![0f32; frames * 2];
    let block = 4800;
    let mut i = 0;
    while i < frames {
        if cancel() {
            return None;
        }
        let n = block.min(frames - i);
        let t0 = start + i as f64 / sr;
        mix(&mut out[i * 2..(i + n) * 2], clips, tracks, t0, sr, start, stop, 0.8);
        i += n;
    }
    Some(out)
}
