// Decoded sound for imported audio and video files: the whole track at 48 kHz stereo f32,
// decoded once per asset and shared by playback and export.

use anyhow::{anyhow, Result};
use ffmpeg_next::{codec, format, frame, media, software::resampling, util::channel_layout::ChannelLayout, util::format::sample::Sample, util::format::sample::Type as SampleType};
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

pub fn decode(path: &Path) -> Result<AudioBuf> {
    super::init();
    let mut ictx = format::input(path)?;
    let st = ictx.streams().best(media::Type::Audio).ok_or_else(|| anyhow!("no audio"))?;
    let idx = st.index();
    let mut dec = codec::context::Context::from_parameters(st.parameters())?.decoder().audio()?;
    let mut resampler: Option<(resampling::Context, Sample, u64, u32)> = None;
    let mut samples: Vec<f32> = vec![];
    let target = Sample::F32(SampleType::Packed);

    let take = |out: &frame::Audio, samples: &mut Vec<f32>| {
        let n = out.samples() * 2;
        let data = out.data(0);
        let fl: &[f32] = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, n.min(data.len() / 4)) };
        samples.extend_from_slice(fl);
    };
    let drain = |dec: &mut ffmpeg_next::decoder::Audio, resampler: &mut Option<(resampling::Context, Sample, u64, u32)>, samples: &mut Vec<f32>| {
        let mut f = frame::Audio::empty();
        while dec.receive_frame(&mut f).is_ok() {
            let mut layout = f.channel_layout();
            // Plain PCM (WAV, AIFF) often says only how many channels it has, in no particular
            // order. The resampler is set up from a channel mask, so such a frame never matches it
            // ("input changed") and nothing decodes: use the standard layout for that many.
            if layout.channels() == 0 || layout.bits() == 0 {
                let n = if layout.channels() > 0 { layout.channels() as i32 } else { f.channels() as i32 };
                layout = ChannelLayout::default(n.max(1));
            }
            f.set_channel_layout(layout);
            let key = (f.format(), layout.bits(), f.rate());
            let rebuild = !matches!(resampler, Some((_, fm, l, r)) if (*fm, *l, *r) == key);
            if rebuild {
                match resampling::Context::get(f.format(), layout, f.rate(), target, ChannelLayout::STEREO, RATE) {
                    Ok(c) => *resampler = Some((c, key.0, key.1, key.2)),
                    Err(_) => return,
                }
            }
            let (rs, ..) = resampler.as_mut().unwrap();
            // Room for every output sample: ffmpeg-next would size the frame to the input, which
            // cuts off part of each frame when going up in rate (44.1 → 48 kHz loses 8%).
            let room = (f.samples() as u64 * RATE as u64).div_ceil(f.rate().max(1) as u64) as usize + 256;
            let mut out = frame::Audio::new(target, room, ChannelLayout::STEREO);
            if rs.run(&f, &mut out).is_ok() {
                take(&out, samples);
            }
        }
    };
    for (s, packet) in ictx.packets() {
        if s.index() != idx {
            continue;
        }
        if dec.send_packet(&packet).is_ok() {
            drain(&mut dec, &mut resampler, &mut samples);
        }
    }
    let _ = dec.send_eof();
    drain(&mut dec, &mut resampler, &mut samples);
    // What the resampler still holds at the end.
    if let Some((rs, ..)) = resampler.as_mut() {
        let mut out = frame::Audio::new(target, 4096, ChannelLayout::STEREO);
        if rs.flush(&mut out).is_ok() {
            take(&out, &mut samples);
        }
    }
    if samples.is_empty() {
        return Err(anyhow!("no samples"));
    }
    Ok(AudioBuf { samples })
}

fn registry() -> &'static RwLock<HashMap<String, Arc<AudioBuf>>> {
    static R: OnceLock<RwLock<HashMap<String, Arc<AudioBuf>>>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(HashMap::new()))
}
pub fn buffer(asset_id: &str) -> Option<Arc<AudioBuf>> {
    registry().read().get(asset_id).cloned()
}
pub fn set_buffer(asset_id: &str, b: AudioBuf) {
    peak_registry().write().insert(asset_id.to_string(), Arc::new(peaks_of(&b.samples)));
    if let Some(bt) = detect_beats(&b.samples) {
        beat_registry().write().insert(asset_id.to_string(), bt);
    }
    registry().write().insert(asset_id.to_string(), Arc::new(b));
}

