// A small Canvas 2D work-alike over tiny-skia, so the scene, effect and transition code reads
// like the browser version: save/restore, transforms, globalAlpha, composite ops, paths, clips,
// gradients, patterns and drawImage. Path points are stored in device space (transformed when
// added), exactly as the canvas spec does.

use std::f32::consts::PI;
use std::sync::Arc;
use tiny_skia::{
    BlendMode, Color, FillRule, FilterQuality, GradientStop, IntSize, LineCap, LinearGradient, Mask, Paint, Path,
    PathBuilder, Pattern, Pixmap, PixmapPaint, Point, RadialGradient, Rect, Shader, SpreadMode, Stroke, Transform,
};


// ---------- Colours ----------
pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::from_rgba8(r, g, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}
pub fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba8(r, g, b, 255)
}
/// "#rgb", "#rrggbb" or "#rrggbbaa".
pub fn hex(s: &str) -> Color {
    let [r, g, b, a] = hex_rgba(s);
    Color::from_rgba8(r, g, b, a)
}
pub fn hex_rgba(s: &str) -> [u8; 4] {
    let h = s.trim().trim_start_matches('#');
    let h: String = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
    let p = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    [p(0), p(2), p(4), if h.len() >= 8 { p(6) } else { 255 }]
}
pub fn hex_rgb(s: &str) -> [f32; 3] {
    let [r, g, b, _] = hex_rgba(s);
    [r as f32, g as f32, b as f32]
}
/// Same colour, alpha multiplied.
pub fn with_alpha(c: Color, a: f32) -> Color {
    let mut c = c;
    c.apply_opacity(a.clamp(0.0, 1.0));
    c
}

/// Canvas `globalCompositeOperation` names.
pub fn blend_named(name: &str) -> BlendMode {
    match name {
        "screen" => BlendMode::Screen,
        "overlay" => BlendMode::Overlay,
        "multiply" => BlendMode::Multiply,
        "lighter" => BlendMode::Plus,
        "difference" => BlendMode::Difference,
        "exclusion" => BlendMode::Exclusion,
        "destination-in" => BlendMode::DestinationIn,
        "destination-over" => BlendMode::DestinationOver,
        "copy" => BlendMode::Source,
        "source-in" => BlendMode::SourceIn,
        "saturation" => BlendMode::Saturation,
        _ => BlendMode::SourceOver,
    }
}

// ---------- Paint styles ----------
#[derive(Clone)]
pub enum Fill {
    Solid(Color),
    Linear { x0: f32, y0: f32, x1: f32, y1: f32, stops: Vec<(f32, Color)> },
    Radial { x0: f32, y0: f32, r0: f32, x1: f32, y1: f32, r1: f32, stops: Vec<(f32, Color)> },
    Pattern { pm: Arc<Pixmap> },
}

impl Fill {
    /// Gradients and patterns are laid out in the user space current when painting, like canvas.
    fn shader(&self, alpha: f32, ts: Transform) -> Option<Shader<'_>> {
        let stops = |s: &[(f32, Color)]| s.iter().map(|(p, c)| GradientStop::new(*p, with_alpha(*c, alpha))).collect::<Vec<_>>();
        match self {
            Fill::Solid(c) => Some(Shader::SolidColor(with_alpha(*c, alpha))),
            Fill::Linear { x0, y0, x1, y1, stops: s } => {
                if (x0 - x1).abs() < 1e-6 && (y0 - y1).abs() < 1e-6 {
                    return s.last().map(|(_, c)| Shader::SolidColor(with_alpha(*c, alpha)));
                }
                LinearGradient::new(Point::from_xy(*x0, *y0), Point::from_xy(*x1, *y1), stops(s), SpreadMode::Pad, ts)
            }
            Fill::Radial { x0, y0, r0, x1, y1, r1, stops: s } => RadialGradient::new(
                Point::from_xy(*x0, *y0), r0.max(0.0), Point::from_xy(*x1, *y1), r1.max(0.001), stops(s), SpreadMode::Pad, ts,
            ),
            Fill::Pattern { pm } => Some(Pattern::new(pm.as_ref().as_ref(), SpreadMode::Repeat, FilterQuality::Nearest, alpha, ts)),
        }
    }
}

