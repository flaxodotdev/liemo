// Live audio output. The audio callback renders the timeline mix block by block and is also the
// playback clock: the editor reads the playhead from here while playing, so picture follows sound.

pub mod fx;
pub mod mix;
pub mod record;

use crate::model::assets::Asset;
use crate::model::types::{Clip, Track};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Instant;

struct State {
    clips: Arc<Vec<Clip>>,
    tracks: Arc<Vec<Track>>,
    playing: bool,
    /// Timeline time of the next sample to render.
    pos: f64,
    /// Where this run of playback started; earlier beats aren't replayed.
    from: f64,
    /// When the last block was handed to the device, and the position at its start.
    stamp: Option<(Instant, f64)>,
    preview: Option<(Arc<Asset>, f64)>,
    gain: f32,
}

pub struct AudioEngine {
    state: Arc<Mutex<State>>,
    _stream: Option<cpal::Stream>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let state = Arc::new(Mutex::new(State {
            clips: Arc::new(vec![]),
            tracks: Arc::new(vec![]),
            playing: false,
            pos: 0.0,
            from: 0.0,
            stamp: None,
            preview: None,
            gain: 0.0,
        }));
        // RAYNA_MUTE=1 runs without an output device (the playhead then follows the wall clock).
        let muted = std::env::var("RAYNA_MUTE").map(|v| v == "1").unwrap_or(false);
        let (stream, _rate) = match if muted { None } else { Self::open(state.clone()) } {
            Some((s, r)) => (Some(s), r),
            None => (None, 48000.0),
        };
        AudioEngine { state, _stream: stream }
    }

    pub fn available(&self) -> bool {
        self._stream.is_some()
    }

    fn open(state: Arc<Mutex<State>>) -> Option<(cpal::Stream, f64)> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let cfg = device.default_output_config().ok()?;
        if cfg.sample_format() != cpal::SampleFormat::F32 {
            return None;
        }
        let config: cpal::StreamConfig = cfg.into();
        let sr = config.sample_rate as f64;
        let ch = config.channels as usize;
        let mut buf: Vec<f32> = vec![];
        let stream = device
            .build_output_stream::<f32, _, _>(
                config,
                move |data: &mut [f32], _| {
                    let frames = data.len() / ch.max(1);
                    buf.clear();
                    buf.resize(frames * 2, 0.0);
                    let mut s = state.lock();
                    if s.playing {
                        let (clips, tracks, pos, from) = (s.clips.clone(), s.tracks.clone(), s.pos, s.from);
                        drop(s);
                        mix::mix(&mut buf, &clips, &tracks, pos, sr, from, f64::INFINITY, 0.8);
                        s = state.lock();
                        if s.playing && (s.pos - pos).abs() < 1e-9 {
                            s.stamp = Some((Instant::now(), pos));
                            s.pos = pos + frames as f64 / sr;
                        }
                    }
                    if let Some((a, t)) = s.preview.clone() {
                        let mut pv = vec![0f32; frames * 2];
                        mix::preview(&mut pv, &a, t, sr);
                        for (o, v) in buf.iter_mut().zip(pv) {
                            *o += v;
                        }
                        let beat = 60.0 / a.bpm;
                        let next = t + frames as f64 / sr;
                        s.preview = if next > beat * 16.0 + 0.3 { None } else { Some((a, next)) };
                    }
                    // Short ramps on start and stop so playback never clicks.
                    let target = if s.playing || s.preview.is_some() { 1.0 } else { 0.0 };
                    let mut g = s.gain;
                    drop(s);
                    for f in 0..frames {
                        g += (target - g) * 0.004;
                        for c in 0..ch {
                            data[f * ch + c] = buf[f * 2 + c.min(1)] * g;
                        }
                    }
                    state.lock().gain = g;
                },
                |e| eprintln!("Liemo: audio output error: {e}"),
                None,
            )
            .ok()?;
        stream.play().ok()?;
        Some((stream, sr))
    }

    /// New edit to play (clips or tracks changed).
    pub fn set_timeline(&self, clips: Arc<Vec<Clip>>, tracks: Arc<Vec<Track>>) {
        let mut s = self.state.lock();
        s.clips = clips;
        s.tracks = tracks;
    }
    pub fn play(&self, at: f64) {
        let mut s = self.state.lock();
        s.playing = true;
        s.pos = at;
        s.from = at;
        s.stamp = None;
    }
    pub fn stop(&self) {
        let mut s = self.state.lock();
        s.playing = false;
        s.stamp = None;
    }
    /// The playhead while playing, interpolated between audio blocks.
    pub fn time(&self) -> Option<f64> {
        let s = self.state.lock();
        if !s.playing {
            return None;
        }
        let (at, pos) = s.stamp?;
        let block = s.pos - pos;
        Some(pos + at.elapsed().as_secs_f64().min(block.max(0.0) * 1.5))
    }
    pub fn preview(&self, a: Option<Arc<Asset>>) {
        self.state.lock().preview = a.map(|a| (a, 0.0));
    }
    pub fn previewing(&self) -> Option<String> {
        self.state.lock().preview.as_ref().map(|(a, _)| a.id.clone())
    }
}