/// Waveform resolution: one peak per 1/200 s.
pub const PEAK_RATE: f64 = 200.0;

/// The loudest sample (either channel) in each 1/200 s, for drawing waveforms.
pub fn peaks_of(samples: &[f32]) -> Vec<f32> {
    let per = (RATE as f64 / PEAK_RATE) as usize * 2;
    samples.chunks(per).map(|c| c.iter().fold(0f32, |m, v| m.max(v.abs())).min(1.0)).collect()
}
fn peak_registry() -> &'static RwLock<HashMap<String, Arc<Vec<f32>>>> {
    static R: OnceLock<RwLock<HashMap<String, Arc<Vec<f32>>>>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(HashMap::new()))
}
/// An imported file's waveform, once its sound has been decoded.
pub fn peaks(asset_id: &str) -> Option<Arc<Vec<f32>>> {
    peak_registry().read().get(asset_id).cloned()
}

// ---------- Beats ----------

/// A steady beat found in a sound: its tempo and where the first beat falls (seconds into the file).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beats {
    pub bpm: f64,
    pub first: f64,
}
impl Beats {
    pub fn period(&self) -> f64 {
        60.0 / self.bpm
    }
}
fn beat_registry() -> &'static RwLock<HashMap<String, Beats>> {
    static R: OnceLock<RwLock<HashMap<String, Beats>>> = OnceLock::new();
    R.get_or_init(|| RwLock::new(HashMap::new()))
}
/// The beat of an imported sound, when it has a steady one.
pub fn beats(asset_id: &str) -> Option<Beats> {
    beat_registry().read().get(asset_id).copied()
}

/// Tempo and phase from onsets: the rise in loudness every ~10 ms, its autocorrelation over
/// 70–180 BPM (leaning towards 120), then the offset where beats land on the most onsets.
pub fn detect_beats(samples: &[f32]) -> Option<Beats> {
    const HOP: usize = 480; // 10 ms at 48 kHz
    let frames = samples.len() / 2 / HOP;
    if frames < 600 {
        return None; // under 6 seconds: too little to tell
    }
    let mut energy = Vec::with_capacity(frames);
    for f in 0..frames {
        let ch = &samples[f * HOP * 2..(f + 1) * HOP * 2];
        let e: f32 = ch.chunks_exact(2).map(|p| ((p[0] + p[1]) * 0.5).powi(2)).sum::<f32>() / HOP as f32;
        energy.push((1e-6 + e).ln());
    }
    let mut onset: Vec<f32> = energy.windows(2).map(|w| (w[1] - w[0]).max(0.0)).collect();
    // Take away the local average so steady loud passages don't count as onsets.
    let avg: Vec<f32> = (0..onset.len()).map(|i| {
        let (a, b) = (i.saturating_sub(15), (i + 15).min(onset.len()));
        onset[a..b].iter().sum::<f32>() / (b - a) as f32
    }).collect();
    for (o, a) in onset.iter_mut().zip(avg) {
        *o = (*o - a).max(0.0);
    }
    let rate = 100.0;
    let (lo, hi) = ((rate * 60.0 / 180.0) as usize, (rate * 60.0 / 70.0) as usize);
    let mut scores = vec![0f32; hi + 2];
    for (lag, sc) in scores.iter_mut().enumerate().take(hi + 2).skip(lo.saturating_sub(1)) {
        let v: f32 = onset.iter().zip(&onset[lag..]).map(|(a, b)| a * b).sum();
        let bpm = rate * 60.0 / lag.max(1) as f64;
        let lean = (-(bpm / 120.0).log2().powi(2) / (2.0 * 0.6f64.powi(2))).exp() as f32;
        *sc = v * lean;
    }
    let best = (lo..=hi).max_by(|a, b| scores[*a].total_cmp(&scores[*b]))?;
    let total: f32 = (lo..=hi).map(|l| scores[l]).sum::<f32>() / (hi - lo + 1) as f32;
    if scores[best] < total * 1.35 {
        return None; // no clear pulse
    }
    // Refine the lag between frames.
    let (a, b, c) = (scores[best - 1] as f64, scores[best] as f64, scores[best + 1] as f64);
    let d = a - 2.0 * b + c;
    let lag = best as f64 + if d.abs() > 1e-12 { 0.5 * (a - c) / d } else { 0.0 };
    let period = lag / rate;
    // Phase: the offset whose beat grid sits on the most onset strength.
    let steps = (period * rate).round() as usize;
    let phase = (0..steps.max(1))
        .max_by(|x, y| {
            let sum = |o: usize| {
                let mut s = 0f32;
                let mut t = o as f64;
                while (t as usize) < onset.len() {
                    s += onset[t as usize];
                    t += lag;
                }
                s
            };
            sum(*x).total_cmp(&sum(*y))
        })
        .unwrap_or(0);
    Some(Beats { bpm: (60.0 / period * 10.0).round() / 10.0, first: (phase as f64 + 1.0) / rate })
}

