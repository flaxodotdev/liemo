// Sound effects for audio clips: speed (tape-style varispeed, or a WSOLA time-stretch that keeps
// the pitch), bass boost, a low-pass "muffle", Freeverb reverb and an 8D pan. Reverb and filters
// carry state from sample to sample, so a clip's whole sound is rendered once, off the audio
// thread, and cached; the mixer reads the result like any other decoded buffer.

use crate::media::audio::{buffer, AudioBuf, RATE};
use crate::model::types::AudioFx;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::f64::consts::PI;
use std::sync::{Arc, OnceLock};

/// A clip's reworked sound. Sample 0 is source time 0; one second of `buf` is `speed` seconds
/// of source.
pub struct Processed {
    pub buf: AudioBuf,
    pub speed: f64,
}

/// Ready-made settings, shown as one-click presets.
pub const PRESETS: [(&str, &str, AudioFx); 9] = [
    ("original", "Original", fx(1.0, false, 0.0, 0.6, 0.0, 0.0, 0.0)),
    ("slowed-reverb", "Slowed + reverb", fx(0.82, false, 0.45, 0.85, 0.25, 0.0, 0.0)),
    ("sped-up", "Sped up", fx(1.25, false, 0.0, 0.6, 0.0, 0.0, 0.0)),
    ("nightcore", "Nightcore", fx(1.35, false, 0.12, 0.5, 0.1, 0.0, 0.0)),
    ("daycore", "Daycore", fx(0.75, false, 0.3, 0.7, 0.15, 0.1, 0.0)),
    ("hall", "Concert hall", fx(1.0, false, 0.5, 0.9, 0.0, 0.0, 0.0)),
    ("lofi", "Lo-fi", fx(0.92, false, 0.2, 0.5, 0.2, 0.45, 0.0)),
    ("underwater", "Underwater", fx(0.95, false, 0.35, 0.7, 0.3, 0.85, 0.0)),
    ("8d", "8D audio", fx(1.0, false, 0.3, 0.75, 0.0, 0.0, 0.8)),
];
const fn fx(speed: f64, keep_pitch: bool, reverb: f64, room: f64, bass: f64, muffle: f64, pan8d: f64) -> AudioFx {
    AudioFx { speed, keep_pitch, reverb, room, bass, muffle, pan8d, eq_low: 0.0, eq_mid: 0.0, eq_high: 0.0 }
}

// ---------- Speed ----------

/// Varispeed: read the source faster or slower, so pitch moves with speed (Catmull-Rom).
fn varispeed(src: &[f32], speed: f64) -> Vec<f32> {
    let n = src.len() / 2;
    let out_n = (n as f64 / speed).floor() as usize;
    let mut out = vec![0f32; out_n * 2];
    let get = |i: isize, c: usize| -> f32 {
        if i < 0 || i as usize >= n {
            0.0
        } else {
            src[i as usize * 2 + c]
        }
    };
    for o in 0..out_n {
        let x = o as f64 * speed;
        let i = x.floor() as isize;
        let t = (x - i as f64) as f32;
        for c in 0..2 {
            let (p0, p1, p2, p3) = (get(i - 1, c), get(i, c), get(i + 1, c), get(i + 2, c));
            out[o * 2 + c] = p1 + 0.5 * t * (p2 - p0 + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)));
        }
    }
    out
}