#[derive(Clone)]
struct St {
    ts: Transform,
    alpha: f32,
    blend: BlendMode,
    clip: Option<Arc<Mask>>,
    fill: Fill,
    stroke: Fill,
    line_width: f32,
    line_cap: LineCap,
    smooth: bool,
}

pub struct Canvas {
    pub pm: Pixmap,
    st: St,
    stack: Vec<St>,
    path: PathBuilder,
    has_point: bool,
}

pub fn pixmap(w: u32, h: u32) -> Pixmap {
    Pixmap::new(w.max(1), h.max(1)).expect("pixmap size")
}

impl Canvas {
    pub fn new(w: u32, h: u32) -> Self {
        Self::from_pixmap(pixmap(w, h))
    }
    pub fn from_pixmap(pm: Pixmap) -> Self {
        Self {
            pm,
            st: St {
                ts: Transform::identity(),
                alpha: 1.0,
                blend: BlendMode::SourceOver,
                clip: None,
                fill: Fill::Solid(Color::BLACK),
                stroke: Fill::Solid(Color::BLACK),
                line_width: 1.0,
                line_cap: LineCap::Butt,
                smooth: true,
            },
            stack: vec![],
            path: PathBuilder::new(),
            has_point: false,
        }
    }
    pub fn width(&self) -> u32 {
        self.pm.width()
    }
    pub fn height(&self) -> u32 {
        self.pm.height()
    }
    pub fn w(&self) -> f32 {
        self.pm.width() as f32
    }
    pub fn h(&self) -> f32 {
        self.pm.height() as f32
    }
    pub fn into_pixmap(self) -> Pixmap {
        self.pm
    }

