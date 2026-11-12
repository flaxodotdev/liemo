// The design check: measures an edit for what most often goes wrong on screen, so neither the
// person nor the AI has to eyeball every frame. Text that's too fast to read, too small, too low
// in contrast with what's behind it, outside the title-safe area or colliding with other text;
// empty (black) stretches and flash frames; sound that clips; music that stops dead.

use crate::model::time::{tc, FPS, H, W};
use crate::model::types::{AssetType, Clip, Track};
use crate::render::canvas::{hex_rgb, Canvas};
use crate::render::compositor::{render_frame, Scene};
use crate::render::frames::DecoderPool;
use crate::storage::ProjectRecord;
use crate::ui::editor::geometry::clip_box;
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Issue {
    /// Where to look, in seconds.
    pub time: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clip: Option<String>,
    /// readability, contrast, size, safe-area, collision, gap, flash, audio, ending
    pub kind: &'static str,
    pub message: String,
}

fn issue(time: f64, clip: Option<&Clip>, kind: &'static str, message: String) -> Issue {
    Issue { time, clip: clip.map(|c| c.id.clone()), kind, message }
}

fn shown(c: &Clip, tracks: &[Track]) -> bool {
    !tracks.iter().find(|t| t.id == c.track).is_some_and(|t| t.hidden)
}