/// WSOLA time-stretch: Hann-windowed grains overlap-added at a fixed output hop, each taken from
/// near its nominal input position where it best lines up with the previous grain's continuation.
fn stretch(src: &[f32], speed: f64) -> Vec<f32> {
    const N: usize = 2048;
    const HOP: usize = N / 2;
    const TOL: isize = 512;
    let n = src.len() / 2;
    let out_n = (n as f64 / speed).floor() as usize;
    let mut out = vec![0f32; (out_n + N) * 2];
    let win: Vec<f32> = (0..N).map(|i| (0.5 - 0.5 * (2.0 * PI * i as f64 / N as f64).cos()) as f32).collect();
    let mono = |i: isize| -> f32 {
        if i < 0 || i as usize >= n {
            0.0
        } else {
            src[i as usize * 2] + src[i as usize * 2 + 1]
        }
    };
    // Similarity of the grain at `cand` with the natural continuation at `nat`, over the overlap.
    let corr = |cand: isize, nat: isize, step: usize| -> f32 {
        let mut s = 0.0;
        let mut j = 0;
        while j < HOP {
            s += mono(cand + j as isize) * mono(nat + j as isize);
            j += step;
        }
        s
    };
    let mut prev: isize = 0;
    let mut k = 0usize;
    while k * HOP < out_n {
        let nominal = (k as f64 * HOP as f64 * speed) as isize;
        let pos = if k == 0 {
            0
        } else {
            let nat = prev + HOP as isize;
            // Coarse search, then refine around the best.
            let (mut best, mut best_v) = (nominal, f32::MIN);
            let mut d = -TOL;
            while d <= TOL {
                let v = corr(nominal + d, nat, 8);
                if v > best_v {
                    best_v = v;
                    best = nominal + d;
                }
                d += 8;
            }
            let coarse = best;
            for d in -8..=8 {
                let v = corr(coarse + d, nat, 2);
                if v > best_v {
                    best_v = v;
                    best = coarse + d;
                }
            }
            best.max(0)
        };
        let o0 = k * HOP;
        for j in 0..N {
            let si = pos + j as isize;
            if si < 0 || si as usize >= n {
                continue;
            }
            let w = win[j];
            out[(o0 + j) * 2] += src[si as usize * 2] * w;
            out[(o0 + j) * 2 + 1] += src[si as usize * 2 + 1] * w;
        }
        prev = pos;
        k += 1;
    }
    out.truncate(out_n * 2);
    out
}

// ---------- Filters ----------

#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [[f64; 2]; 2],
}
impl Biquad {
    fn new(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Biquad { b: [b0 / a0, b1 / a0, b2 / a0], a: [a1 / a0, a2 / a0], z: [[0.0; 2]; 2] }
    }
    /// RBJ low-pass.
    fn lowpass(f: f64, q: f64) -> Self {
        let w = 2.0 * PI * f / RATE as f64;
        let (c, al) = (w.cos(), w.sin() / (2.0 * q));
        Self::new((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + al, -2.0 * c, 1.0 - al)
    }
    /// RBJ low shelf, `db` of gain below `f`.
    fn low_shelf(f: f64, db: f64) -> Self {
        let a = 10f64.powf(db / 40.0);
        let w = 2.0 * PI * f / RATE as f64;
        let (c, s) = (w.cos(), w.sin());
        let al = s / 2.0 * 2f64.sqrt();
        let k = 2.0 * a.sqrt() * al;
        Self::new(
            a * ((a + 1.0) - (a - 1.0) * c + k),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - k),
            (a + 1.0) + (a - 1.0) * c + k,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - k,
        )
    }
    /// RBJ high shelf, `db` of gain above `f`.
    fn high_shelf(f: f64, db: f64) -> Self {
        let a = 10f64.powf(db / 40.0);
        let w = 2.0 * PI * f / RATE as f64;
        let (c, s) = (w.cos(), w.sin());
        let al = s / 2.0 * 2f64.sqrt();
        let k = 2.0 * a.sqrt() * al;
        Self::new(
            a * ((a + 1.0) + (a - 1.0) * c + k),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - k),
            (a + 1.0) - (a - 1.0) * c + k,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - k,
        )
    }
    /// RBJ peaking bell, `db` at `f`.
    fn peaking(f: f64, q: f64, db: f64) -> Self {
        let a = 10f64.powf(db / 40.0);
        let w = 2.0 * PI * f / RATE as f64;
        let (c, al) = (w.cos(), w.sin() / (2.0 * q));
        Self::new(1.0 + al * a, -2.0 * c, 1.0 - al * a, 1.0 + al / a, -2.0 * c, 1.0 - al / a)
    }
    fn run(&mut self, buf: &mut [f32]) {
        for f in buf.chunks_exact_mut(2) {
            for (ch, x) in f.iter_mut().enumerate() {
                let xi = *x as f64;
                let z = &mut self.z[ch];
                let y = self.b[0] * xi + z[0];
                z[0] = self.b[1] * xi - self.a[0] * y + z[1];
                z[1] = self.b[2] * xi - self.a[1] * y;
                *x = y as f32;
            }
        }
    }
}

// ---------- Reverb (Freeverb) ----------

struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}
impl Comb {
    fn tick(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let y = self.buf[self.i];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.i] = x + self.store * feedback;
        self.i = (self.i + 1) % self.buf.len();
        y
    }
}
struct Allpass {
    buf: Vec<f32>,
    i: usize,
}
impl Allpass {
    fn tick(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = x + b * 0.5;
        self.i = (self.i + 1) % self.buf.len();
        b - x
    }
}

