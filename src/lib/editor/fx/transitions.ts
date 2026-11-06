"use client";

// Transitions between two clips on the same track. `a` is the outgoing clip (or null when the
// clip transitions in from what's below), `b` the incoming one; both are already-rendered
// frames the size of the target. `p` runs 0 → 1 across the transition.
import { rng } from "../time";

type C2D = CanvasRenderingContext2D;
type Frame = HTMLCanvasElement;
export interface TransitionDef {
  id: string;
  name: string;
  group: "dissolve" | "motion" | "shape" | "stylized";
  draw: (c: C2D, a: Frame | null, b: Frame, p: number, t: number) => void;
}

const ease = (p: number) => (p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2);
const size = (c: C2D) => ({ w: c.canvas.width, h: c.canvas.height });

function withAlpha(c: C2D, alpha: number, fn: () => void) {
  if (alpha <= 0) return;
  c.save(); c.globalAlpha *= Math.min(1, alpha); fn(); c.restore();
}
function clipTo(c: C2D, path: (c: C2D) => void, fn: () => void) {
  c.save(); c.beginPath(); path(c); c.clip(); fn(); c.restore();
}
const draw = (c: C2D, f: Frame | null, x = 0, y = 0, w?: number, h?: number) => {
  if (!f) return;
  if (w === undefined) c.drawImage(f, x, y); else c.drawImage(f, x, y, w, h!);
};

// Scratch canvases for transitions that need intermediate passes.
const pool = new Map<string, HTMLCanvasElement>();
function scratch(name: string, w: number, h: number) {
  const key = `${name}:${w}x${h}`;
  let s = pool.get(key);
  if (!s) { s = document.createElement("canvas"); s.width = w; s.height = h; if (pool.size > 24) pool.clear(); pool.set(key, s); }
  const g = s.getContext("2d")!; g.setTransform(1, 0, 0, 1, 0, 0); g.globalAlpha = 1; g.globalCompositeOperation = "source-over"; g.clearRect(0, 0, w, h);
  return s;
}
function blurred(f: Frame, amount: number, name: string) {
  const w = f.width, h = f.height, k = 1 + amount * 18;
  const small = scratch(name + "-s", Math.max(1, Math.round(w / k)), Math.max(1, Math.round(h / k)));
  const sg = small.getContext("2d")!; sg.imageSmoothingQuality = "high"; sg.drawImage(f, 0, 0, small.width, small.height);
  const out = scratch(name, w, h), og = out.getContext("2d")!; og.imageSmoothingQuality = "high"; og.drawImage(small, 0, 0, w, h);
  return out;
}
function pixelated(f: Frame, cells: number, name: string) {
  const w = f.width, h = f.height, cols = Math.max(1, Math.round(cells)), rows = Math.max(1, Math.round((cells * h) / w));
  const small = scratch(name + "-s", cols, rows); small.getContext("2d")!.drawImage(f, 0, 0, cols, rows);
  const out = scratch(name, w, h), og = out.getContext("2d")!; og.imageSmoothingEnabled = false; og.drawImage(small, 0, 0, w, h);
  return out;
}

const push = (dx: number, dy: number): TransitionDef["draw"] => (c, a, b, p) => {
  const { w, h } = size(c), e = ease(p);
  draw(c, a, -dx * w * e, -dy * h * e);
  draw(c, b, dx * w * (1 - e), dy * h * (1 - e));
};

