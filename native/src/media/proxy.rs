// Proxies: small editing copies of big videos (4K, high bit rates). The preview plays the proxy,
// export always reads the original. Proxies live in the data folder, named after the media id,
// and keep the original's timing frame for frame.

use anyhow::{anyhow, Result};
use ffmpeg_next::{codec, encoder, format, frame, media, software::scaling, util::format::Pixel, Dictionary, Packet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Preview with proxies where they exist (a project-independent setting).
pub static ENABLED: AtomicBool = AtomicBool::new(true);

/// Long side of a proxy picture.
pub const SIZE: u32 = 960;

pub fn path(media_id: &str) -> PathBuf {
    crate::storage::root().join("proxies").join(format!("{media_id}.mp4"))
}
pub fn exists(media_id: &str) -> bool {
    path(media_id).exists()
}
/// Whether a video is big enough for a proxy to help: above 1080p.
pub fn worth_it(width: u32, height: u32) -> bool {
    width.max(height) > 1920 || width.min(height) > 1080
}
/// The file the preview should decode for an asset: its proxy when there is one and proxies are on.
pub fn for_preview(media_id: &str, original: &Path) -> PathBuf {
    if ENABLED.load(Ordering::Relaxed) {
        let p = path(media_id);
        if p.exists() {
            return p;
        }
    }
    original.to_path_buf()
}

/// Transcode `src`'s picture to an H.264 proxy (keyframe every half second, no sound) at `dst`.
/// `progress` gets 0–1; `cancel` stops it and leaves nothing behind.
pub fn make(src: &Path, dst: &Path, cancel: &AtomicBool, progress: &dyn Fn(f64)) -> Result<()> {
    super::init();
    let mut ictx = format::input(src)?;
    let ist = ictx.streams().best(media::Type::Video).ok_or_else(|| anyhow!("no video stream"))?;
    let idx = ist.index();
    let itb = ist.time_base();
    let rate = ist.avg_frame_rate();
    let fps = if rate.numerator() > 0 { f64::from(rate) } else { 30.0 };
    let dur = if ictx.duration() > 0 { ictx.duration() as f64 / f64::from(ffmpeg_next::ffi::AV_TIME_BASE) } else { 0.0 };
    let mut ctx = codec::context::Context::from_parameters(ist.parameters())?;
    if let Ok(t) = std::thread::available_parallelism() {
        ctx.set_threading(codec::threading::Config { kind: codec::threading::Type::Frame, count: t.get().min(8) });
    }
    let mut dec = ctx.decoder().video()?;
    let (sw, sh) = (dec.width(), dec.height());
    let k = (SIZE as f64 / sw.max(sh).max(1) as f64).min(1.0);
    let even = |v: f64| ((v / 2.0).round() as u32 * 2).max(2);
    let (w, h) = (even(sw as f64 * k), even(sh as f64 * k));

    let name = ["h264_videotoolbox", "libx264", "mpeg4"].into_iter().find(|n| encoder::find_by_name(n).is_some()).ok_or_else(|| anyhow!("This build can't make proxies (no H.264 encoder)"))?;
    let codec = encoder::find_by_name(name).unwrap();
    if let Some(dir) = dst.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = dst.with_extension("part.mp4");
    let mut octx = format::output_as(&tmp, "mp4")?;
    let global = octx.format().flags().contains(format::Flags::GLOBAL_HEADER);
    let mut ost = octx.add_stream(codec)?;
    let mut enc = codec::context::Context::new_with_codec(codec).encoder().video()?;
    let pix = if name.contains("videotoolbox") { Pixel::NV12 } else { Pixel::YUV420P };
    enc.set_width(w);
    enc.set_height(h);
    enc.set_format(pix);
    enc.set_time_base(itb);
    enc.set_frame_rate(Some(rate));
    // Frequent keyframes so scrubbing the proxy is quick.
    enc.set_gop(((fps / 2.0).round() as u32).max(1));
    enc.set_max_b_frames(0);
    enc.set_bit_rate(5_000_000);
    if global {
        enc.set_flags(codec::Flags::GLOBAL_HEADER);
    }
    let mut opts = Dictionary::new();
    if name == "libx264" {
        opts.set("preset", "veryfast");
        opts.set("crf", "23");
    }
    let mut venc = enc.open_as_with(codec, opts)?;
    ost.set_parameters(&venc);
    ost.set_time_base(itb);
    octx.write_header()?;
    let otb = octx.stream(0).map(|s| s.time_base()).unwrap_or(itb);
    let mut scaler = scaling::Context::get(dec.format(), sw, sh, pix, w, h, scaling::Flags::BILINEAR)?;

    let drain = |venc: &mut encoder::Video, octx: &mut format::context::Output| -> Result<()> {
        let mut pkt = Packet::empty();
        while venc.receive_packet(&mut pkt).is_ok() {
            pkt.set_stream(0);
            pkt.rescale_ts(itb, otb);
            pkt.write_interleaved(octx)?;
        }
        Ok(())
    };
    let mut last_pts = i64::MIN;
    let mut push = |dec: &mut ffmpeg_next::decoder::Video, venc: &mut encoder::Video, octx: &mut format::context::Output, scaler: &mut scaling::Context| -> Result<()> {
        let mut f = frame::Video::empty();
        while dec.receive_frame(&mut f).is_ok() {
            let Some(pts) = f.timestamp().or(f.pts()) else { continue };
            if pts <= last_pts {
                continue; // muxers need rising timestamps
            }
            last_pts = pts;
            let mut out = frame::Video::empty();
            scaler.run(&f, &mut out)?;
            out.set_pts(Some(pts));
            venc.send_frame(&out)?;
            drain(venc, octx)?;
            if dur > 0.0 {
                progress((pts as f64 * f64::from(itb) / dur).clamp(0.0, 1.0));
            }
        }
        Ok(())
    };
    let r = (|| -> Result<()> {
        for (s, packet) in ictx.packets() {
            if cancel.load(Ordering::Relaxed) {
                return Err(anyhow!("cancelled"));
            }
            if s.index() != idx {
                continue;
            }
            if dec.send_packet(&packet).is_ok() {
                push(&mut dec, &mut venc, &mut octx, &mut scaler)?;
            }
        }
        let _ = dec.send_eof();
        push(&mut dec, &mut venc, &mut octx, &mut scaler)?;
        venc.send_eof()?;
        let mut pkt = Packet::empty();
        while venc.receive_packet(&mut pkt).is_ok() {
            pkt.set_stream(0);
            pkt.rescale_ts(itb, otb);
            pkt.write_interleaved(&mut octx)?;
        }
        octx.write_trailer()?;
        Ok(())
    })();
    match r {
        Ok(()) => {
            std::fs::rename(&tmp, dst)?;
            progress(1.0);
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Throw away a proxy (the preview goes back to the original).
pub fn remove(media_id: &str) {
    let _ = std::fs::remove_file(path(media_id));
}


#[cfg(test)]
mod tests {
    /// Run with LIEMO_PROXY_SRC=<a video> cargo test proxy -- --ignored
    #[test]
    #[ignore]
    fn makes_a_matching_proxy() {
        let src = std::path::PathBuf::from(std::env::var("LIEMO_PROXY_SRC").expect("LIEMO_PROXY_SRC"));
        let dst = std::env::temp_dir().join("liemo-proxy-test.mp4");
        let cancel = std::sync::atomic::AtomicBool::new(false);
        super::make(&src, &dst, &cancel, &|_| {}).unwrap();
        let (a, b) = (crate::media::video::probe(&src).unwrap(), crate::media::video::probe(&dst).unwrap());
        assert!(b.width.max(b.height) <= super::SIZE, "{}x{}", b.width, b.height);
        assert!((a.duration - b.duration).abs() < 0.1, "{} vs {}", a.duration, b.duration);
        // The same moment shows the same picture (brightness of a frame late in the clip).
        let mean = |p: &std::path::Path| {
            let mut d = crate::media::video::VideoDecoder::open(p).unwrap();
            d.set_max(64, 64);
            let f = d.frame_at(a.duration * 0.7).unwrap();
            f.data().iter().map(|v| *v as f64).sum::<f64>() / f.data().len() as f64
        };
        assert!((mean(&src) - mean(&dst)).abs() < 8.0);
        let _ = std::fs::remove_file(dst);
    }
}