/// Freeverb's tunings (at 44.1 kHz) scaled to our rate; the right channel is 23 samples longer.
fn reverb(buf: &mut [f32], wet: f32, room: f32) {
    const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
    const ALLPASS: [usize; 4] = [556, 441, 341, 225];
    let k = RATE as f64 / 44100.0;
    let sz = |n: usize, spread: usize| (((n + spread) as f64) * k) as usize;
    let mut combs: Vec<[Comb; 2]> = COMBS.iter().map(|&n| [0, 23].map(|sp| Comb { buf: vec![0.0; sz(n, sp)], i: 0, store: 0.0 })).collect();
    let mut aps: Vec<[Allpass; 2]> = ALLPASS.iter().map(|&n| [0, 23].map(|sp| Allpass { buf: vec![0.0; sz(n, sp)], i: 0 })).collect();
    let feedback = 0.7 + 0.28 * room;
    let damp = 0.45 - 0.2 * room;
    // 20–60 ms pre-delay, longer for bigger rooms.
    let pre = ((0.02 + 0.04 * room as f64) * RATE as f64) as usize;
    let mut pd = vec![[0f32; 2]; pre.max(1)];
    let mut pi = 0;
    let dry = 1.0 - 0.5 * wet;
    // The tail's energy grows with feedback; scale it out so every room size sits at about the
    // same loudness, and a full wet mix isn't louder than the dry sound.
    let wet_gain = wet * 1.45 * ((1.0 - feedback * feedback) / (1.0 - 0.952f32 * 0.952)).sqrt();
    for f in buf.chunks_exact_mut(2) {
        let input = (f[0] + f[1]) * 0.015;
        let delayed = pd[pi];
        pd[pi] = [input, input];
        pi = (pi + 1) % pd.len();
        let mut o = [0f32; 2];
        for ch in 0..2 {
            let mut s = 0.0;
            for c in combs.iter_mut() {
                s += c[ch].tick(delayed[ch], feedback, damp);
            }
            for a in aps.iter_mut() {
                s = a[ch].tick(s);
            }
            o[ch] = s;
        }
        // A little cross-feed keeps the stereo image wide but not split.
        let (l, r) = (o[0] * 0.85 + o[1] * 0.15, o[1] * 0.85 + o[0] * 0.15);
        f[0] = f[0] * dry + l * wet_gain;
        f[1] = f[1] * dry + r * wet_gain;
    }
}

// ---------- 8D ----------

/// Equal-power pan circling the listener every 8 seconds, with a slight dip as it passes behind.
fn pan8d(buf: &mut [f32], depth: f32) {
    let sr = RATE as f64;
    for (i, f) in buf.chunks_exact_mut(2).enumerate() {
        let ph = 2.0 * PI * i as f64 / sr / 8.0;
        let p = (ph.sin() as f32 * depth).clamp(-1.0, 1.0);
        let behind = 1.0 - 0.15 * depth * (0.5 - 0.5 * ph.cos() as f32);
        let a = (p + 1.0) * std::f32::consts::FRAC_PI_4;
        let m = (f[0] + f[1]) * 0.5;
        let (l, r) = (f[0] * (1.0 - depth) + m * depth * a.cos() * std::f32::consts::SQRT_2, f[1] * (1.0 - depth) + m * depth * a.sin() * std::f32::consts::SQRT_2);
        f[0] = l * behind;
        f[1] = r * behind;
    }
}

/// Soft knee above 0.9 so boosts and reverb build-ups never hard-clip.
fn limit(buf: &mut [f32]) {
    for x in buf.iter_mut() {
        let a = x.abs();
        if a > 0.9 {
            *x = x.signum() * (0.9 + 0.1 * ((a - 0.9) / 0.1).tanh());
        }
    }
}

