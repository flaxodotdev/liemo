export const FPS = 30;
export const W = 1280;
export const H = 720;

export function rng(seed: number) {
  let s = seed >>> 0;
  return () => (s = (s * 1664525 + 1013904223) >>> 0) / 4294967296;
}
export const lerp = (a: number, b: number, t: number) => a + (b - a) * t;
export const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));
export const snapFrame = (t: number) => Math.round(t * FPS) / FPS;

/** SMPTE-style timecode, HH:MM:SS:FF at 30 fps. */
export function tc(sec: number) {
  const f = Math.floor(Math.max(0, sec) * FPS + 1e-6);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(Math.floor(f / FPS / 3600))}:${p(Math.floor(f / FPS / 60) % 60)}:${p(Math.floor(f / FPS) % 60)}:${p(f % FPS)}`;
}
export function shortTime(sec: number) {
  return `${Math.floor(sec / 60)}:${String(Math.round(sec % 60)).padStart(2, "0")}`;
}
