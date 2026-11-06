// Export: video (MP4/WebM), animated GIF, a still frame, or audio only. Video is rendered frame by
// frame, audio is mixed offline, and both are encoded with FFmpeg. Nothing plays aloud and every
// frame is exact.

use crate::audio::mix::render_range;
use crate::model::store::project_end;
use crate::model::time::{H, W};
use crate::model::types::{Clip, Track};
use crate::render::canvas::{pixmap, resample_area, resize_bilinear, rgba, to_straight, Canvas};
use crate::render::compositor::{render_frame, Scene};
use crate::render::frames::DecoderPool;
use anyhow::{anyhow, bail, Result};
use ffmpeg_next::{codec, encoder, format, frame, software::scaling, util::channel_layout::ChannelLayout, util::format::sample::Sample, util::format::sample::Type as SampleType, util::format::Pixel, Dictionary, Packet, Rational};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Video,
    Gif,
    Image,
    Audio,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Container {
    Mp4,
    Webm,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Framing {
    Crop,
    Blur,
    Bars,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Low,
    Medium,
    High,
    Best,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RangeMode {
    All,
    Clip,
    Custom,
}

pub const RESOLUTIONS: [(u32, &str); 4] = [(720, "720p"), (1080, "1080p · Full HD"), (1440, "1440p · 2K"), (2160, "2160p · 4K")];
pub const ASPECTS: [(&str, &str, u32, u32); 8] = [
    ("16:9", "16:9 · Landscape", 16, 9),
    ("9:16", "9:16 · Vertical", 9, 16),
    ("1:1", "1:1 · Square", 1, 1),
    ("4:5", "4:5 · Portrait", 4, 5),
    ("2:3", "2:3 · Pin", 2, 3),
    ("4:3", "4:3 · Classic", 4, 3),
    ("21:9", "21:9 · Widescreen", 21, 9),
    ("1.85:1", "1.85:1 · Cinema", 37, 20),
];

/// "9:16" → (9, 16); "1.85:1" → (185, 100); unknown → 16:9.
fn aspect(a: &str) -> (u32, u32) {
    if let Some(x) = ASPECTS.iter().find(|x| x.0 == a) {
        return (x.2, x.3);
    }
    let parse = |s: &str| s.trim().parse::<f64>().ok().filter(|v| *v > 0.0);
    match a.split_once(':').map(|(l, r)| (parse(l), parse(r))) {
        Some((Some(l), Some(r))) => (((l * 100.0).round() as u32).max(1), ((r * 100.0).round() as u32).max(1)),
        _ => (16, 9),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub kind: Kind,
    pub range_mode: RangeMode,
    pub resolution: u32,
    pub aspect: String,
    pub framing: Framing,
    pub container: Container,
    pub fps: u32,
    pub quality: Quality,
    pub audio: bool,
    pub gif_width: u32,
    pub gif_fps: u32,
    pub image_png: bool,
    pub audio_wav: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Settings {
            kind: Kind::Video, range_mode: RangeMode::All, resolution: 1080, aspect: "16:9".into(), framing: Framing::Blur,
            container: Container::Mp4, fps: 30, quality: Quality::High, audio: true, gif_width: 480, gif_fps: 15, image_png: true, audio_wav: true,
        }
    }
}

/// Output pixel size: the short side follows the resolution; sides rounded to even numbers.
pub fn output_size(res: u32, asp: &str) -> (u32, u32) {
    let (w, h) = aspect(asp);
    let even = |n: f64| ((n / 2.0).round() * 2.0) as u32;
    if w >= h { (even(res as f64 * w as f64 / h as f64), res) } else { (res, even(res as f64 * h as f64 / w as f64)) }
}
pub fn gif_size(width: u32, asp: &str) -> (u32, u32) {
    let (w, h) = aspect(asp);
    (((width as f64 / 2.0).round() * 2.0) as u32, (((width as f64 * h as f64) / w as f64 / 2.0).round() * 2.0) as u32)
}

#[derive(Clone, Debug)]
pub struct Support {
    pub video: Option<&'static str>,
    pub video_label: &'static str,
    pub audio: Option<&'static str>,
    pub audio_label: &'static str,
}

fn find_encoder(names: &[&'static str]) -> Option<&'static str> {
    crate::media::init();
    names.iter().copied().find(|n| encoder::find_by_name(n).is_some())
}
fn video_candidates(c: Container) -> &'static [&'static str] {
    match c {
        Container::Mp4 => &["libx264", "h264_videotoolbox", "libx265", "hevc_videotoolbox", "mpeg4"],
        Container::Webm => &["libvpx-vp9", "libvpx"],
    }
}
fn audio_candidates(c: Container) -> &'static [&'static str] {
    match c {
        Container::Mp4 => &["aac", "aac_at"],
        Container::Webm => &["libopus", "opus", "libvorbis"],
    }
}
fn label(name: &str) -> &'static str {
    match name {
        "libx264" | "h264_videotoolbox" => "H.264",
        "libx265" | "hevc_videotoolbox" => "HEVC",
        "libvpx-vp9" => "VP9",
        "libvpx" => "VP8",
        "mpeg4" => "MPEG-4",
        "aac" | "aac_at" => "AAC",
        "libopus" | "opus" => "OPUS",
        "libvorbis" => "VORBIS",
        _ => "",
    }
}
/// Which encoders this build of FFmpeg has for a container.
pub fn probe(c: Container) -> Support {
    let v = find_encoder(video_candidates(c));
    let a = find_encoder(audio_candidates(c));
    Support { video: v, video_label: v.map(label).unwrap_or(""), audio: a, audio_label: a.map(label).unwrap_or("") }
}

pub struct Job {
    pub settings: Settings,
    pub start: f64,
    pub end: f64,
    pub image_time: f64,
    pub clips: Vec<Clip>,
    pub tracks: Vec<Track>,
    pub path: PathBuf,
}

pub struct Outcome {
    pub path: PathBuf,
    pub bytes: u64,
    pub with_audio: bool,
    pub seconds: f64,
    pub size: Option<(u32, u32)>,
}

pub struct Progress<'a> {
    pub cancel: &'a AtomicBool,
    pub report: &'a dyn Fn(f64, String),
}

