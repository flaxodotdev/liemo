// Colour grading: 3D LUTs from .cube files, and lift/gamma/gain colour wheels (shadows, midtones,
// highlights) the way grading tools have them.

use crate::model::types::{ParamValue, Params};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tiny_skia::Pixmap;

/// A 3D colour lookup table: `size`³ RGB entries, red changing fastest (the .cube order).
#[derive(Debug)]
pub struct Lut {
    pub size: usize,
    pub data: Vec<[f32; 3]>,
    min: [f32; 3],
    max: [f32; 3],
    pub title: String,
}

/// Read an Adobe/Resolve .cube file (3D tables; 1D tables are refused).
pub fn parse_cube(text: &str) -> Result<Lut, String> {
    let (mut size, mut data, mut title) = (0usize, vec![], String::new());
    let (mut min, mut max) = ([0f32; 3], [1f32; 3]);
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        let mut it = l.split_whitespace();
        let first = it.next().unwrap_or("");
        let nums = |it: std::str::SplitWhitespace| -> Vec<f32> { it.filter_map(|v| v.parse().ok()).collect() };
        match first {
            "TITLE" => title = l[5..].trim().trim_matches('"').to_string(),
            "LUT_3D_SIZE" => size = it.next().and_then(|v| v.parse().ok()).ok_or("Bad LUT_3D_SIZE")?,
            "LUT_1D_SIZE" => return Err("This is a 1D LUT; Liemo uses 3D .cube LUTs".into()),
            "DOMAIN_MIN" => {
                let v = nums(it);
                if v.len() == 3 {
                    min = [v[0], v[1], v[2]];
                }
            }
            "DOMAIN_MAX" => {
                let v = nums(it);
                if v.len() == 3 {
                    max = [v[0], v[1], v[2]];
                }
            }
            _ if first.parse::<f32>().is_ok() => {
                let mut v = vec![first.parse::<f32>().unwrap()];
                v.extend(nums(it));
                if v.len() >= 3 {
                    data.push([v[0], v[1], v[2]]);
                }
            }
            _ => {}
        }
    }
    if size < 2 || size > 256 {
        return Err("This .cube file has no 3D table size".into());
    }
    if data.len() != size * size * size {
        return Err(format!("This .cube file should have {} entries but has {}", size * size * size, data.len()));
    }
    Ok(Lut { size, data, min, max, title })
}

/// LUTs by path, read once.
pub fn load_lut(path: &str) -> Result<Arc<Lut>, String> {
    static C: OnceLock<Mutex<HashMap<String, Arc<Lut>>>> = OnceLock::new();
    let c = C.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(l) = c.lock().get(path) {
        return Ok(l.clone());
    }
    let text = std::fs::read_to_string(Path::new(path)).map_err(|e| format!("Couldn't read the LUT: {e}"))?;
    let lut = Arc::new(parse_cube(&text)?);
    c.lock().insert(path.to_string(), lut.clone());
    Ok(lut)
}

impl Lut {
    /// Look up a colour (0–1 each), with trilinear interpolation.
    pub fn sample(&self, rgb: [f32; 3]) -> [f32; 3] {
        let n = self.size;
        let f = |i: usize| ((rgb[i] - self.min[i]) / (self.max[i] - self.min[i]).max(1e-6)).clamp(0.0, 1.0) * (n - 1) as f32;
        let (x, y, z) = (f(0), f(1), f(2));
        let (x0, y0, z0) = (x.floor() as usize, y.floor() as usize, z.floor() as usize);
        let (x1, y1, z1) = ((x0 + 1).min(n - 1), (y0 + 1).min(n - 1), (z0 + 1).min(n - 1));
        let (dx, dy, dz) = (x - x0 as f32, y - y0 as f32, z - z0 as f32);
        let at = |r: usize, g: usize, b: usize| self.data[r + g * n + b * n * n];
        let lerp = |a: [f32; 3], b: [f32; 3], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        let c00 = lerp(at(x0, y0, z0), at(x1, y0, z0), dx);
        let c10 = lerp(at(x0, y1, z0), at(x1, y1, z0), dx);
        let c01 = lerp(at(x0, y0, z1), at(x1, y0, z1), dx);
        let c11 = lerp(at(x0, y1, z1), at(x1, y1, z1), dx);
        lerp(lerp(c00, c10, dy), lerp(c01, c11, dy), dz)
    }
}

