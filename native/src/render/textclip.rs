// Text clips: drawing with their entrance animation, and the box they occupy (for hit-testing
// and handles on the preview). Both lay lines out identically.

use super::canvas::{hex, rgba, Canvas};
use super::text::{fill_text, measure, Align, Baseline, TextStyle};
use crate::model::assets::font_def;
use crate::model::time::{H, W};
use crate::model::types::{TextAlign, TextAnim, TextProps};

fn ease(p: f32) -> f32 {
    1.0 - (1.0 - p).powi(3)
}

struct Layout {
    lines: Vec<String>,
    sizes: Vec<f32>,
    lh: Vec<f32>,
    total: f32,
    base_x: f32,
}

fn layout(t: &TextProps, lines: Vec<String>) -> Layout {
    let n = lines.len();
    let sizes: Vec<f32> = (0..n).map(|i| if i > 0 && t.boxed && n > 1 { t.size as f32 * 0.6 } else { t.size as f32 }).collect();
    let k = if t.font == "Bebas Neue" { 0.95 } else { 1.12 };
    let lh: Vec<f32> = sizes.iter().map(|s| s * k).collect();
    let total = lh.iter().sum();
    let base_x = match t.align {
        TextAlign::Left => 120.0,
        TextAlign::Right => W() - 120.0,
        TextAlign::Center => W() / 2.0,
    };
    Layout { lines, sizes, lh, total, base_x }
}

fn style<'a>(t: &'a TextProps, i: usize, size: f32) -> TextStyle<'a> {
    let f = font_def(&t.font);
    TextStyle { family: f.family, weight: if i > 0 { 500.0 } else { t.weight as f32 }, size, spacing: f.track * size }
}

fn align(a: TextAlign) -> Align {
    match a {
        TextAlign::Left => Align::Left,
        TextAlign::Center => Align::Center,
        TextAlign::Right => Align::Right,
    }
}

/// Draw a text clip at local time `lt` (seconds into the clip), in 1280×720 units.
pub fn draw_text(c: &mut Canvas, t: &TextProps, lt: f32, alpha: f32) {
    let e = ease((lt / 0.6).min(1.0));
    let (mut ox, mut oy, mut a) = (0.0, 0.0, 1.0);
    match t.anim {
        TextAnim::Fade => a = e,
        TextAnim::Rise => {
            a = e;
            oy = (1.0 - e) * 40.0;
        }
        TextAnim::Slide => {
            a = e;
            ox = -(1.0 - e) * 80.0;
        }
        _ => {}
    }
    let mut lines: Vec<String> = t.content.split('\n').map(String::from).collect();
    if t.anim == TextAnim::Type {
        let mut n = (lt / 0.045).floor() as i64;
        lines = lines
            .into_iter()
            .map(|l| {
                let len = l.chars().count() as i64;
                let s: String = l.chars().take(n.max(0) as usize).collect();
                n -= len;
                s
            })
            .collect();
    }
    let lay = layout(t, lines);
    let mut y = H() / 2.0 + t.y as f32 - lay.total / 2.0 + oy;
    let widths: Vec<f32> = lay.lines.iter().enumerate().map(|(i, l)| measure(l, &style(t, i, lay.sizes[i]))).collect();
    let color = hex(&t.color);
    if t.boxed {
        let bw = widths.iter().cloned().fold(f32::MIN, f32::max) + 48.0;
        let bx = match t.align {
            TextAlign::Left => lay.base_x - 24.0,
            TextAlign::Right => lay.base_x - bw + 24.0,
            TextAlign::Center => lay.base_x - bw / 2.0,
        };
        c.set_alpha(alpha * a);
        c.set_fill_color(rgba(0, 0, 0, 0.55));
        c.begin_path();
        c.round_rect(bx + ox, y - 14.0, bw, lay.total + 28.0, 12.0);
        c.fill();
        if t.align == TextAlign::Left {
            let accent = if t.color == "#ffffff" { hex("#ff7a3d") } else { color };
            c.fill_rect_color(bx + ox, y - 14.0, 6.0, lay.total + 28.0, accent);
        }
    }
    for (i, l) in lay.lines.iter().enumerate() {
        c.set_alpha((if i > 0 { 0.75 } else { 1.0 }) * a * alpha);
        fill_text(c, l, lay.base_x + ox, y + lay.lh[i] / 2.0, &style(t, i, lay.sizes[i]), align(t.align), Baseline::Middle, color);
        y += lay.lh[i];
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The untransformed rectangle a text clip draws into (matches draw_text's layout).
pub fn text_rect(t: &TextProps) -> Rect {
    let lay = layout(t, t.content.split('\n').map(String::from).collect());
    let widths: Vec<f32> = lay
        .lines
        .iter()
        .enumerate()
        .map(|(i, l)| measure(if l.is_empty() { " " } else { l }, &style(t, i, lay.sizes[i])))
        .collect();
    let top = H() / 2.0 + t.y as f32 - lay.total / 2.0;
    let max_w = widths.iter().cloned().fold(8.0, f32::max);
    if t.boxed {
        let bw = max_w + 48.0;
        let bx = match t.align {
            TextAlign::Left => lay.base_x - 24.0,
            TextAlign::Right => lay.base_x - bw + 24.0,
            TextAlign::Center => lay.base_x - bw / 2.0,
        };
        return Rect { x: bx, y: top - 14.0, w: bw, h: lay.total + 28.0 };
    }
    let x = match t.align {
        TextAlign::Left => lay.base_x,
        TextAlign::Right => lay.base_x - max_w,
        TextAlign::Center => lay.base_x - max_w / 2.0,
    };
    Rect { x: x - 8.0, y: top - 4.0, w: max_w + 16.0, h: lay.total + 8.0 }
}