fn check(p: &Progress) -> Result<()> {
    if p.cancel.load(Ordering::Relaxed) {
        bail!(CancelledError);
    }
    Ok(())
}
#[derive(Debug)]
pub struct CancelledError;
impl std::fmt::Display for CancelledError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("Export cancelled")
    }
}
impl std::error::Error for CancelledError {}

pub fn extension(s: &Settings) -> &'static str {
    match s.kind {
        Kind::Video => if s.container == Container::Mp4 { "mp4" } else { "webm" },
        Kind::Gif => "gif",
        Kind::Image => if s.image_png { "png" } else { "jpg" },
        Kind::Audio => if s.audio_wav { "wav" } else { "m4a" },
    }
}

/// A free file name in `dir`, adding " (2)", " (3)" … like a browser download.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let mut p = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    p
}

// ---------- Framing: draw the 16:9 edit into any output shape ----------
struct Framer {
    out_w: u32,
    out_h: u32,
    framing: Framing,
    exact: bool,
    src_w: u32,
    src_h: u32,
    pool: DecoderPool,
}
impl Framer {
    fn new(out_w: u32, out_h: u32, framing: Framing) -> Self {
        let exact = ((out_w as f32 / out_h as f32) - W() / H()).abs() < 0.01;
        let k = if exact {
            out_w as f32 / W()
        } else if framing == Framing::Crop {
            (out_w as f32 / W()).max(out_h as f32 / H())
        } else {
            (out_w as f32 / W()).min(out_h as f32 / H())
        };
        let (src_w, src_h) = if exact { (out_w, out_h) } else { ((W() * k).round() as u32, (H() * k).round() as u32) };
        Framer { out_w, out_h, framing, exact, src_w, src_h, pool: DecoderPool::new(src_w.max(1280), src_h.max(720)) }
    }
    fn draw(&mut self, scene: &Scene, t: f64) -> tiny_skia::Pixmap {
        let mut src = Canvas::from_pixmap(pixmap(self.src_w, self.src_h));
        render_frame(&mut src, scene, t, &mut self.pool);
        let src = src.into_pixmap();
        if self.exact {
            return src;
        }
        let (ow, oh) = (self.out_w, self.out_h);
        let mut og = Canvas::new(ow, oh);
        og.fill_all(tiny_skia::Color::BLACK);
        if self.framing == Framing::Blur {
            // A heavily blurred, darkened copy of the frame fills the space around the picture.
            let tw = 12u32;
            let th = ((12.0 * oh as f32) / ow as f32).round().max(1.0) as u32;
            let mut tiny = Canvas::new(tw, th);
            let cover = (tw as f32 / src.width() as f32).max(th as f32 / src.height() as f32);
            tiny.draw_image_scaled(&src, (tw as f32 - src.width() as f32 * cover) / 2.0, (th as f32 - src.height() as f32 * cover) / 2.0, src.width() as f32 * cover, src.height() as f32 * cover);
            let mid = resize_bilinear(&tiny.into_pixmap(), 96, ((96.0 * oh as f32) / ow as f32).round().max(1.0) as u32);
            let wash = resize_bilinear(&mid, ow, oh);
            og.draw_image(&wash, 0.0, 0.0);
            og.fill_rect_color(0.0, 0.0, ow as f32, oh as f32, rgba(0, 0, 0, 0.4));
        }
        og.draw_image(&src, ((ow as f32 - src.width() as f32) / 2.0).round(), ((oh as f32 - src.height() as f32) / 2.0).round());
        og.into_pixmap()
    }
}