    // ---------- State ----------
    pub fn save(&mut self) {
        self.stack.push(self.st.clone());
    }
    pub fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.st = s;
        }
    }
    /// Back to defaults: identity, alpha 1, source-over, no clip.
    pub fn reset_state(&mut self) {
        self.stack.clear();
        self.st.ts = Transform::identity();
        self.st.alpha = 1.0;
        self.st.blend = BlendMode::SourceOver;
        self.st.clip = None;
        self.st.smooth = true;
    }
    pub fn transform(&self) -> Transform {
        self.st.ts
    }
    pub fn set_transform(&mut self, ts: Transform) {
        self.st.ts = ts;
    }
    pub fn reset_transform(&mut self) {
        self.st.ts = Transform::identity();
    }
    pub fn translate(&mut self, x: f32, y: f32) {
        self.st.ts = self.st.ts.pre_translate(x, y);
    }
    pub fn scale(&mut self, sx: f32, sy: f32) {
        self.st.ts = self.st.ts.pre_scale(sx, sy);
    }
    /// Radians, like canvas.
    pub fn rotate(&mut self, a: f32) {
        self.st.ts = self.st.ts.pre_concat(Transform::from_rotate(a.to_degrees()));
    }
    pub fn alpha(&self) -> f32 {
        self.st.alpha
    }
    pub fn set_alpha(&mut self, a: f32) {
        self.st.alpha = a.clamp(0.0, 1.0);
    }
    pub fn blend(&self) -> BlendMode {
        self.st.blend
    }
    pub fn set_blend(&mut self, b: BlendMode) {
        self.st.blend = b;
    }
    pub fn set_op(&mut self, name: &str) {
        self.st.blend = blend_named(name);
    }
    pub fn set_fill(&mut self, f: Fill) {
        self.st.fill = f;
    }
    pub fn set_fill_color(&mut self, c: Color) {
        self.st.fill = Fill::Solid(c);
    }
    pub fn set_stroke_color(&mut self, c: Color) {
        self.st.stroke = Fill::Solid(c);
    }
    pub fn set_line_width(&mut self, w: f32) {
        self.st.line_width = w;
    }
    pub fn set_line_cap(&mut self, c: LineCap) {
        self.st.line_cap = c;
    }

    // ---------- Gradients (in user space at creation, like createLinearGradient) ----------
    pub fn linear(&self, x0: f32, y0: f32, x1: f32, y1: f32, stops: &[(f32, Color)]) -> Fill {
        Fill::Linear { x0, y0, x1, y1, stops: stops.to_vec() }
    }
    pub fn radial(&self, x0: f32, y0: f32, r0: f32, x1: f32, y1: f32, r1: f32, stops: &[(f32, Color)]) -> Fill {
        Fill::Radial { x0, y0, r0, x1, y1, r1, stops: stops.to_vec() }
    }
    pub fn pattern(&self, pm: Arc<Pixmap>) -> Fill {
        Fill::Pattern { pm }
    }

    // ---------- Paths ----------
    fn tp(&self, x: f32, y: f32) -> (f32, f32) {
        let mut p = [Point::from_xy(x, y)];
        self.st.ts.map_points(&mut p);
        (p[0].x, p[0].y)
    }
    pub fn begin_path(&mut self) {
        self.path = PathBuilder::new();
        self.has_point = false;
    }
    pub fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.tp(x, y);
        self.path.move_to(x, y);
        self.has_point = true;
    }
    pub fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.tp(x, y);
        if self.has_point {
            self.path.line_to(x, y);
        } else {
            self.path.move_to(x, y);
            self.has_point = true;
        }
    }
    fn cubic_dev(&mut self, c1: (f32, f32), c2: (f32, f32), p: (f32, f32)) {
        let (a, b, c) = (self.tp(c1.0, c1.1), self.tp(c2.0, c2.1), self.tp(p.0, p.1));
        self.path.cubic_to(a.0, a.1, b.0, b.1, c.0, c.1);
    }
    pub fn close_path(&mut self) {
        if self.has_point {
            self.path.close();
        }
    }
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.move_to(x, y);
        self.line_to(x + w, y);
        self.line_to(x + w, y + h);
        self.line_to(x, y + h);
        self.close_path();
    }
    /// Elliptical arc around (cx, cy), angles in radians, clockwise unless `ccw`.
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32, ccw: bool) {
        let mut sweep = a1 - a0;
        if !ccw {
            if sweep >= 2.0 * PI {
                sweep = 2.0 * PI;
            } else {
                sweep = sweep.rem_euclid(2.0 * PI);
                if sweep == 0.0 && a1 != a0 {
                    sweep = 2.0 * PI;
                }
            }
        } else if -sweep >= 2.0 * PI {
            sweep = -2.0 * PI;
        } else {
            sweep = -((-sweep).rem_euclid(2.0 * PI));
        }
        let start = (cx + rx * a0.cos(), cy + ry * a0.sin());
        self.line_to(start.0, start.1);
        let n = ((sweep.abs() / (PI / 2.0)).ceil() as usize).max(1);
        let step = sweep / n as f32;
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        let mut a = a0;
        for _ in 0..n {
            let b = a + step;
            let (ca, sa, cb, sb) = (a.cos(), a.sin(), b.cos(), b.sin());
            let c1 = (cx + rx * (ca - k * sa), cy + ry * (sa + k * ca));
            let c2 = (cx + rx * (cb + k * sb), cy + ry * (sb - k * cb));
            self.cubic_dev(c1, c2, (cx + rx * cb, cy + ry * sb));
            a = b;
        }
    }
    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32) {
        self.ellipse(cx, cy, r, r, a0, a1, false);
    }
    pub fn circle(&mut self, cx: f32, cy: f32, r: f32) {
        self.move_to(cx + r, cy);
        self.arc(cx, cy, r, 0.0, 2.0 * PI);
        self.close_path();
    }
    pub fn round_rect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32) {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        self.move_to(x + r, y);
        self.line_to(x + w - r, y);
        self.ellipse(x + w - r, y + r, r, r, -PI / 2.0, 0.0, false);
        self.line_to(x + w, y + h - r);
        self.ellipse(x + w - r, y + h - r, r, r, 0.0, PI / 2.0, false);
        self.line_to(x + r, y + h);
        self.ellipse(x + r, y + h - r, r, r, PI / 2.0, PI, false);
        self.line_to(x, y + r);
        self.ellipse(x + r, y + r, r, r, PI, 1.5 * PI, false);
        self.close_path();
    }
    fn finish_path(&self) -> Option<Path> {
        self.path.clone().finish()
    }

    fn paint_for<'a>(&self, f: &'a Fill) -> Option<Paint<'a>> {
        let shader = f.shader(self.st.alpha, self.st.ts)?;
        Some(Paint { shader, blend_mode: self.st.blend, anti_alias: true, ..Default::default() })
    }

    pub fn fill(&mut self) {
        let Some(path) = self.finish_path() else { return };
        let fill = self.st.fill.clone();
        if let Some(paint) = self.paint_for(&fill) {
            let clip = self.st.clip.clone();
            self.pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), clip.as_deref());
        }
    }
    pub fn fill_path_with(&mut self, path: &Path, fill: &Fill) {
        if let Some(paint) = self.paint_for(fill) {
            let clip = self.st.clip.clone();
            self.pm.fill_path(path, &paint, FillRule::Winding, Transform::identity(), clip.as_deref());
        }
    }
    pub fn stroke(&mut self) {
        let Some(path) = self.finish_path() else { return };
        let ts = self.st.ts;
        let k = (ts.sx * ts.sy - ts.kx * ts.ky).abs().sqrt().max(1e-6);
        let stroke = Stroke { width: (self.st.line_width * k).max(0.01), line_cap: self.st.line_cap, ..Default::default() };
        let fill = self.st.stroke.clone();
        if let Some(paint) = self.paint_for(&fill) {
            let clip = self.st.clip.clone();
            self.pm.stroke_path(&path, &paint, &stroke, Transform::identity(), clip.as_deref());
        }
    }
    /// Intersect the clip with the current path.
    pub fn clip(&mut self) {
        let Some(path) = self.finish_path() else {
            let mut m = Mask::new(self.width(), self.height()).unwrap();
            m.clear();
            self.st.clip = Some(Arc::new(m));
            return;
        };
        let m = match &self.st.clip {
            Some(old) => {
                let mut m = (**old).clone();
                m.intersect_path(&path, FillRule::Winding, true, Transform::identity());
                m
            }
            None => {
                let mut m = Mask::new(self.width(), self.height()).unwrap();
                m.fill_path(&path, FillRule::Winding, true, Transform::identity());
                m
            }
        };
        self.st.clip = Some(Arc::new(m));
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let saved = std::mem::replace(&mut self.path, PathBuilder::new());
        let had = self.has_point;
        self.begin_path();
        self.rect(x, y, w, h);
        self.fill();
        self.path = saved;
        self.has_point = had;
    }
    pub fn fill_rect_color(&mut self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        let old = std::mem::replace(&mut self.st.fill, Fill::Solid(c));
        self.fill_rect(x, y, w, h);
        self.st.fill = old;
    }
    pub fn fill_all(&mut self, c: Color) {
        self.pm.fill(c);
    }

    // ---------- Images ----------
    pub fn draw_image(&mut self, src: &Pixmap, dx: f32, dy: f32) {
        self.draw_image_sub(src, 0.0, 0.0, src.width() as f32, src.height() as f32, dx, dy, src.width() as f32, src.height() as f32);
    }
    pub fn draw_image_scaled(&mut self, src: &Pixmap, dx: f32, dy: f32, dw: f32, dh: f32) {
        self.draw_image_sub(src, 0.0, 0.0, src.width() as f32, src.height() as f32, dx, dy, dw, dh);
    }
    /// drawImage(src, sx, sy, sw, sh, dx, dy, dw, dh). The source rect is clipped to the image.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_image_sub(&mut self, src: &Pixmap, sx: f32, sy: f32, sw: f32, sh: f32, dx: f32, dy: f32, dw: f32, dh: f32) {
        if sw <= 0.0 || sh <= 0.0 || dw.abs() < 1e-6 || dh.abs() < 1e-6 || self.st.alpha <= 0.0 {
            return;
        }
        let (iw, ih) = (src.width() as f32, src.height() as f32);
        let (kx, ky) = (dw / sw, dh / sh);
        let (mut sx0, mut sy0, mut sx1, mut sy1) = (sx, sy, sx + sw, sy + sh);
        let (mut dx0, mut dy0) = (dx, dy);
        if sx0 < 0.0 { dx0 -= sx0 * kx; sx0 = 0.0; }
        if sy0 < 0.0 { dy0 -= sy0 * ky; sy0 = 0.0; }
        sx1 = sx1.min(iw);
        sy1 = sy1.min(ih);
        if sx1 <= sx0 || sy1 <= sy0 {
            return;
        }
        let (dw2, dh2) = ((sx1 - sx0) * kx, (sy1 - sy0) * ky);

        // Large reductions: pre-shrink with an area filter so the result is a proper average, like
        // a browser's high-quality smoothing, instead of a few sparse bilinear taps.
        let ts = self.st.ts;
        let dev_kx = (kx * (ts.sx * ts.sx + ts.ky * ts.ky).sqrt()).abs();
        let dev_ky = (ky * (ts.kx * ts.kx + ts.sy * ts.sy).sqrt()).abs();
        let shrink = self.st.smooth && (dev_kx < 0.6 || dev_ky < 0.6);
        let (owned, s_off_x, s_off_y, fx, fy);
        let source: &Pixmap = if shrink {
            let tw = (((sx1 - sx0) * dev_kx).round() as u32).max(1);
            let th = (((sy1 - sy0) * dev_ky).round() as u32).max(1);
            let cropped = crop(src, sx0, sy0, sx1 - sx0, sy1 - sy0);
            owned = resample_area(&cropped, tw.max(1), th.max(1));
            s_off_x = 0.0;
            s_off_y = 0.0;
            fx = (sx1 - sx0) / owned.width() as f32;
            fy = (sy1 - sy0) / owned.height() as f32;
            &owned
        } else {
            s_off_x = sx0;
            s_off_y = sy0;
            fx = 1.0;
            fy = 1.0;
            src
        };
        // The pattern maps source pixels into user space; fill_path then applies `ts` to both.
        let pts = Transform::identity()
            .pre_translate(dx0, dy0)
            .pre_scale(kx * fx, ky * fy)
            .pre_translate(-s_off_x / fx, -s_off_y / fy);
        let quality = if !self.st.smooth {
            FilterQuality::Nearest
        } else if is_integer_translate(&ts.pre_concat(pts)) {
            FilterQuality::Nearest
        } else {
            FilterQuality::Bilinear
        };
        let shader = Pattern::new(source.as_ref(), SpreadMode::Pad, quality, self.st.alpha, pts);
        let paint = Paint { shader, blend_mode: self.st.blend, anti_alias: true, ..Default::default() };
        if let Some(rect) = Rect::from_xywh(dx0, dy0, dw2, dh2) {
            let path = PathBuilder::from_rect(rect);
            let clip = self.st.clip.clone();
            self.pm.fill_path(&path, &paint, FillRule::Winding, ts, clip.as_deref());
        }
    }

    /// Fast path for compositing a same-size layer with alpha and blend (no transform).
    pub fn composite(&mut self, src: &Pixmap, alpha: f32, blend: BlendMode) {
        let paint = PixmapPaint { opacity: alpha.clamp(0.0, 1.0), blend_mode: blend, quality: FilterQuality::Nearest };
        let clip = self.st.clip.clone();
        self.pm.draw_pixmap(0, 0, src.as_ref(), &paint, Transform::identity(), clip.as_deref());
    }
}

