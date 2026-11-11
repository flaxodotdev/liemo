// Video scopes from a preview frame: luma waveform, RGB parade, vectorscope and histogram, drawn
// as small images.

use tiny_skia::Pixmap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Waveform,
    Parade,
    Vector,
    Histogram,
}
pub const SCOPES: [(Scope, &str); 4] = [(Scope::Waveform, "Waveform"), (Scope::Parade, "Parade"), (Scope::Vector, "Vector"), (Scope::Histogram, "Histogram")];

pub const W: u32 = 256;
pub const H: u32 = 160;

/// Straight RGB of every `step`-th pixel.
fn samples(pm: &Pixmap, step: usize) -> impl Iterator<Item = (u32, [f32; 3])> + '_ {
    let w = pm.width() as usize;
    pm.data().chunks_exact(4).enumerate().step_by(step).filter(|(_, p)| p[3] > 0).map(move |(i, p)| {
        let a = p[3] as f32 / 255.0;
        ((i % w) as u32, [p[0] as f32 / 255.0 / a, p[1] as f32 / 255.0 / a, p[2] as f32 / 255.0 / a])
    })
}

/// Accumulate hits into a W×H grid per channel, then turn counts into glow.
struct Acc {
    v: Vec<[u32; 3]>,
}
impl Acc {
    fn new() -> Self {
        Acc { v: vec![[0; 3]; (W * H) as usize] }
    }
    fn hit(&mut self, x: u32, y: u32, ch: usize) {
        if x < W && y < H {
            self.v[(y * W + x) as usize][ch] += 1;
        }
    }
    fn image(&self, colors: [[f32; 3]; 3], gain: f32) -> Pixmap {
        let mut pm = Pixmap::new(W, H).unwrap();
        for (i, px) in pm.data_mut().chunks_exact_mut(4).enumerate() {
            let mut c = [0f32; 3];
            for ch in 0..3 {
                let n = self.v[i][ch];
                if n > 0 {
                    let k = (1.0 - (-(n as f32) * gain).exp()).min(1.0);
                    for j in 0..3 {
                        c[j] += colors[ch][j] * k;
                    }
                }
            }
            let a = c.iter().cloned().fold(0f32, f32::max).min(1.0);
            px.copy_from_slice(&[(c[0].min(1.0) * 255.0) as u8, (c[1].min(1.0) * 255.0) as u8, (c[2].min(1.0) * 255.0) as u8, (a * 255.0) as u8]);
        }
        pm
    }
}

fn luma(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

pub fn draw(pm: &Pixmap, kind: Scope) -> Pixmap {
    let n = (pm.width() * pm.height()) as usize;
    let step = (n / 120_000).max(1);
    let fw = pm.width().max(1) as f32;
    let mut acc = Acc::new();
    let white = [0.85, 0.95, 0.88];
    let y_of = |v: f32| ((1.0 - v.clamp(0.0, 1.0)) * (H - 1) as f32) as u32;
    match kind {
        Scope::Waveform => {
            for (x, c) in samples(pm, step) {
                acc.hit((x as f32 / fw * W as f32) as u32, y_of(luma(c)), 0);
            }
            acc.image([white, [0.0; 3], [0.0; 3]], 0.08)
        }
        Scope::Parade => {
            let third = W as f32 / 3.0;
            for (x, c) in samples(pm, step) {
                let fx = x as f32 / fw * (third - 2.0);
                for ch in 0..3 {
                    acc.hit((fx + third * ch as f32) as u32, y_of(c[ch]), ch);
                }
            }
            acc.image([[1.0, 0.25, 0.25], [0.3, 1.0, 0.35], [0.3, 0.5, 1.0]], 0.1)
        }
        Scope::Vector => {
            // Cb/Cr around the centre; a circle of saturation.
            let (cx, cy, r) = (W as f32 / 2.0, H as f32 / 2.0, H as f32 / 2.0 - 4.0);
            for (_, c) in samples(pm, step) {
                let y = luma(c);
                let cb = (c[2] - y) / 1.8556;
                let cr = (c[0] - y) / 1.5748;
                acc.hit((cx + cb * 2.0 * r) as u32, (cy - cr * 2.0 * r) as u32, 0);
            }
            let mut img = acc.image([white, [0.0; 3], [0.0; 3]], 0.12);
            // Graticule: the circle and the skin-tone line.
            let mut paint = |x: f32, y: f32| {
                let (x, y) = (x as i32, y as i32);
                if x >= 0 && y >= 0 && (x as u32) < W && (y as u32) < H {
                    let o = ((y as u32 * W + x as u32) * 4) as usize;
                    let d = img.data_mut();
                    d[o..o + 4].copy_from_slice(&[90, 90, 90, 255]);
                }
            };
            for i in 0..360 {
                let a = (i as f32).to_radians();
                paint(cx + a.cos() * r, cy + a.sin() * r);
            }
            for i in 0..(r as i32) {
                let a = 123f32.to_radians();
                paint(cx + a.cos() * i as f32, cy - a.sin() * i as f32);
            }
            img
        }
        Scope::Histogram => {
            let mut bins = [[0u32; 256]; 3];
            for (_, c) in samples(pm, step) {
                for ch in 0..3 {
                    bins[ch][(c[ch].clamp(0.0, 1.0) * 255.0) as usize] += 1;
                }
            }
            let top = bins.iter().flat_map(|b| b.iter()).cloned().max().unwrap_or(1).max(1) as f32;
            for ch in 0..3 {
                for (i, v) in bins[ch].iter().enumerate() {
                    let hgt = ((*v as f32 / top).sqrt() * (H - 2) as f32) as u32;
                    for y in 0..hgt {
                        for _ in 0..12 {
                            acc.hit(i as u32 * W / 256, H - 1 - y, ch);
                        }
                    }
                }
            }
            acc.image([[1.0, 0.3, 0.3], [0.3, 1.0, 0.4], [0.35, 0.55, 1.0]], 0.5)
        }
    }
}
