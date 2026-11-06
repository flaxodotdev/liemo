"use client";

// Canvas `filter` is ignored by WebKit (Safari and WebKit-based browsers): the property exists
// but nothing changes. This module detects that and applies the same CSS filter strings with
// pixel math instead: colour filters become one 3×4 colour matrix, blur becomes a downscale pass.

type Matrix = number[]; // 12 numbers: rows R, G, B; columns r, g, b, offset (0–255)

let native: boolean | null = null;
/** True when ctx.filter actually changes pixels in this browser. */
export function nativeCanvasFilter() {
  if (native !== null) return native;
  try {
    const c = document.createElement("canvas"); c.width = c.height = 2;
    const g = c.getContext("2d", { willReadFrequently: true })!;
    g.filter = "grayscale(1)"; g.fillStyle = "#ff0000"; g.fillRect(0, 0, 2, 2);
    const [r, gr] = g.getImageData(0, 0, 1, 1).data;
    native = Math.abs(r - gr) < 40;
  } catch { native = false; }
  return native;
}

const IDENTITY: Matrix = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0];

/** a ∘ b: apply b first, then a. */
function mul(a: Matrix, b: Matrix): Matrix {
  const out = new Array(12).fill(0);
  for (let r = 0; r < 3; r++) {
    for (let c = 0; c < 3; c++) out[r * 4 + c] = a[r * 4] * b[c] + a[r * 4 + 1] * b[4 + c] + a[r * 4 + 2] * b[8 + c];
    out[r * 4 + 3] = a[r * 4] * b[3] + a[r * 4 + 1] * b[7] + a[r * 4 + 2] * b[11] + a[r * 4 + 3];
  }
  return out;
}
const m3 = (v: number[], off = [0, 0, 0]): Matrix => [v[0], v[1], v[2], off[0], v[3], v[4], v[5], off[1], v[6], v[7], v[8], off[2]];

// Matrices from the Filter Effects spec.
function opMatrix(name: string, v: number): Matrix | null {
  switch (name) {
    case "grayscale": { const a = 1 - Math.min(1, v); return m3([
      0.2126 + 0.7874 * a, 0.7152 - 0.7152 * a, 0.0722 - 0.0722 * a,
      0.2126 - 0.2126 * a, 0.7152 + 0.2848 * a, 0.0722 - 0.0722 * a,
      0.2126 - 0.2126 * a, 0.7152 - 0.7152 * a, 0.0722 + 0.9278 * a]); }
    case "sepia": { const a = 1 - Math.min(1, v); return m3([
      0.393 + 0.607 * a, 0.769 - 0.769 * a, 0.189 - 0.189 * a,
      0.349 - 0.349 * a, 0.686 + 0.314 * a, 0.168 - 0.168 * a,
      0.272 - 0.272 * a, 0.534 - 0.534 * a, 0.131 + 0.869 * a]); }
    case "saturate": return m3([
      0.213 + 0.787 * v, 0.715 - 0.715 * v, 0.072 - 0.072 * v,
      0.213 - 0.213 * v, 0.715 + 0.285 * v, 0.072 - 0.072 * v,
      0.213 - 0.213 * v, 0.715 - 0.715 * v, 0.072 + 0.928 * v]);
    case "hue-rotate": { const t = (v * Math.PI) / 180, c = Math.cos(t), s = Math.sin(t); return m3([
      0.213 + c * 0.787 - s * 0.213, 0.715 - c * 0.715 - s * 0.715, 0.072 - c * 0.072 + s * 0.928,
      0.213 - c * 0.213 + s * 0.143, 0.715 + c * 0.285 + s * 0.14, 0.072 - c * 0.072 - s * 0.283,
      0.213 - c * 0.213 - s * 0.787, 0.715 - c * 0.715 + s * 0.715, 0.072 + c * 0.928 + s * 0.072]); }
    case "brightness": return m3([v, 0, 0, 0, v, 0, 0, 0, v]);
    case "contrast": { const o = 255 * (0.5 - 0.5 * v); return m3([v, 0, 0, 0, v, 0, 0, 0, v], [o, o, o]); }
    case "invert": { const a = Math.min(1, v), d = 1 - 2 * a, o = 255 * a; return m3([d, 0, 0, 0, d, 0, 0, 0, d], [o, o, o]); }
    default: return null;
  }
}

const parsed = new Map<string, { matrix: Matrix | null; blur: number }>();
/** Parse a CSS filter string such as "sepia(0.35) saturate(1.4) hue-rotate(-12deg) blur(2px)". */
function parse(filter: string) {
  let hit = parsed.get(filter);
  if (hit) return hit;
  let matrix: Matrix = IDENTITY, blur = 0, any = false;
  for (const [, name, arg] of filter.matchAll(/([a-z-]+)\(([^)]*)\)/g)) {
    const v = parseFloat(arg);
    if (name === "blur") { blur += v; continue; }
    const m = opMatrix(name, v);
    if (m) { matrix = mul(m, matrix); any = true; }
  }
  hit = { matrix: any ? matrix : null, blur };
  if (parsed.size > 500) parsed.clear();
  parsed.set(filter, hit);
  return hit;
}

let blurCanvas: HTMLCanvasElement | null = null;

/** Apply a CSS filter string to everything already drawn on `canvas`. */
export function applyFilterToCanvas(canvas: HTMLCanvasElement, filter: string) {
  if (!filter || filter === "none") return;
  const { matrix, blur } = parse(filter);
  const g = canvas.getContext("2d", { willReadFrequently: true })!;
  const w = canvas.width, h = canvas.height;

  if (blur > 0.3) {
    // Box-ish blur: shrink, then scale back up with smoothing.
    const k = Math.max(1.5, blur * 0.8);
    blurCanvas ??= document.createElement("canvas");
    blurCanvas.width = Math.max(1, Math.round(w / k)); blurCanvas.height = Math.max(1, Math.round(h / k));
    const b = blurCanvas.getContext("2d")!;
    b.imageSmoothingQuality = "high";
    b.clearRect(0, 0, blurCanvas.width, blurCanvas.height);
    b.drawImage(canvas, 0, 0, blurCanvas.width, blurCanvas.height);
    g.save();
    g.globalCompositeOperation = "copy";
    g.imageSmoothingQuality = "high";
    g.drawImage(blurCanvas, 0, 0, w, h);
    g.restore();
  }

  if (matrix) {
    const img = g.getImageData(0, 0, w, h), d = img.data, m = matrix;
    for (let i = 0; i < d.length; i += 4) {
      if (d[i + 3] === 0) continue;
      const r = d[i], gg = d[i + 1], bb = d[i + 2];
      d[i] = m[0] * r + m[1] * gg + m[2] * bb + m[3];
      d[i + 1] = m[4] * r + m[5] * gg + m[6] * bb + m[7];
      d[i + 2] = m[8] * r + m[9] * gg + m[10] * bb + m[11];
    }
    g.putImageData(img, 0, 0);
  }
}