/// Render the full reworked sound of `src`.
pub fn process(src: &AudioBuf, fx: &AudioFx) -> Processed {
    let speed = fx.speed.clamp(0.25, 4.0);
    let mut s = if (speed - 1.0).abs() < 1e-6 {
        src.samples.clone()
    } else if fx.keep_pitch {
        stretch(&src.samples, speed)
    } else {
        varispeed(&src.samples, speed)
    };
    if fx.reverb > 0.0 {
        // Room for the tail to ring out past the end of the source.
        let tail = (1.0 + 5.0 * fx.room) * RATE as f64;
        s.resize(s.len() + tail as usize * 2, 0.0);
    }
    if fx.bass > 0.0 {
        Biquad::low_shelf(90.0, 12.0 * fx.bass).run(&mut s);
    }
    if fx.eq_low != 0.0 {
        Biquad::low_shelf(110.0, fx.eq_low.clamp(-12.0, 12.0)).run(&mut s);
    }
    if fx.eq_mid != 0.0 {
        Biquad::peaking(1000.0, 0.8, fx.eq_mid.clamp(-12.0, 12.0)).run(&mut s);
    }
    if fx.eq_high != 0.0 {
        Biquad::high_shelf(7000.0, fx.eq_high.clamp(-12.0, 12.0)).run(&mut s);
    }
    if fx.muffle > 0.0 {
        // 20 kHz at 0 down to about 250 Hz at 1, 24 dB/octave.
        let f = 20000.0 * 0.0125f64.powf(fx.muffle.clamp(0.0, 1.0));
        Biquad::lowpass(f, 0.54).run(&mut s);
        Biquad::lowpass(f, 1.31).run(&mut s);
    }
    if fx.reverb > 0.0 {
        reverb(&mut s, fx.reverb.clamp(0.0, 1.0) as f32, fx.room.clamp(0.0, 1.0) as f32);
    }
    if fx.pan8d > 0.0 {
        pan8d(&mut s, fx.pan8d.clamp(0.0, 1.0) as f32);
    }
    limit(&mut s);
    Processed { buf: AudioBuf { samples: s }, speed }
}

// ---------- Cache ----------

fn key(asset: &str, fx: &AudioFx) -> String {
    format!("{asset}|{:.3}|{}|{:.3}|{:.3}|{:.3}|{:.3}|{:.3}|{:.2}|{:.2}|{:.2}", fx.speed, fx.keep_pitch, fx.reverb, fx.room, fx.bass, fx.muffle, fx.pan8d, fx.eq_low, fx.eq_mid, fx.eq_high)
}

#[derive(Default)]
struct Cache {
    ready: HashMap<String, Arc<Processed>>,
    order: VecDeque<String>,
    /// The newest ready render per asset, played while a newer setting renders.
    latest: HashMap<String, Arc<Processed>>,
    pending: HashSet<String>,
}
fn cache() -> &'static Mutex<Cache> {
    static C: OnceLock<Mutex<Cache>> = OnceLock::new();
    C.get_or_init(Default::default)
}
fn store(k: String, asset: &str, p: Arc<Processed>) {
    let mut c = cache().lock();
    c.pending.remove(&k);
    c.latest.insert(asset.to_string(), p.clone());
    if c.ready.insert(k.clone(), p).is_none() {
        c.order.push_back(k);
    }
    // Whole songs are big: keep the last handful.
    while c.order.len() > 8 {
        if let Some(old) = c.order.pop_front() {
            c.ready.remove(&old);
        }
    }
}

type Job = (String, String, AudioFx);
fn worker() -> &'static std::sync::mpsc::Sender<Job> {
    static W: OnceLock<std::sync::mpsc::Sender<Job>> = OnceLock::new();
    W.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("audio-fx".into())
            .spawn(move || {
                while let Ok(mut job) = rx.recv() {
                    // While a slider is dragged, only the newest setting matters.
                    while let Ok(j) = rx.try_recv() {
                        cache().lock().pending.remove(&job.0);
                        job = j;
                    }
                    let (k, asset, fx) = job;
                    if let Some(src) = buffer(&asset) {
                        store(k, &asset, Arc::new(process(&src, &fx)));
                    } else {
                        cache().lock().pending.remove(&k);
                    }
                }
            })
            .expect("audio-fx thread");
        tx
    })
}

/// The reworked sound for a clip, without waiting: the exact render if it's ready, otherwise the
/// newest render of the same file (queued to catch up), otherwise None.
pub fn processed(asset: &str, fx: &AudioFx) -> Option<Arc<Processed>> {
    let k = key(asset, fx);
    let mut c = cache().lock();
    if let Some(p) = c.ready.get(&k) {
        return Some(p.clone());
    }
    if buffer(asset).is_some() && c.pending.insert(k.clone()) {
        let _ = worker().send((k, asset.to_string(), fx.clone()));
    }
    c.latest.get(asset).cloned()
}

/// The exact render, made here if needed (for export).
pub fn processed_now(asset: &str, fx: &AudioFx) -> Option<Arc<Processed>> {
    let k = key(asset, fx);
    if let Some(p) = cache().lock().ready.get(&k) {
        return Some(p.clone());
    }
    let p = Arc::new(process(&*buffer(asset)?, fx));
    store(k, asset, p.clone());
    Some(p)
}

/// Whether a clip's sound runs through these effects.
pub fn active(fx: Option<&AudioFx>) -> Option<&AudioFx> {
    fx.filter(|f| !f.is_identity())
}