fn is_integer_translate(t: &Transform) -> bool {
    (t.sx - 1.0).abs() < 1e-5 && (t.sy - 1.0).abs() < 1e-5 && t.kx.abs() < 1e-5 && t.ky.abs() < 1e-5
        && (t.tx - t.tx.round()).abs() < 1e-4 && (t.ty - t.ty.round()).abs() < 1e-4
}

/// Copy of a sub-rectangle (rounded outwards).
pub fn crop(src: &Pixmap, x: f32, y: f32, w: f32, h: f32) -> Pixmap {
    let x0 = x.floor().max(0.0) as u32;
    let y0 = y.floor().max(0.0) as u32;
    let x1 = ((x + w).ceil() as u32).min(src.width());
    let y1 = ((y + h).ceil() as u32).min(src.height());
    if x0 == 0 && y0 == 0 && x1 == src.width() && y1 == src.height() {
        return src.clone();
    }
    let (cw, ch) = ((x1.saturating_sub(x0)).max(1), (y1.saturating_sub(y0)).max(1));
    let mut out = pixmap(cw, ch);
    let sw = src.width() as usize;
    let s = src.data();
    let d = out.data_mut();
    for row in 0..ch as usize {
        let sy = (y0 as usize + row).min(src.height() as usize - 1);
        let so = (sy * sw + x0 as usize) * 4;
        let n = (cw as usize * 4).min(s.len() - so);
        d[row * cw as usize * 4..row * cw as usize * 4 + n].copy_from_slice(&s[so..so + n]);
    }
    out
}