/// Relative luminance (WCAG) of an sRGB colour in 0..1.
fn lum(rgb: [f32; 3]) -> f32 {
    let f = |c: f32| if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
    0.2126 * f(rgb[0]) + 0.7152 * f(rgb[1]) + 0.0722 * f(rgb[2])
}
fn contrast(a: f32, b: f32) -> f32 {
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// Run every check. The project's canvas size and frame rate must be the current environment.
pub fn run(rec: &ProjectRecord, cancel: &dyn Fn() -> bool) -> Vec<Issue> {
    let (clips, tracks) = (&rec.clips, &rec.tracks);
    let end = crate::model::store::project_end(clips);
    let mut out = vec![];
    let texts: Vec<&Clip> = clips.iter().filter(|c| c.kind == AssetType::Text && shown(c, tracks)).collect();
    let k = W() / 1280.0;

    for c in &texts {
        let t = c.text.as_ref().unwrap();
        let words = t.content.split_whitespace().count();
        // About three words a second, and never under a second for more than a word.
        let need = (words as f64 / 3.0).max(if words > 1 { 1.0 } else { 0.5 }) + 0.3;
        if c.dur + 1e-6 < need {
            out.push(issue(c.start, Some(c), "readability", format!("\"{}\" is on screen {:.1} s for {words} words; it needs about {:.1} s to be read.", c.name, c.dur, need)));
        }
        let px = t.size * c.scale * k as f64;
        if px < 26.0 * k as f64 {
            out.push(issue(c.start, Some(c), "size", format!("\"{}\" is small ({:.0} px at 1280 wide); below about 26 it's hard to read on a phone.", c.name, px / k as f64)));
        }
        // Title-safe: inside 5% of each edge, at the middle of the clip.
        let b = clip_box(&c.posed(c.start + c.dur / 2.0));
        let (mx, my) = (W() * 0.05, H() * 0.05);
        if b.cx - b.w / 2.0 < mx || b.cx + b.w / 2.0 > W() - mx || b.cy - b.h / 2.0 < my || b.cy + b.h / 2.0 > H() - my {
            out.push(issue(c.start, Some(c), "safe-area", format!("\"{}\" reaches past the title-safe area; edges can be cropped on TVs and covered by app buttons on phones.", c.name)));
        }
    }

    // Text over text.
    for (i, a) in texts.iter().enumerate() {
        for b in texts.iter().skip(i + 1) {
            let (s, e) = (a.start.max(b.start), (a.start + a.dur).min(b.start + b.dur));
            if e - s < 0.1 {
                continue;
            }
            let m = (s + e) / 2.0;
            let (ra, rb) = (clip_box(&a.posed(m)), clip_box(&b.posed(m)));
            if (ra.cx - rb.cx).abs() * 2.0 < ra.w + rb.w && (ra.cy - rb.cy).abs() * 2.0 < ra.h + rb.h {
                out.push(issue(s, Some(b), "collision", format!("\"{}\" and \"{}\" overlap on screen at {}.", a.name, b.name, tc(s))));
            }
        }
    }

    // Contrast: the frame behind each text, without that text, against its colour.
    let mut pool = DecoderPool::new(640, 640);
    for c in &texts {
        if cancel() {
            return out;
        }
        let t = c.text.as_ref().unwrap();
        let backed = t.boxed || t.highlight.is_some() || t.blend != crate::model::types::TextBlend::Normal || (t.outline.is_some() && t.outline_width >= 4.0);
        if backed {
            continue;
        }
        let at = c.start + (c.dur / 2.0).min(1.5);
        let others: Vec<Clip> = clips.iter().filter(|o| o.id != c.id && !(o.kind == AssetType::Text && o.track == c.track)).cloned().collect();
        let s = 0.25;
        let mut cv = Canvas::new((W() * s) as u32, (H() * s) as u32);
        cv.scale(s, s);
        render_frame(&mut cv, &Scene { clips: &others, tracks }, at, &mut pool);
        let pm = cv.into_pixmap();
        let b = clip_box(&c.posed(at));
        let (x0, x1) = (((b.cx - b.w / 2.0) * s).max(0.0) as u32, (((b.cx + b.w / 2.0) * s) as u32).min(pm.width()));
        let (y0, y1) = (((b.cy - b.h / 2.0) * s).max(0.0) as u32, (((b.cy + b.h / 2.0) * s) as u32).min(pm.height()));
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        // The background's typical brightness, and its brightest tenth (busy, light patches).
        let mut ls = vec![];
        for y in (y0..y1).step_by(2) {
            for x in (x0..x1).step_by(2) {
                let p = pm.pixel(x, y).unwrap().demultiply();
                ls.push(lum([p.red() as f32 / 255.0, p.green() as f32 / 255.0, p.blue() as f32 / 255.0]));
            }
        }
        ls.sort_by(f32::total_cmp);
        let fg = lum(hex_rgb(&t.color).map(|v| v / 255.0));
        let worst = if fg > 0.4 { ls[ls.len() * 9 / 10] } else { ls[ls.len() / 10] };
        let ratio = contrast(fg, worst);
        let need = if t.shadow.is_some() || t.glow.is_some() { 2.2 } else { 3.0 };
        if ratio < need {
            out.push(issue(at, Some(c), "contrast", format!("\"{}\" has {:.1}:1 contrast with the picture behind it (aim for {need}:1). Add a backing box, outline or shadow, change its colour, or use the Invert blend.", c.name, ratio)));
        }
    }

    // Empty frames: no visible picture anywhere.
    let step = 1.0 / FPS();
    let mut t = 0.0;
    let mut gap: Option<f64> = None;
    while t < end {
        let covered = clips.iter().any(|c| c.visual() && c.kind != AssetType::Text && shown(c, tracks) && t >= c.start && t < c.start + c.dur);
        match (covered, gap) {
            (false, None) => gap = Some(t),
            (true, Some(g)) => {
                out.push(issue(g, None, "gap", format!("Black from {} to {} ({} frames): nothing is in the picture.", tc(g), tc(t), ((t - g) * FPS()).round())));
                gap = None;
            }
            _ => {}
        }
        t += step;
    }
    if let Some(g) = gap {
        out.push(issue(g, None, "gap", format!("Black from {} to the end: nothing is in the picture.", tc(g))));
    }

    // Flash clips: a picture shown for a blink.
    for c in clips.iter().filter(|c| c.visual() && shown(c, tracks)) {
        if c.dur < 4.0 / FPS() {
            out.push(issue(c.start, Some(c), "flash", format!("\"{}\" lasts {} frames, which reads as a glitch. Lengthen or remove it.", c.name, (c.dur * FPS()).round())));
        }
    }

    // Music that stops dead at the end.
    for c in clips.iter().filter(|c| c.kind == AssetType::Music || (c.kind == AssetType::Audio && c.track == "A1")) {
        if (c.start + c.dur - end).abs() < 0.05 && c.fade_out < 0.3 {
            out.push(issue(c.start + c.dur - 1.0, Some(c), "ending", format!("The music \"{}\" stops dead at the end. Fade it out (1–3 s).", c.name)));
        }
    }

    // Sound that clips.
    if !cancel() {
        if let Some(buf) = crate::audio::mix::render_range(clips, tracks, 0.0, end, cancel) {
            let rate = crate::media::audio::RATE as f64;
            let mut last = -10.0;
            for (i, v) in buf.iter().enumerate() {
                if v.abs() > 1.0 {
                    let at = (i / 2) as f64 / rate;
                    if at - last > 2.0 {
                        out.push(issue(at, None, "audio", format!("The mix clips (goes over full scale) at {}. Turn the loudest clip down there.", tc(at))));
                        last = at;
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.time.total_cmp(&b.time));
    out
}

/// Check a project as the MCP tools see it.
pub fn check_record(rec: &ProjectRecord) -> Vec<Issue> {
    let old = crate::model::time::env();
    crate::model::time::set_env(rec.env());
    crate::mcp::tools::ensure_media(rec, true);
    let r = run(rec, &|| false);
    crate::model::time::set_env(old);
    r
}