#[cfg(test)]
mod beat_tests {
    use super::*;
    #[test]
    fn finds_a_click_track() {
        // 20 s of clicks at 128 BPM starting 0.3 s in.
        let rate = RATE as usize;
        let mut s = vec![0f32; rate * 20 * 2];
        let period = 60.0 / 128.0;
        let mut t = 0.3;
        while t < 19.9 {
            let i = (t * rate as f64) as usize;
            for k in 0..800 {
                let v = (1.0 - k as f32 / 800.0) * if k % 20 < 10 { 0.8 } else { -0.8 };
                s[(i + k) * 2] = v;
                s[(i + k) * 2 + 1] = v;
            }
            t += period;
        }
        let b = detect_beats(&s).expect("a beat");
        assert!((b.bpm - 128.0).abs() < 1.5, "{b:?}");
        let off = ((b.first - 0.3) / period).fract().abs();
        assert!(off < 0.06 || off > 0.94, "{b:?}");
    }
}

#[cfg(test)]
mod duck_tests {
    use super::*;
    use crate::model::store::{default_tracks, make_clip};
    use crate::model::types::{AssetType, Clip};

    #[test]
    fn music_dips_only_while_voice_plays() {
        // 4 s of steady "voice" (a loud tone), then nothing.
        let n = RATE as usize * 4;
        let samples: Vec<f32> = (0..n * 2).map(|i| ((i / 2) as f32 * 0.03).sin() * 0.5).collect();
        set_buffer("test-voice", AudioBuf { samples });
        let tracks = default_tracks();
        let music = |duck: f64| {
            let mut c = make_clip(&tracks, 0.0, "mu-golden", Some("A1"), Some(0.0)).unwrap();
            c.dur = 12.0;
            c.duck = duck;
            c
        };
        let mut voice: Clip = music(0.0);
        voice.id = "v".into();
        voice.asset = "test-voice".into();
        voice.kind = AssetType::Audio;
        voice.track = "A2".into();
        voice.dur = 4.0;
        let block = |clips: &[Clip], t0: f64| {
            let mut out = vec![0f32; 4800 * 2];
            crate::audio::mix::mix(&mut out, clips, &tracks, t0, RATE as f64, 0.0, f64::INFINITY, 1.0);
            out
        };
        let diff = |t0: f64| {
            let a = block(&[music(0.0), voice.clone()], t0);
            let b = block(&[music(1.0), voice.clone()], t0);
            let m = block(&[music(0.0)], t0);
            let d: f32 = a.iter().zip(&b).map(|(x, y)| (x - y).powi(2)).sum();
            let e: f32 = m.iter().map(|x| x * x).sum();
            (d / e.max(1e-9)).sqrt()
        };
        assert!(diff(2.0) > 0.7, "music should dip under the voice: {}", diff(2.0));
        assert!(diff(8.0) < 0.01, "music should be untouched with no voice: {}", diff(8.0));
    }
}
