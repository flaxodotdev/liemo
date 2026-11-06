// Frame-accurate video decoding. Each decoder keeps its position: asking for a later time decodes
// forward, an earlier or far-away time seeks to the keyframe before it first. Only the frame that
// is returned gets converted to RGBA.

use crate::render::canvas::pixmap;
use anyhow::{anyhow, Result};
use ffmpeg_next::{codec, decoder, format, frame, media, software::scaling, util::format::Pixel, Rational};
use std::path::Path;
use std::sync::Arc;
use tiny_skia::Pixmap;

struct Decoded {
    pts: f64,
    raw: frame::Video,
    rgba: Option<Arc<Pixmap>>,
}

pub struct VideoDecoder {
    ictx: format::context::Input,
    stream: usize,
    dec: decoder::Video,
    tb: f64,
    start: f64,
    frame_dur: f64,
    scaler: Option<(scaling::Context, Pixel, u32, u32, u32, u32)>,
    cur: Option<Decoded>,
    pending: Option<Decoded>,
    eof: bool,
    /// Largest picture to produce; bigger sources are scaled down (aspect kept).
    max_w: u32,
    max_h: u32,
}

impl VideoDecoder {
    pub fn open(path: &Path) -> Result<Self> {
        super::init();
        let ictx = format::input(path)?;
        let st = ictx.streams().best(media::Type::Video).ok_or_else(|| anyhow!("no video stream"))?;
        let stream = st.index();
        let tb = f64::from(st.time_base());
        let start = if st.start_time() == ffmpeg_next::ffi::AV_NOPTS_VALUE { 0.0 } else { st.start_time() as f64 * tb };
        let rate = st.avg_frame_rate();
        let frame_dur = if rate.numerator() > 0 { 1.0 / f64::from(rate) } else { 1.0 / 30.0 };
        let mut ctx = codec::context::Context::from_parameters(st.parameters())?;
        if let Ok(threads) = std::thread::available_parallelism() {
            ctx.set_threading(codec::threading::Config { kind: codec::threading::Type::Frame, count: threads.get().min(8) });
        }
        let dec = ctx.decoder().video()?;
        Ok(Self { ictx, stream, dec, tb, start, frame_dur, scaler: None, cur: None, pending: None, eof: false, max_w: 1920, max_h: 1080 })
    }

    pub fn set_max(&mut self, w: u32, h: u32) {
        if (w, h) != (self.max_w, self.max_h) {
            self.max_w = w.max(2);
            self.max_h = h.max(2);
            if let Some(c) = &mut self.cur {
                c.rgba = None;
            }
        }
    }

    fn seek(&mut self, t: f64) {
        let ts = ((t.max(0.0)) * f64::from(ffmpeg_next::ffi::AV_TIME_BASE)) as i64;
        let _ = self.ictx.seek(ts, ..ts);
        self.dec.flush();
        self.cur = None;
        self.pending = None;
        self.eof = false;
    }

    fn decode_next(&mut self) -> Option<Decoded> {
        if let Some(p) = self.pending.take() {
            return Some(p);
        }
        loop {
            let mut f = frame::Video::empty();
            if self.dec.receive_frame(&mut f).is_ok() {
                let pts = f.timestamp().map(|v| v as f64 * self.tb - self.start).unwrap_or(0.0);
                return Some(Decoded { pts, raw: f, rgba: None });
            }
            if self.eof {
                return None;
            }
            let mut got = false;
            for (s, packet) in self.ictx.packets() {
                if s.index() == self.stream {
                    let _ = self.dec.send_packet(&packet);
                    got = true;
                    break;
                }
            }
            if !got {
                self.eof = true;
                let _ = self.dec.send_eof();
            }
        }
    }

    /// The frame showing at `t` seconds (the last one whose timestamp is at or before it).
    pub fn frame_at(&mut self, t: f64) -> Option<Arc<Pixmap>> {
        let near = self.cur.as_ref().map(|c| t >= c.pts - 1e-4 && t < c.pts + 2.5).unwrap_or(false);
        if !near {
            self.seek(t);
        } else if let Some(c) = &self.cur {
            // Still inside the current frame, and the next one (if decoded) starts after `t`.
            let next_after = self.pending.as_ref().map(|p| p.pts > t + 1e-3).unwrap_or(true);
            if t < c.pts + self.frame_dur * 0.999 && next_after {
                return self.convert_cur();
            }
        }
        loop {
            match self.decode_next() {
                Some(d) if d.pts <= t + 1e-3 => self.cur = Some(d),
                Some(d) => {
                    if self.cur.is_none() {
                        // Nothing before `t` (e.g. a stream starting late): show the first frame.
                        self.cur = Some(d);
                    } else {
                        self.pending = Some(d);
                    }
                    break;
                }
                None => break,
            }
        }
        self.convert_cur()
    }

    fn convert_cur(&mut self) -> Option<Arc<Pixmap>> {
        let (max_w, max_h) = (self.max_w, self.max_h);
        let c = self.cur.as_mut()?;
        if let Some(p) = &c.rgba {
            return Some(p.clone());
        }
        let (sw, sh, fmt) = (c.raw.width(), c.raw.height(), c.raw.format());
        if sw == 0 || sh == 0 {
            return None;
        }
        let k = (max_w as f64 / sw as f64).min(max_h as f64 / sh as f64).min(1.0);
        let (dw, dh) = (((sw as f64 * k).round() as u32).max(2), ((sh as f64 * k).round() as u32).max(2));
        let rebuild = !matches!(&self.scaler, Some((_, f, a, b, cw, ch)) if *f == fmt && *a == sw && *b == sh && *cw == dw && *ch == dh);
        if rebuild {
            let ctx = scaling::Context::get(fmt, sw, sh, Pixel::RGBA, dw, dh, scaling::Flags::BILINEAR).ok()?;
            self.scaler = Some((ctx, fmt, sw, sh, dw, dh));
        }
        let (scaler, ..) = self.scaler.as_mut()?;
        let mut out = frame::Video::empty();
        scaler.run(&c.raw, &mut out).ok()?;
        let mut pm = pixmap(dw, dh);
        let stride = out.stride(0);
        let src = out.data(0);
        let row = dw as usize * 4;
        let dst = pm.data_mut();
        for y in 0..dh as usize {
            dst[y * row..(y + 1) * row].copy_from_slice(&src[y * stride..y * stride + row]);
        }
        for px in dst.chunks_exact_mut(4) {
            px[3] = 255;
        }
        let p = Arc::new(pm);
        c.rgba = Some(p.clone());
        Some(p)
    }
}

pub struct Probe {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub has_video: bool,
    pub has_audio: bool,
}

pub fn probe(path: &Path) -> Result<Probe> {
    super::init();
    let ictx = format::input(path)?;
    let mut duration = if ictx.duration() > 0 { ictx.duration() as f64 / f64::from(ffmpeg_next::ffi::AV_TIME_BASE) } else { 0.0 };
    let v = ictx.streams().best(media::Type::Video);
    let (mut width, mut height) = (0, 0);
    if let Some(s) = &v {
        if let Ok(ctx) = codec::context::Context::from_parameters(s.parameters()) {
            if let Ok(d) = ctx.decoder().video() {
                width = d.width();
                height = d.height();
            }
        }
        if duration <= 0.0 && s.duration() > 0 {
            duration = s.duration() as f64 * f64::from(s.time_base());
        }
    }
    let has_audio = ictx.streams().best(media::Type::Audio).is_some();
    let _ = Rational(1, 1);
    Ok(Probe { duration, width, height, has_video: v.is_some(), has_audio })
}