/// Run an export to `job.path`.
pub fn run(job: &Job, p: &Progress) -> Result<Outcome> {
    let end = project_end(&job.clips);
    if end <= 0.0 {
        bail!("There's nothing on the timeline to export yet.");
    }
    let start = job.start.min(end).max(0.0);
    let stop = job.end.min(end).max(start);
    if job.settings.kind != Kind::Image && stop - start < 0.05 {
        bail!("The export range is empty. Choose a longer range.");
    }
    let r = match job.settings.kind {
        Kind::Image => export_image(job),
        Kind::Audio => export_audio(job, start, stop, p),
        Kind::Gif => export_gif(job, start, stop, p),
        Kind::Video => export_video(job, start, stop, p),
    };
    if r.is_err() {
        let _ = std::fs::remove_file(&job.path);
    }
    r
}

fn bytes_of(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

fn export_image(job: &Job) -> Result<Outcome> {
    let s = &job.settings;
    let (w, h) = output_size(s.resolution, &s.aspect);
    let mut f = Framer::new(w, h, s.framing);
    let pm = f.draw(&Scene { clips: &job.clips, tracks: &job.tracks }, job.image_time);
    let rgba = to_straight(&pm);
    if s.image_png {
        image::save_buffer(&job.path, &rgba, w, h, image::ColorType::Rgba8)?;
    } else {
        let rgb: Vec<u8> = rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect();
        let file = std::fs::File::create(&job.path)?;
        image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(file), 92).encode(&rgb, w, h, image::ExtendedColorType::Rgb8)?;
    }
    Ok(Outcome { bytes: bytes_of(&job.path), path: job.path.clone(), with_audio: false, seconds: 0.0, size: Some((w, h)) })
}

fn export_gif(job: &Job, start: f64, stop: f64, p: &Progress) -> Result<Outcome> {
    let s = &job.settings;
    let (w, h) = gif_size(s.gif_width, &s.aspect);
    let mut framer = Framer::new(w, h, s.framing);
    let frames = (((stop - start) * s.gif_fps as f64).round() as usize).max(1);
    let delay = (100.0 / s.gif_fps as f64).round() as u16;
    let file = std::fs::File::create(&job.path)?;
    let mut enc = gif::Encoder::new(std::io::BufWriter::new(file), w as u16, h as u16, &[])?;
    enc.set_repeat(gif::Repeat::Infinite)?;
    let scene = Scene { clips: &job.clips, tracks: &job.tracks };
    for f in 0..frames {
        check(p)?;
        let t = start + f as f64 / s.gif_fps as f64;
        let pm = framer.draw(&scene, t);
        let mut rgba = to_straight(&pm);
        let mut frame = gif::Frame::from_rgba_speed(w as u16, h as u16, &mut rgba, 10);
        frame.delay = delay;
        enc.write_frame(&frame)?;
        if f % 2 == 0 || f == frames - 1 {
            (p.report)((f + 1) as f64 / frames as f64 * 0.97, format!("Rendering frame {} of {frames}", f + 1));
        }
    }
    drop(enc);
    (p.report)(1.0, "Done".into());
    Ok(Outcome { bytes: bytes_of(&job.path), path: job.path.clone(), with_audio: false, seconds: stop - start, size: Some((w, h)) })
}