/// Area-average resample (box filter with fractional coverage). Works on premultiplied data,
/// which is the correct space to average in.
pub fn resample_area(src: &Pixmap, w: u32, h: u32) -> Pixmap {
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    let (w, h) = (w.max(1) as usize, h.max(1) as usize);
    if w == sw && h == sh {
        return src.clone();
    }
    if w > sw || h > sh {
        return resize_bilinear(src, w as u32, h as u32);
    }
    let s = src.data();
    let kx = sw as f32 / w as f32;
    let ky = sh as f32 / h as f32;
    // Horizontal pass into f32 rows.
    let mut tmp = vec![0f32; w * sh * 4];
    for x in 0..w {
        let a = x as f32 * kx;
        let b = a + kx;
        let (i0, i1) = (a.floor() as usize, (b.ceil() as usize).min(sw));
        for i in i0..i1 {
            let cov = ((i + 1) as f32).min(b) - (i as f32).max(a);
            if cov <= 0.0 {
                continue;
            }
            let wgt = cov / kx;
            for y in 0..sh {
                let si = (y * sw + i) * 4;
                let ti = (y * w + x) * 4;
                for c in 0..4 {
                    tmp[ti + c] += s[si + c] as f32 * wgt;
                }
            }
        }
    }
    let mut out = pixmap(w as u32, h as u32);
    let d = out.data_mut();
    let mut acc = vec![0f32; w * 4];
    for y in 0..h {
        acc.iter_mut().for_each(|v| *v = 0.0);
        let a = y as f32 * ky;
        let b = a + ky;
        let (j0, j1) = (a.floor() as usize, (b.ceil() as usize).min(sh));
        for j in j0..j1 {
            let cov = ((j + 1) as f32).min(b) - (j as f32).max(a);
            if cov <= 0.0 {
                continue;
            }
            let wgt = cov / ky;
            let row = &tmp[j * w * 4..(j + 1) * w * 4];
            for (v, r) in acc.iter_mut().zip(row) {
                *v += r * wgt;
            }
        }
        let o = &mut d[y * w * 4..(y + 1) * w * 4];
        for x in 0..w {
            let a = acc[x * 4 + 3].round().clamp(0.0, 255.0);
            for c in 0..3 {
                o[x * 4 + c] = acc[x * 4 + c].round().clamp(0.0, a) as u8;
            }
            o[x * 4 + 3] = a as u8;
        }
    }
    out
}

