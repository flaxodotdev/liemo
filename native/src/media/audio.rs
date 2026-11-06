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

    let drain = |dec: &mut ffmpeg_next::decoder::Audio, resampler: &mut Option<(resampling::Context, Sample, u64, u32)>, samples: &mut Vec<f32>| {
        let mut f = frame::Audio::empty();
        while dec.receive_frame(&mut f).is_ok() {
            let mut layout = f.channel_layout();
            if layout.channels() == 0 {
                layout = ChannelLayout::default(f.channels() as i32);
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
            let mut out = frame::Audio::empty();
            if rs.run(&f, &mut out).is_ok() {
                let n = out.samples() * 2;
                let data = out.data(0);
                let fl: &[f32] = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const f32, n.min(data.len() / 4)) };
                samples.extend_from_slice(fl);
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
    registry().write().insert(asset_id.to_string(), Arc::new(b));
}
