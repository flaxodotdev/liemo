use std::cell::Cell;

/// The open project's settings that layout and timing depend on. Each thread that works on a
/// project (UI, preview renderer, export, saving) sets them for itself.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Env {
    /// Design canvas in units (short side 720): 1280×720 landscape, 720×1280 for a Reel…
    pub w: f32,
    pub h: f32,
    pub fps: f64,
    /// Behind everything: what empty parts of the frame show.
    pub bg: [u8; 3],
}
impl Default for Env {
    fn default() -> Self {
        Env { w: 1280.0, h: 720.0, fps: 30.0, bg: [0, 0, 0] }
    }
}

thread_local! {
    static ENV: Cell<Env> = Cell::new(Env::default());
}
pub fn env() -> Env {
    ENV.with(|c| c.get())
}
pub fn set_env(e: Env) {
    ENV.with(|c| c.set(e));
}
/// Canvas width in design units.
#[allow(non_snake_case)]
pub fn W() -> f32 {
    env().w
}
/// Canvas height in design units.
#[allow(non_snake_case)]
pub fn H() -> f32 {
    env().h
}
/// The project's frame rate.
#[allow(non_snake_case)]
pub fn FPS() -> f64 {
    env().fps
}
/// Run `f` with another canvas size on this thread, then put the old settings back.
pub fn with_canvas<R>(size: (f32, f32), f: impl FnOnce() -> R) -> R {
    let old = env();
    set_env(Env { w: size.0, h: size.1, ..old });
    let r = f();
    set_env(old);
    r
}

/// The LCG the browser version uses, so seeded patterns match it exactly.
pub struct Rng(u32);
impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(seed)
    }
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.0 as f64 / 4294967296.0) as f32
    }
}
pub fn rng(seed: i64) -> Rng {
    Rng::new(seed as u32)
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
pub fn snap_frame(t: f64) -> f64 {
    (t * FPS()).round() / FPS()
}

/// SMPTE-style timecode, HH:MM:SS:FF at the project's frame rate.
pub fn tc(sec: f64) -> String {
    let fps = FPS().round().max(1.0) as i64;
    let f = (sec.max(0.0) * fps as f64 + 1e-6).floor() as i64;
    format!("{:02}:{:02}:{:02}:{:02}", f / fps / 3600, (f / fps / 60) % 60, (f / fps) % 60, f % fps)
}
pub fn short_time(sec: f64) -> String {
    format!("{}:{:02}", (sec / 60.0).floor() as i64, (sec % 60.0).round() as i64)
}