/// Bilinear resize (used for enlarging).
pub fn resize_bilinear(src: &Pixmap, w: u32, h: u32) -> Pixmap {
    let mut out = pixmap(w, h);
    let sx = w as f32 / src.width() as f32;
    let sy = h as f32 / src.height() as f32;
    let shader = Pattern::new(src.as_ref(), SpreadMode::Pad, FilterQuality::Bilinear, 1.0, Transform::from_scale(sx, sy));
    let paint = Paint { shader, blend_mode: BlendMode::Source, anti_alias: false, ..Default::default() };
    out.fill_rect(Rect::from_xywh(0.0, 0.0, w as f32, h as f32).unwrap(), &paint, Transform::identity(), None);
    out
}

/// Nearest-neighbour resize (pixel-art enlargements).
pub fn resize_nearest(src: &Pixmap, w: u32, h: u32) -> Pixmap {
    let mut out = pixmap(w, h);
    let sx = w as f32 / src.width() as f32;
    let sy = h as f32 / src.height() as f32;
    let shader = Pattern::new(src.as_ref(), SpreadMode::Pad, FilterQuality::Nearest, 1.0, Transform::from_scale(sx, sy));
    let paint = Paint { shader, blend_mode: BlendMode::Source, anti_alias: false, ..Default::default() };
    out.fill_rect(Rect::from_xywh(0.0, 0.0, w as f32, h as f32).unwrap(), &paint, Transform::identity(), None);
    out
}

