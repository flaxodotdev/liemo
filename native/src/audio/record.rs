// Recording from the microphone (or any input device): samples collect in memory while the
// stream runs, then land in a 32-bit float WAV in the app's data folder, ready to import.

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

fn device_name(d: &cpal::Device) -> Option<String> {
    d.description().ok().map(|x| x.name().to_string())
}
use parking_lot::Mutex;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub struct Recorder {
    _stream: cpal::Stream,
    data: Arc<Mutex<Vec<f32>>>,
    /// Peak of the last block, as f32 bits (for the level meter).
    peak: Arc<AtomicU32>,
    rate: u32,
    channels: u16,
    started: Instant,
    pub device: String,
}

/// The input device recordings come from, if there is one.
pub fn input_name() -> Option<String> {
    device_name(&cpal::default_host().default_input_device()?)
}

fn build<T>(device: &cpal::Device, config: &cpal::StreamConfig, data: Arc<Mutex<Vec<f32>>>, peak: Arc<AtomicU32>) -> Result<cpal::Stream>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let stream = device.build_input_stream::<T, _, _>(
        config.clone(),
        move |input: &[T], _| {
            let mut p = 0f32;
            let mut d = data.lock();
            d.reserve(input.len());
            for s in input {
                let v: f32 = s.to_sample();
                p = p.max(v.abs());
                d.push(v);
            }
            peak.store(p.to_bits(), Ordering::Relaxed);
        },
        |e| eprintln!("Liemo: recording error: {e}"),
        None,
    )?;
    Ok(stream)
}

impl Recorder {
    pub fn start() -> Result<Self> {
        let host = cpal::default_host();
        let device = host.default_input_device().ok_or_else(|| anyhow!("No microphone found. Connect one, or pick an input in System Settings › Sound."))?;
        let name = device_name(&device).unwrap_or_else(|| "Microphone".into());
        let cfg = device.default_input_config()?;
        let format = cfg.sample_format();
        let config: cpal::StreamConfig = cfg.into();
        let data = Arc::new(Mutex::new(Vec::with_capacity(config.sample_rate as usize * config.channels as usize * 60)));
        let peak = Arc::new(AtomicU32::new(0));
        let (d, p) = (data.clone(), peak.clone());
        let stream = match format {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, d, p)?,
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, d, p)?,
            cpal::SampleFormat::I32 => build::<i32>(&device, &config, d, p)?,
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, d, p)?,
            f => return Err(anyhow!("The microphone uses a sample format Liemo can't record ({f:?})")),
        };
        stream.play()?;
        Ok(Recorder { _stream: stream, data, peak, rate: config.sample_rate, channels: config.channels, started: Instant::now(), device: name })
    }

    pub fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
    /// Input level, 0–1, shaped so speech sits mid-meter.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.peak.load(Ordering::Relaxed)).clamp(0.0, 1.0).sqrt()
    }

    /// Stop and write the take. Returns the WAV's path.
    pub fn finish(self) -> Result<PathBuf> {
        let Recorder { _stream, data, rate, channels, .. } = self;
        drop(_stream);
        let samples = std::mem::take(&mut *data.lock());
        if samples.len() < (rate as usize * channels as usize) / 10 {
            return Err(anyhow!("That take was too short to keep"));
        }
        if samples.iter().all(|s| s.abs() < 1e-6) {
            return Err(anyhow!("The take was silent. Allow Liemo to use the microphone in System Settings › Privacy & Security › Microphone."));
        }
        let dir = crate::storage::root().join("recordings");
        std::fs::create_dir_all(&dir)?;
        let n = std::fs::read_dir(&dir).map(|r| r.count()).unwrap_or(0) + 1;
        let mut path = dir.join(format!("Recording {n}.wav"));
        let mut k = n;
        while path.exists() {
            k += 1;
            path = dir.join(format!("Recording {k}.wav"));
        }
        write_wav(&path, &samples, rate, channels)?;
        Ok(path)
    }
}

/// IEEE float WAV.
fn write_wav(path: &std::path::Path, samples: &[f32], rate: u32, channels: u16) -> Result<()> {
    let bytes = (samples.len() * 4) as u32;
    let mut out = Vec::with_capacity(bytes as usize + 58);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(50 + bytes).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&18u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes()); // WAVE_FORMAT_IEEE_FLOAT
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * channels as u32 * 4).to_le_bytes());
    out.extend_from_slice(&(channels * 4).to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(b"fact");
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&((samples.len() / channels.max(1) as usize) as u32).to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&bytes.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::File::create(path)?.write_all(&out)?;
    Ok(())
}