fn write_wav(path: &Path, samples: &[f32]) -> Result<()> {
    use std::io::Write;
    let (ch, sr) = (2u16, crate::media::audio::RATE);
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&ch.to_le_bytes());
    out.extend_from_slice(&sr.to_le_bytes());
    out.extend_from_slice(&(sr * ch as u32 * 2).to_le_bytes());
    out.extend_from_slice(&(ch * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &x in samples {
        let x = x.clamp(-1.0, 1.0);
        let v = if x < 0.0 { x * 32768.0 } else { x * 32767.0 } as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::File::create(path)?.write_all(&out)?;
    Ok(())
}

fn export_audio(job: &Job, start: f64, stop: f64, p: &Progress) -> Result<Outcome> {
    (p.report)(0.05, "Mixing audio".into());
    let buf = render_range(&job.clips, &job.tracks, start, stop, &|| p.cancel.load(Ordering::Relaxed));
    check(p)?;
    let buf = buf.ok_or_else(|| anyhow!("There's no audio in this range to export. Add music, or unmute a track."))?;
    if job.settings.audio_wav {
        (p.report)(0.9, "Writing WAV".into());
        write_wav(&job.path, &buf)?;
    } else {
        let name = find_encoder(&["aac", "aac_at"]).ok_or_else(|| anyhow!("This build can't encode M4A audio. Choose WAV instead."))?;
        (p.report)(0.5, "Encoding audio".into());
        let mut octx = format::output_as(&job.path, "ipod")?;
        let mut a = AudioOut::new(&mut octx, name, audio_bitrate(job.settings.quality))?;
        octx.write_header()?;
        a.fix_time_base(&octx);
        a.push(&mut octx, &buf, true)?;
        octx.write_trailer()?;
    }
    (p.report)(1.0, "Done".into());
    Ok(Outcome { bytes: bytes_of(&job.path), path: job.path.clone(), with_audio: true, seconds: stop - start, size: None })
}

fn audio_bitrate(q: Quality) -> usize {
    match q {
        Quality::Low => 96_000,
        Quality::Medium => 128_000,
        Quality::High => 192_000,
        Quality::Best => 256_000,
    }
}

// ---------- Audio encoding ----------
struct AudioOut {
    enc: encoder::Audio,
    stream: usize,
    tb_out: Rational,
    frame_size: usize,
    fmt: Sample,
    written: usize,
    /// Interleaved samples not yet sent (less than a frame).
    pending: Vec<f32>,
}
impl AudioOut {
    fn new(octx: &mut format::context::Output, name: &str, bitrate: usize) -> Result<Self> {
        let global = octx.format().flags().contains(format::Flags::GLOBAL_HEADER);
        let codec = encoder::find_by_name(name).ok_or_else(|| anyhow!("audio encoder missing"))?;
        let mut ost = octx.add_stream(codec)?;
        let stream = ost.index();
        let mut enc = codec::context::Context::new_with_codec(codec).encoder().audio()?;
        let ac = codec.audio()?;
        let fmt = ac
            .formats()
            .and_then(|mut f| f.find(|s| matches!(s, Sample::F32(_) | Sample::I16(_))))
            .unwrap_or(Sample::F32(SampleType::Planar));
        enc.set_rate(crate::media::audio::RATE as i32);
        enc.set_channel_layout(ChannelLayout::STEREO);
        enc.set_format(fmt);
        enc.set_bit_rate(bitrate);
        enc.set_time_base(Rational(1, crate::media::audio::RATE as i32));
        if global {
            enc.set_flags(codec::Flags::GLOBAL_HEADER);
        }
        let enc = enc.open_as(codec)?;
        ost.set_parameters(&enc);
        ost.set_time_base(Rational(1, crate::media::audio::RATE as i32));
        let frame_size = if enc.frame_size() > 0 { enc.frame_size() as usize } else { 1024 };
        Ok(AudioOut { enc, stream, tb_out: Rational(1, crate::media::audio::RATE as i32), frame_size, fmt, written: 0, pending: vec![] })
    }
    fn fix_time_base(&mut self, octx: &format::context::Output) {
        self.tb_out = octx.stream(self.stream).map(|s| s.time_base()).unwrap_or(self.tb_out);
    }
    fn drain(&mut self, octx: &mut format::context::Output) -> Result<()> {
        let mut pkt = Packet::empty();
        while self.enc.receive_packet(&mut pkt).is_ok() {
            pkt.set_stream(self.stream);
            pkt.rescale_ts(Rational(1, crate::media::audio::RATE as i32), self.tb_out);
            pkt.write_interleaved(octx)?;
        }
        Ok(())
    }
    /// Seconds of audio already sent.
    fn time(&self) -> f64 {
        self.written as f64 / crate::media::audio::RATE as f64
    }
    fn send(&mut self, octx: &mut format::context::Output, chunk: &[f32]) -> Result<()> {
        let n = chunk.len() / 2;
        let mut f = frame::Audio::new(self.fmt, n, ChannelLayout::STEREO);
        f.set_rate(crate::media::audio::RATE);
        match self.fmt {
            Sample::F32(SampleType::Planar) => {
                for c in 0..2 {
                    let plane = f.plane_mut::<f32>(c);
                    for i in 0..n {
                        plane[i] = chunk[i * 2 + c];
                    }
                }
            }
            Sample::F32(SampleType::Packed) => {
                f.plane_mut::<f32>(0)[..n * 2].copy_from_slice(chunk);
            }
            Sample::I16(SampleType::Planar) => {
                for c in 0..2 {
                    let plane = f.plane_mut::<i16>(c);
                    for i in 0..n {
                        plane[i] = (chunk[i * 2 + c].clamp(-1.0, 1.0) * 32767.0) as i16;
                    }
                }
            }
            _ => {
                let plane = f.plane_mut::<i16>(0);
                for (o, v) in plane.iter_mut().zip(chunk) {
                    *o = (v.clamp(-1.0, 1.0) * 32767.0) as i16;
                }
            }
        }
        f.set_pts(Some(self.written as i64));
        self.written += n;
        self.enc.send_frame(&f)?;
        self.drain(octx)
    }
    /// Feed interleaved stereo; with `last`, pad and flush.
    fn push(&mut self, octx: &mut format::context::Output, samples: &[f32], last: bool) -> Result<()> {
        self.pending.extend_from_slice(samples);
        let fs = self.frame_size * 2;
        while self.pending.len() >= fs {
            let chunk: Vec<f32> = self.pending.drain(..fs).collect();
            self.send(octx, &chunk)?;
        }
        if last {
            if !self.pending.is_empty() {
                let mut chunk = std::mem::take(&mut self.pending);
                chunk.resize(fs, 0.0);
                self.send(octx, &chunk)?;
            }
            self.enc.send_eof()?;
            self.drain(octx)?;
        }
        Ok(())
    }
}

// ---------- Video ----------
fn video_opts(name: &str, q: Quality, w: u32, h: u32, fps: u32) -> (Dictionary<'static>, Option<usize>) {
    let mut d = Dictionary::new();
    let pick = |a: &'static str, b: &'static str, c: &'static str, e: &'static str| match q {
        Quality::Low => a,
        Quality::Medium => b,
        Quality::High => c,
        Quality::Best => e,
    };
    match name {
        "libx264" | "libx265" => {
            d.set("preset", "medium");
            d.set("crf", pick("30", "26", "21", "17"));
            (d, None)
        }
        "libvpx-vp9" | "libvpx" => {
            d.set("crf", pick("40", "34", "30", "24"));
            d.set("deadline", "good");
            d.set("cpu-used", "4");
            d.set("row-mt", "1");
            (d, Some(0))
        }
        _ => {
            // Hardware and fallback encoders take a bitrate.
            let px = (w * h) as f64 * fps as f64;
            let bpp = match q {
                Quality::Low => 0.05,
                Quality::Medium => 0.08,
                Quality::High => 0.12,
                Quality::Best => 0.2,
            };
            (d, Some((px * bpp) as usize))
        }
    }
}

fn export_video(job: &Job, start: f64, stop: f64, p: &Progress) -> Result<Outcome> {
    let s = &job.settings;
    let (w, h) = output_size(s.resolution, &s.aspect);
    let fps = s.fps.max(1);
    (p.report)(0.0, "Checking encoders".into());
    let support = probe(s.container);
    let vname = support.video.ok_or_else(|| anyhow!("This build can't encode {} video. Try the other format.", if s.container == Container::Mp4 { "MP4" } else { "WebM" }))?;

    (p.report)(0.01, "Mixing audio".into());
    let audio = if s.audio && support.audio.is_some() { render_range(&job.clips, &job.tracks, start, stop, &|| p.cancel.load(Ordering::Relaxed)) } else { None };
    check(p)?;

    crate::media::init();
    let mut octx = format::output_as(&job.path, if s.container == Container::Mp4 { "mp4" } else { "webm" })?;
    let global = octx.format().flags().contains(format::Flags::GLOBAL_HEADER);
    let codec = encoder::find_by_name(vname).ok_or_else(|| anyhow!("video encoder missing"))?;
    let mut ost = octx.add_stream(codec)?;
    let vstream = ost.index();
    let mut enc = codec::context::Context::new_with_codec(codec).encoder().video()?;
    let pix = if vname.contains("videotoolbox") { Pixel::NV12 } else { Pixel::YUV420P };
    enc.set_width(w);
    enc.set_height(h);
    enc.set_format(pix);
    enc.set_time_base(Rational(1, fps as i32));
    enc.set_frame_rate(Some(Rational(fps as i32, 1)));
    enc.set_gop(fps * 2);
    if global {
        enc.set_flags(codec::Flags::GLOBAL_HEADER);
    }
    let (opts, bitrate) = video_opts(vname, s.quality, w, h, fps);
    if let Some(b) = bitrate {
        enc.set_bit_rate(b);
    }
    let mut venc = enc.open_as_with(codec, opts)?;
    ost.set_parameters(&venc);
    ost.set_time_base(Rational(1, fps as i32));
    // MP4 players expect BT.709 for HD; tag it so colours aren't shifted.
    let mut aout = match (&audio, support.audio) {
        (Some(_), Some(an)) => Some(AudioOut::new(&mut octx, an, audio_bitrate(if s.quality == Quality::Low { Quality::Low } else { Quality::High }))?),
        _ => None,
    };
    octx.write_header()?;
    let vtb = octx.stream(vstream).map(|s| s.time_base()).unwrap_or(Rational(1, fps as i32));
    if let Some(a) = &mut aout {
        a.fix_time_base(&octx);
    }

    let mut scaler = scaling::Context::get(Pixel::RGBA, w, h, pix, w, h, scaling::Flags::BICUBIC)?;
    let mut framer = Framer::new(w, h, s.framing);
    let scene = Scene { clips: &job.clips, tracks: &job.tracks };
    let frames = (((stop - start) * fps as f64).round() as usize).max(1);
    let audio_buf = audio.as_deref().unwrap_or(&[]);
    let mut audio_at = 0usize; // interleaved index
    let drain_video = |venc: &mut encoder::Video, octx: &mut format::context::Output| -> Result<()> {
        let mut pkt = Packet::empty();
        while venc.receive_packet(&mut pkt).is_ok() {
            pkt.set_stream(vstream);
            pkt.rescale_ts(Rational(1, fps as i32), vtb);
            pkt.write_interleaved(octx)?;
        }
        Ok(())
    };
    let result = (|| -> Result<()> {
        for f in 0..frames {
            check(p)?;
            let local = f as f64 / fps as f64;
            // Audio fed alongside in one-second slices so the muxer can interleave by time.
            if let Some(a) = &mut aout {
                while a.time() <= local + 1.0 && audio_at < audio_buf.len() {
                    let next = (audio_at + 48000 * 2).min(audio_buf.len());
                    a.push(&mut octx, &audio_buf[audio_at..next], false)?;
                    audio_at = next;
                }
            }
            let pm = framer.draw(&scene, start + local);
            let mut rgb = frame::Video::new(Pixel::RGBA, w, h);
            let stride = rgb.stride(0);
            let src = to_straight(&pm);
            let row = w as usize * 4;
            let dst = rgb.data_mut(0);
            for y in 0..h as usize {
                dst[y * stride..y * stride + row].copy_from_slice(&src[y * row..(y + 1) * row]);
            }
            let mut yuv = frame::Video::empty();
            scaler.run(&rgb, &mut yuv)?;
            yuv.set_pts(Some(f as i64));
            venc.send_frame(&yuv)?;
            drain_video(&mut venc, &mut octx)?;
            if f % 3 == 0 || f == frames - 1 {
                (p.report)(0.03 + 0.93 * ((f + 1) as f64 / frames as f64), format!("Rendering frame {} of {frames}", f + 1));
            }
        }
        if let Some(a) = &mut aout {
            a.push(&mut octx, &audio_buf[audio_at..], true)?;
        }
        (p.report)(0.98, "Finishing the file".into());
        venc.send_eof()?;
        drain_video(&mut venc, &mut octx)?;
        octx.write_trailer()?;
        Ok(())
    })();
    result?;
    (p.report)(1.0, "Done".into());
    let _ = resample_area;
    Ok(Outcome { bytes: bytes_of(&job.path), path: job.path.clone(), with_audio: aout.is_some(), seconds: stop - start, size: Some((w, h)) })
}