/// Run `f` over every pixel with straight (unpremultiplied) RGBA in 0–255, then premultiply back.
pub fn map_pixels(pm: &mut Pixmap, mut f: impl FnMut(usize, &mut [f32; 4])) {
    let d = pm.data_mut();
    for (i, px) in d.chunks_exact_mut(4).enumerate() {
        let a = px[3];
        if a == 0 {
            continue;
        }
        let k = 255.0 / a as f32;
        let mut v = [px[0] as f32 * k, px[1] as f32 * k, px[2] as f32 * k, a as f32];
        f(i, &mut v);
        let a2 = v[3].round().clamp(0.0, 255.0);
        let m = a2 / 255.0;
        px[0] = (v[0].clamp(0.0, 255.0) * m).round() as u8;
        px[1] = (v[1].clamp(0.0, 255.0) * m).round() as u8;
        px[2] = (v[2].clamp(0.0, 255.0) * m).round() as u8;
        px[3] = a2 as u8;
    }
}

/// Straight RGBA bytes (like getImageData).
pub fn to_straight(pm: &Pixmap) -> Vec<u8> {
    let mut out = pm.data().to_vec();
    for px in out.chunks_exact_mut(4) {
        let a = px[3];
        if a != 0 && a != 255 {
            let k = 255.0 / a as f32;
            for c in px.iter_mut().take(3) {
                *c = (*c as f32 * k).round().min(255.0) as u8;
            }
        }
    }
    out
}

/// From straight RGBA bytes (like putImageData).
pub fn from_straight(data: &[u8], w: u32, h: u32) -> Pixmap {
    let mut v = data.to_vec();
    for px in v.chunks_exact_mut(4) {
        let a = px[3];
        if a != 255 {
            let m = a as f32 / 255.0;
            for c in px.iter_mut().take(3) {
                *c = (*c as f32 * m).round() as u8;
            }
        }
    }
    Pixmap::from_vec(v, IntSize::from_wh(w.max(1), h.max(1)).unwrap()).unwrap()
}

/// RGBA premultiplied → BGRA straight, the layout GPUI images use.
pub fn to_bgra(pm: &Pixmap) -> Vec<u8> {
    let mut out = to_straight(pm);
    for px in out.chunks_exact_mut(4) {
        px.swap(0, 2);
    }
    out
}