/// Run `f` on every pixel's straight (unpremultiplied) colour.
fn per_pixel(src: &Pixmap, dst: &mut Pixmap, f: impl Fn([f32; 3]) -> [f32; 3]) {
    let s = src.data();
    let d = dst.data_mut();
    for (i, px) in s.chunks_exact(4).enumerate() {
        let a = px[3] as f32 / 255.0;
        let o = i * 4;
        if a <= 0.0 {
            d[o..o + 4].copy_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        let c = f([px[0] as f32 / 255.0 / a, px[1] as f32 / 255.0 / a, px[2] as f32 / 255.0 / a]);
        for k in 0..3 {
            d[o + k] = (c[k].clamp(0.0, 1.0) * a * 255.0).round() as u8;
        }
        d[o + 3] = px[3];
    }
}

pub fn lut(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    let path = match p.get("file") {
        Some(ParamValue::Str(s)) if !s.is_empty() => s.clone(),
        _ => {
            *dst = src.clone();
            return;
        }
    };
    match load_lut(&path) {
        Ok(l) => per_pixel(src, dst, |c| l.sample(c)),
        Err(_) => *dst = src.clone(),
    }
}

/// A wheel's push towards a hue: zero-sum across red, green and blue, so it tints without
/// brightening.
pub fn wheel_vec(hue_deg: f64, amount: f64) -> [f32; 3] {
    let h = hue_deg.to_radians();
    let k = amount.clamp(0.0, 1.0);
    [(h.cos() * k) as f32, ((h - 2.0944).cos() * k) as f32, ((h - 4.1888).cos() * k) as f32]
}

/// Lift (shadows), gamma (midtones) and gain (highlights): each wheel a hue and amount, each with
/// a brightness slider; then saturation.
pub fn grade(src: &Pixmap, dst: &mut Pixmap, p: &Params) {
    let g = |k: &str| match p.get(k) {
        Some(ParamValue::Num(n)) => *n,
        _ => 0.0,
    };
    let lift = wheel_vec(g("lift_hue"), g("lift_amt"));
    let gamma = wheel_vec(g("gamma_hue"), g("gamma_amt"));
    let gain = wheel_vec(g("gain_hue"), g("gain_amt"));
    let (ll, gl, kl) = (g("lift_lum") as f32, g("gamma_lum") as f32, g("gain_lum") as f32);
    let sat = 1.0 + g("saturation") as f32 / 100.0;
    per_pixel(src, dst, |c| {
        let mut o = [0f32; 3];
        for i in 0..3 {
            let lift_i = lift[i] * 0.2 + ll * 0.25;
            let gain_i = 1.0 + gain[i] * 0.35 + kl * 0.5;
            let mut x = c[i] * gain_i + lift_i * (1.0 - c[i]);
            // Midtones bend the curve: up brightens the middle and keeps black and white.
            let gm = (1.0 + gamma[i] * 0.45 + gl * 0.6).max(0.1);
            x = x.max(0.0).powf(1.0 / gm);
            o[i] = x;
        }
        let l = 0.2126 * o[0] + 0.7152 * o[1] + 0.0722 * o[2];
        [l + (o[0] - l) * sat, l + (o[1] - l) * sat, l + (o[2] - l) * sat]
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_cube_round_trips() {
        let mut t = String::from("TITLE \"id\"\nLUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    t += &format!("{r} {g} {b}\n");
                }
            }
        }
        let l = parse_cube(&t).unwrap();
        let c = l.sample([0.25, 0.5, 0.75]);
        assert!((c[0] - 0.25).abs() < 1e-5 && (c[1] - 0.5).abs() < 1e-5 && (c[2] - 0.75).abs() < 1e-5);
        assert_eq!(l.title, "id");
        assert!(parse_cube("LUT_1D_SIZE 4\n0 0 0").is_err());
    }
}