export const TRANSITIONS: TransitionDef[] = [
  // Dissolves
  { id: "dissolve", name: "Dissolve", group: "dissolve", draw: (c, a, b, p) => { draw(c, a); withAlpha(c, p, () => draw(c, b)); } },
  { id: "dip-black", name: "Dip to black", group: "dissolve", draw: (c, a, b, p) => {
    const { w, h } = size(c);
    if (p < 0.5) withAlpha(c, 1 - p * 2, () => draw(c, a)); else withAlpha(c, p * 2 - 1, () => draw(c, b));
    c.save(); c.fillStyle = "#000"; c.globalAlpha = 1 - Math.abs(p * 2 - 1); c.fillRect(0, 0, w, h); c.restore();
  } },
  { id: "dip-white", name: "Dip to white", group: "dissolve", draw: (c, a, b, p) => {
    const { w, h } = size(c);
    if (p < 0.5) draw(c, a); else draw(c, b);
    c.save(); c.fillStyle = "#fff"; c.globalAlpha = 1 - Math.abs(p * 2 - 1); c.fillRect(0, 0, w, h); c.restore();
  } },
  { id: "blur", name: "Blur", group: "dissolve", draw: (c, a, b, p) => {
    if (a) withAlpha(c, 1 - p, () => draw(c, blurred(a, p, "tr-blur-a")));
    withAlpha(c, p, () => draw(c, blurred(b, 1 - p, "tr-blur-b")));
  } },

  // Motion
  { id: "push-left", name: "Push left", group: "motion", draw: push(1, 0) },
  { id: "push-right", name: "Push right", group: "motion", draw: push(-1, 0) },
  { id: "push-up", name: "Push up", group: "motion", draw: push(0, 1) },
  { id: "push-down", name: "Push down", group: "motion", draw: push(0, -1) },
  { id: "whip", name: "Whip pan", group: "motion", draw: (c, a, b, p) => {
    // A fast push with smeared copies for motion blur.
    const { w } = size(c), e = ease(p), smear = Math.sin(p * Math.PI) * w * 0.06;
    for (let i = 0; i < 5; i++) {
      const o = (i - 2) * smear * 0.5;
      withAlpha(c, i === 2 ? 1 : 0.25, () => { draw(c, a, -w * e + o, 0); draw(c, b, w * (1 - e) + o, 0); });
    }
  } },
  { id: "zoom", name: "Zoom", group: "motion", draw: (c, a, b, p) => {
    const { w, h } = size(c), e = ease(p);
    if (a) withAlpha(c, 1 - e, () => { const k = 1 + e * 0.6; draw(c, a, (w - w * k) / 2, (h - h * k) / 2, w * k, h * k); });
    withAlpha(c, e, () => { const k = 0.7 + e * 0.3; draw(c, b, (w - w * k) / 2, (h - h * k) / 2, w * k, h * k); });
  } },
  { id: "spin", name: "Spin zoom", group: "motion", draw: (c, a, b, p) => {
    const { w, h } = size(c), e = ease(p);
    const at = (f: Frame | null, rot: number, k: number, alpha: number) => withAlpha(c, alpha, () => {
      c.translate(w / 2, h / 2); c.rotate(rot); c.scale(k, k); c.translate(-w / 2, -h / 2); draw(c, f);
    });
    at(a, e * 0.6, 1 + e, 1 - e);
    at(b, (e - 1) * 0.6, 2 - e, e);
  } },

  // Shapes
  { id: "wipe", name: "Wipe", group: "shape", draw: (c, a, b, p) => {
    const { w, h } = size(c), x = ease(p) * w;
    draw(c, a); clipTo(c, (g) => g.rect(0, 0, x, h), () => draw(c, b));
  } },
  { id: "soft-wipe", name: "Soft wipe", group: "shape", draw: (c, a, b, p) => {
    const { w, h } = size(c), edge = w * 0.25, x = -edge + ease(p) * (w + edge * 2);
    const m = scratch("tr-soft", w, h), mg = m.getContext("2d")!;
    mg.drawImage(b, 0, 0);
    const g = mg.createLinearGradient(x - edge, 0, x + edge, 0); g.addColorStop(0, "#000"); g.addColorStop(1, "rgba(0,0,0,0)");
    mg.globalCompositeOperation = "destination-in"; mg.fillStyle = g; mg.fillRect(0, 0, w, h);
    draw(c, a); c.drawImage(m, 0, 0);
  } },
  { id: "barn", name: "Barn doors", group: "shape", draw: (c, a, b, p) => {
    const { w, h } = size(c), half = (ease(p) * w) / 2;
    draw(c, a); clipTo(c, (g) => g.rect(w / 2 - half, 0, half * 2, h), () => draw(c, b));
  } },
  { id: "blinds", name: "Blinds", group: "shape", draw: (c, a, b, p) => {
    const { w, h } = size(c), n = 9, band = h / n, open = ease(p) * band;
    draw(c, a); clipTo(c, (g) => { for (let i = 0; i < n; i++) g.rect(0, i * band, w, open); }, () => draw(c, b));
  } },
  { id: "clock", name: "Clock wipe", group: "shape", draw: (c, a, b, p) => {
    const { w, h } = size(c), r = Math.hypot(w, h);
    draw(c, a);
    clipTo(c, (g) => { g.moveTo(w / 2, h / 2); g.arc(w / 2, h / 2, r, -Math.PI / 2, -Math.PI / 2 + ease(p) * Math.PI * 2); g.closePath(); }, () => draw(c, b));
  } },
  { id: "iris", name: "Iris", group: "shape", draw: (c, a, b, p) => {
    const { w, h } = size(c), r = (ease(p) * Math.hypot(w, h)) / 2;
    draw(c, a); clipTo(c, (g) => g.arc(w / 2, h / 2, Math.max(0.1, r), 0, Math.PI * 2), () => draw(c, b));
  } },

  // Stylized
  { id: "pixelate", name: "Pixelate", group: "stylized", draw: (c, a, b, p) => {
    const { w } = size(c), k = 1 - Math.abs(p * 2 - 1), cells = Math.max(6, w / (1 + k * 60));
    const f = p < 0.5 ? a : b;
    if (f) c.drawImage(k > 0.02 ? pixelated(f, cells, "tr-pix") : f, 0, 0);
  } },
  { id: "glitch", name: "Glitch", group: "stylized", draw: (c, a, b, p, t) => {
    const { w, h } = size(c), r = rng(Math.floor(t * 30) + 3), k = 1 - Math.abs(p * 2 - 1);
    const base = p < 0.5 ? a : b, other = p < 0.5 ? b : a;
    draw(c, base);
    for (let i = 0; i < 18; i++) {
      if (r() > 0.25 + k * 0.7) continue;
      const y = r() * h, sh = Math.max(2, r() * h * 0.09), dx = (r() - 0.5) * w * 0.25 * k, src = r() < k * 0.6 ? other : base;
      if (src) c.drawImage(src, 0, y, w, sh, dx, y, w, sh);
    }
    if (k > 0.2) { c.save(); c.globalCompositeOperation = "lighter"; c.globalAlpha = k * 0.35; draw(c, base, 8 * k, 0); c.restore(); }
  } },
];

export const transitionDef = (id: string) => TRANSITIONS.find((t) => t.id === id);
export const TRANSITION_GROUPS: [TransitionDef["group"], string][] = [["dissolve", "Dissolves"], ["motion", "Motion"], ["shape", "Wipes & shapes"], ["stylized", "Stylized"]];
