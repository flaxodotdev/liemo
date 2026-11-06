"use client";

// Effect engines. Each one reads `src` and paints the result into `dst` (same size, already cleared).
// Sizes are expressed against a 1280-wide frame and scaled, so thumbnails match the full render.
import { applyFilterToCanvas, nativeCanvasFilter } from "../effects";
import { rng } from "../time";
import { DOT_PALETTES, GLYPH_SETS, GRADIENT_MAPS, PALETTES, effectDef, type ClipEffect, type Params } from "./registry";

const ctx2d = (c: HTMLCanvasElement) => c.getContext("2d", { willReadFrequently: true })!;
const num = (p: Params, k: string, d: number) => (typeof p[k] === "number" ? (p[k] as number) : d);
const str = (p: Params, k: string, d: string) => (typeof p[k] === "string" ? (p[k] as string) : d);
const clamp01 = (v: number) => (v < 0 ? 0 : v > 1 ? 1 : v);

// Scratch canvases, reused by name and size so a frame allocates nothing.
const scratchPool = new Map<string, HTMLCanvasElement>();
function scratch(name: string, w: number, h: number) {
  const key = `${name}:${w}x${h}`;
  let c = scratchPool.get(key);
  if (!c) {
    c = document.createElement("canvas"); c.width = w; c.height = h;
    if (scratchPool.size > 40) scratchPool.clear();
    scratchPool.set(key, c);
  }
  return c;
}
function hexRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", ""), v = parseInt(h.length === 3 ? h.replace(/./g, "$&$&") : h, 16);
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255];
}

// ---------- ASCII / glyph mosaic ----------
const atlases = new Map<string, { canvas: HTMLCanvasElement; levels: number; cell: number }>();
function atlasFor(setId: string, cell: number) {
  const key = `${setId}:${cell}`;
  const hit = atlases.get(key); if (hit) return hit;
  const set = GLYPH_SETS[setId] ?? GLYPH_SETS.classic;
  const glyphs = set.kind === "chars" ? [...set.chars] : [];
  const levels = set.kind === "chars" ? glyphs.length : set.levels;
  const canvas = document.createElement("canvas"); canvas.width = levels * cell; canvas.height = cell;
  const g = canvas.getContext("2d")!;
  g.fillStyle = "#fff"; g.strokeStyle = "#fff"; g.textAlign = "center"; g.textBaseline = "middle";
  const mono = getComputedStyle(document.documentElement).getPropertyValue("--font-jetbrains").trim() || "monospace";
  for (let i = 0; i < levels; i++) {
    const cx = i * cell + cell / 2, cy = cell / 2, s = levels > 1 ? i / (levels - 1) : 1;
    if (set.kind === "chars") {
      g.font = set.emoji ? `${Math.round(cell * 0.86)}px system-ui, "Apple Color Emoji", "Segoe UI Emoji", sans-serif` : `600 ${Math.round(cell * 1.08)}px ${mono}`;
      g.fillText(glyphs[i], cx, cy + (set.emoji ? cell * 0.04 : cell * 0.06));
      continue;
    }
    if (s === 0) continue;
    g.save(); g.translate(cx, cy);
    switch (set.shape) {
      case "circle": g.beginPath(); g.arc(0, 0, (cell / 2) * 1.12 * Math.sqrt(s), 0, Math.PI * 2); g.fill(); break;
      case "square": { const d = cell * Math.sqrt(s); g.fillRect(-d / 2, -d / 2, d, d); break; }
      case "diamond": { const d = (cell / 2) * 1.35 * Math.sqrt(s); g.beginPath(); g.moveTo(0, -d); g.lineTo(d, 0); g.lineTo(0, d); g.lineTo(-d, 0); g.fill(); break; }
      case "cross": { const t = Math.max(1, cell * 0.42 * s), l = cell * 0.9; g.fillRect(-t / 2, -l / 2, t, l); g.fillRect(-l / 2, -t / 2, l, t); break; }
      case "hatch": {
        g.lineWidth = Math.max(0.6, cell * 0.36 * s); g.lineCap = "square"; g.beginPath();
        g.moveTo(-cell / 2, cell / 2); g.lineTo(cell / 2, -cell / 2);
        if (s > 0.55) { g.moveTo(-cell / 2, -cell / 2); g.lineTo(cell / 2, cell / 2); }
        g.stroke(); break;
      }
      case "ring": g.lineWidth = Math.max(0.8, cell * 0.22 * s + 0.5); g.beginPath(); g.arc(0, 0, cell * 0.32, 0, Math.PI * 2); g.stroke(); if (s > 0.8) { g.beginPath(); g.arc(0, 0, cell * 0.12, 0, Math.PI * 2); g.fill(); } break;
    }
    g.restore();
  }
  const out = { canvas, levels, cell };
  if (atlases.size > 60) atlases.clear();
  atlases.set(key, out);
  return out;
}

function ascii(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params, t: number, object = false) {
  const w = dst.width, h = dst.height, k = w / 1280;
  const cellPx = Math.max(2, num(p, "cell", 12) * k);
  const cols = Math.ceil(w / cellPx), rows = Math.ceil(h / cellPx);
  const setId = str(p, "set", "classic"), set = GLYPH_SETS[setId] ?? GLYPH_SETS.classic;
  const contrast = num(p, "contrast", 1.3), invert = p.invert === true;
  // On a single object (a title, a placed photo) there's no background box, just the glyphs.
  const colorMode = str(p, "color", "source"), bg = object ? "none" : str(p, "bg", "black");

  // 1. Sample: one pixel per cell.
  const small = scratch("ascii-small", cols, rows), sg = ctx2d(small);
  sg.clearRect(0, 0, cols, rows); sg.imageSmoothingQuality = "high";
  sg.drawImage(src, 0, 0, cols, rows);
  const px = sg.getImageData(0, 0, cols, rows).data;

  // 2. Stamp one glyph per cell from the atlas (white on transparent).
  const atlas = atlasFor(setId, Math.max(4, Math.round(cellPx))), g = dst.getContext("2d")!;
  const rain = set.kind === "chars" && !!set.rain;
  for (let y = 0; y < rows; y++) {
    for (let x = 0; x < cols; x++) {
      const i = (y * cols + x) * 4, a = px[i + 3] / 255;
      if (a === 0) continue;
      let lum = ((0.2126 * px[i] + 0.7152 * px[i + 1] + 0.0722 * px[i + 2]) / 255) * a;
      lum = clamp01((lum - 0.5) * contrast + 0.5);
      if (invert) lum = 1 - lum;
      let level: number;
      if (rain) {
        // Glyphs change as columns "fall"; brightness comes from alpha.
        const drift = Math.floor(t * 9 + x * 7.31);
        level = Math.abs((x * 131 + y * 17 + drift * 29 + ((x * 7) ^ (y + drift))) % atlas.levels);
        const head = ((t * 12 + x * 3.7) % (rows + 8)) - y;
        g.globalAlpha = clamp01(lum * 1.15 + (head > 0 && head < 1.5 ? 0.6 : 0));
        if (g.globalAlpha < 0.04) continue;
      } else {
        level = Math.round(lum * (atlas.levels - 1));
        if (level <= 0 && set.kind === "shape") continue;
      }
      g.drawImage(atlas.canvas, level * atlas.cell, 0, atlas.cell, atlas.cell, x * cellPx, y * cellPx, cellPx, cellPx);
    }
  }
  g.globalAlpha = 1;

  // 3. Colour the glyphs: keep emoji colours, otherwise recolour with source-in.
  const emoji = set.kind === "chars" && set.emoji;
  if (!emoji && colorMode !== "mono") {
    g.globalCompositeOperation = "source-in";
    if (colorMode === "tint") { g.fillStyle = str(p, "tint", "#22c55e"); g.fillRect(0, 0, w, h); }
    else { g.imageSmoothingEnabled = false; g.drawImage(small, 0, 0, cols * cellPx, rows * cellPx); g.imageSmoothingEnabled = true; }
  }

  // 4. Background goes underneath.
  g.globalCompositeOperation = "destination-over";
  if (bg === "dim") { g.globalAlpha = 0.22; g.drawImage(src, 0, 0); g.globalAlpha = 1; }
  if (bg !== "none") { g.fillStyle = "#000"; g.fillRect(0, 0, w, h); }
  g.globalCompositeOperation = "source-over";
}

// ---------- Pixelate ----------
function pixelate(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params) {
  const w = dst.width, h = dst.height, size = Math.max(1, num(p, "size", 16) * (w / 1280));
  const cols = Math.max(1, Math.round(w / size)), rows = Math.max(1, Math.round(h / size));
  const small = scratch("pix-small", cols, rows), sg = small.getContext("2d")!;
  sg.clearRect(0, 0, cols, rows); sg.drawImage(src, 0, 0, cols, rows);
  const g = dst.getContext("2d")!;
  g.imageSmoothingEnabled = false; g.drawImage(small, 0, 0, w, h); g.imageSmoothingEnabled = true;
}

// ---------- Ordered dither to a palette ----------
const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
function dither(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params) {
  const w = dst.width, h = dst.height, size = Math.max(1, num(p, "pixel", 3) * (w / 1280));
  const cols = Math.max(1, Math.round(w / size)), rows = Math.max(1, Math.round(h / size));
  const pal = (PALETTES[str(p, "palette", "1bit")] ?? PALETTES["1bit"]).colors.map(hexRgb);
  const spread = num(p, "spread", 1) * 64;
  const small = scratch("dither-small", cols, rows), sg = ctx2d(small);
  sg.clearRect(0, 0, cols, rows); sg.drawImage(src, 0, 0, cols, rows);
  const img = sg.getImageData(0, 0, cols, rows), d = img.data;
  for (let y = 0; y < rows; y++) for (let x = 0; x < cols; x++) {
    const i = (y * cols + x) * 4, o = (BAYER[(y & 3) * 4 + (x & 3)] / 16 - 0.5) * spread;
    const r = d[i] + o, gg = d[i + 1] + o, b = d[i + 2] + o;
    let best = 0, bd = Infinity;
    for (let k = 0; k < pal.length; k++) {
      const dr = r - pal[k][0], dg = gg - pal[k][1], db = b - pal[k][2], dist = dr * dr * 0.3 + dg * dg * 0.59 + db * db * 0.11;
      if (dist < bd) { bd = dist; best = k; }
    }
    d[i] = pal[best][0]; d[i + 1] = pal[best][1]; d[i + 2] = pal[best][2];
  }
  sg.putImageData(img, 0, 0);
  const g = dst.getContext("2d")!;
  g.imageSmoothingEnabled = false; g.drawImage(small, 0, 0, w, h); g.imageSmoothingEnabled = true;
}

// ---------- RGB split ----------
function rgbsplit(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params, t: number) {
  const w = dst.width, h = dst.height, k = w / 1280;
  let off = num(p, "offset", 10) * k;
  if (p.jitter === true) off *= 0.6 + 0.8 * Math.abs(Math.sin(t * 23.7) * Math.cos(t * 7.1));
  const tmp = scratch("split", w, h), tg = tmp.getContext("2d")!, g = dst.getContext("2d")!;
  g.globalCompositeOperation = "lighter";
  ([["#ff0000", -off], ["#00ff00", 0], ["#0000ff", off]] as const).forEach(([col, dx]) => {
    tg.globalCompositeOperation = "copy"; tg.drawImage(src, 0, 0);
    tg.globalCompositeOperation = "multiply"; tg.fillStyle = col; tg.fillRect(0, 0, w, h);
    tg.globalCompositeOperation = "destination-in"; tg.drawImage(src, 0, 0);
    g.drawImage(tmp, dx, 0);
  });
  g.globalCompositeOperation = "source-over";
}

// ---------- Glitch ----------
function glitch(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params, t: number) {
  const w = dst.width, h = dst.height, g = dst.getContext("2d")!;
  const intensity = num(p, "intensity", 0.5), slices = Math.round(num(p, "slices", 14));
  const r = rng(Math.floor(t * num(p, "speed", 12)) + 7);
  g.drawImage(src, 0, 0);
  for (let i = 0; i < slices; i++) {
    if (r() > 0.35 + intensity * 0.6) continue;
    const y = r() * h, sh = Math.max(1, r() * h * 0.08 * (0.3 + intensity)), dx = (r() - 0.5) * w * 0.18 * intensity;
    g.drawImage(src, 0, y, w, sh, dx, y, w, sh);
    if (r() < intensity * 0.5) { g.globalCompositeOperation = "lighter"; g.fillStyle = r() > 0.5 ? "rgba(255,0,80,.18)" : "rgba(0,200,255,.18)"; g.fillRect(0, y, w, sh); g.globalCompositeOperation = "source-over"; }
  }
}

// ---------- Posterize / duotone (per pixel) ----------
function posterize(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params) {
  const g = ctx2d(dst); g.drawImage(src, 0, 0);
  const n = Math.max(2, Math.round(num(p, "levels", 4))) - 1, step = 255 / n;
  const img = g.getImageData(0, 0, dst.width, dst.height), d = img.data;
  for (let i = 0; i < d.length; i += 4) {
    d[i] = Math.round(d[i] / step) * step; d[i + 1] = Math.round(d[i + 1] / step) * step; d[i + 2] = Math.round(d[i + 2] / step) * step;
  }
  g.putImageData(img, 0, 0);
}
function duotone(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params) {
  const g = ctx2d(dst); g.drawImage(src, 0, 0);
  const [r0, g0, b0] = hexRgb(str(p, "dark", "#1b0f2e")), [r1, g1, b1] = hexRgb(str(p, "light", "#ff7a3d")), c = num(p, "contrast", 1.1);
  const img = g.getImageData(0, 0, dst.width, dst.height), d = img.data;
  for (let i = 0; i < d.length; i += 4) {
    const l = clamp01((((0.2126 * d[i] + 0.7152 * d[i + 1] + 0.0722 * d[i + 2]) / 255) - 0.5) * c + 0.5);
    d[i] = r0 + (r1 - r0) * l; d[i + 1] = g0 + (g1 - g0) * l; d[i + 2] = b0 + (b1 - b0) * l;
  }
  g.putImageData(img, 0, 0);
}

// ---------- Shared helpers for the engines below ----------
/** Soft blur by shrinking and scaling back up. `radius` is in 1280-wide pixels. */
function blurOf(src: HTMLCanvasElement, radius: number, name: string) {
  const w = src.width, h = src.height, k = Math.max(1, radius * (w / 1280) * 0.7);
  const small = scratch(name + "-s", Math.max(1, Math.round(w / k)), Math.max(1, Math.round(h / k)));
  const sg = small.getContext("2d")!; sg.clearRect(0, 0, small.width, small.height); sg.imageSmoothingQuality = "high";
  sg.drawImage(src, 0, 0, small.width, small.height);
  const out = scratch(name, w, h), og = out.getContext("2d")!;
  og.clearRect(0, 0, w, h); og.imageSmoothingQuality = "high"; og.drawImage(small, 0, 0, w, h);
  return out;
}
const lumOf = (d: Uint8ClampedArray, i: number) => (0.2126 * d[i] + 0.7152 * d[i + 1] + 0.0722 * d[i + 2]) / 255;
/** Smooth pseudo-random wobble in -1..1 for shakes and jitters. */
const wobble = (t: number, seed: number) => (Math.sin(t * 7.1 + seed) * 0.5 + Math.sin(t * 13.7 + seed * 2.3) * 0.3 + Math.sin(t * 29.3 + seed * 4.1) * 0.2);

// ---------- Dots: rounded halftone and braille dot grid ----------
// Dots are stamped straight into pixel data: at braille density a frame has 100k+ of them,
// far too many for canvas arc() calls.
type Mask = { dx: Int16Array; dy: Int16Array; a: Float32Array };
const masks = new Map<number, Mask>();
function dotMask(radius: number): Mask {
  const key = Math.round(radius * 4) / 4;
  let m = masks.get(key);
  if (m) return m;
  const R = Math.ceil(key + 1), dx: number[] = [], dy: number[] = [], a: number[] = [];
  for (let y = -R; y <= R; y++) for (let x = -R; x <= R; x++) {
    const cov = Math.min(1, Math.max(0, key + 0.5 - Math.hypot(x, y))); // anti-aliased edge
    if (cov > 0) { dx.push(x); dy.push(y); a.push(cov); }
  }
  m = { dx: Int16Array.from(dx), dy: Int16Array.from(dy), a: Float32Array.from(a) };
  if (masks.size > 400) masks.clear();
  masks.set(key, m);
  return m;
}
const BG: Record<string, [number, number, number] | null> = { black: [0, 0, 0], midnight: [13, 15, 20], none: null };
const lutCache = new Map<string, Uint8ClampedArray>();
function paletteLut(key: string) {
  let lut = lutCache.get(key);
  if (lut) return lut;
  const stops = (DOT_PALETTES[key] ?? DOT_PALETTES.ice).stops.map(hexRgb);
  lut = new Uint8ClampedArray(256 * 3);
  for (let i = 0; i < 256; i++) {
    const x = (i / 255) * (stops.length - 1), j = Math.min(stops.length - 2, Math.floor(x)), f = x - j;
    for (let c = 0; c < 3; c++) lut[i * 3 + c] = stops[j][c] + (stops[j + 1][c] - stops[j][c]) * f;
  }
  lutCache.set(key, lut);
  return lut;
}
function dotsEngine(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params, object = false) {
  const w = dst.width, h = dst.height, k = w / 1280;
  const braille = str(p, "pattern", "halftone") === "braille";
  const cell = Math.max(2, num(p, "cell", 14) * k);
  const pitch = braille ? Math.max(2, cell / 2) : cell; // a braille character is two dots wide
  const cols = Math.max(1, Math.floor(w / pitch)), rows = Math.max(1, Math.floor(h / pitch));
  const ox = (w - cols * pitch) / 2, oy = (h - rows * pitch) / 2;
  const contrast = num(p, "contrast", 1.15), invert = p.invert === true, scale = num(p, "size", 0.9);
  const colorMode = str(p, "color", "white"), bg = object ? null : BG[str(p, "bg", "black")] ?? null;
  const lut = colorMode !== "white" && colorMode !== "source" ? paletteLut(colorMode) : null;

  const small = scratch("dots-small", cols, rows), sg = ctx2d(small);
  sg.clearRect(0, 0, cols, rows); sg.imageSmoothingQuality = "high"; sg.drawImage(src, 0, 0, cols, rows);
  const px = sg.getImageData(0, 0, cols, rows).data;

  const g = ctx2d(dst), img = g.createImageData(w, h), d = img.data;
  if (bg) for (let i = 0; i < d.length; i += 4) { d[i] = bg[0]; d[i + 1] = bg[1]; d[i + 2] = bg[2]; d[i + 3] = 255; }
  const fixed = braille ? dotMask((pitch / 2) * scale * 0.82) : null;

  for (let y = 0; y < rows; y++) for (let x = 0; x < cols; x++) {
    const i = (y * cols + x) * 4, alpha = px[i + 3] / 255;
    if (alpha === 0) continue;
    let lum = lumOf(px, i) * alpha;
    lum = clamp01((lum - 0.5) * contrast + 0.5);
    if (invert) lum = 1 - lum;
    let mask: Mask;
    if (braille) {
      if (lum <= (BAYER[(y & 3) * 4 + (x & 3)] + 0.5) / 16) continue; // dot off
      mask = fixed!;
    } else {
      const r = (pitch / 2) * scale * Math.sqrt(lum);
      if (r < 0.35) continue;
      mask = dotMask(r);
    }
    let cr: number, cg: number, cb: number;
    if (lut) { const li = Math.round(lum * 255) * 3; cr = lut[li]; cg = lut[li + 1]; cb = lut[li + 2]; }
    else if (colorMode === "source") { const boost = 1.15; cr = px[i] * boost; cg = px[i + 1] * boost; cb = px[i + 2] * boost; }
    else { cr = cg = cb = 255; }
    const cx = Math.round(ox + (x + 0.5) * pitch), cy = Math.round(oy + (y + 0.5) * pitch);
    for (let m = 0; m < mask.a.length; m++) {
      const X = cx + mask.dx[m], Y = cy + mask.dy[m];
      if (X < 0 || Y < 0 || X >= w || Y >= h) continue;
      const o = (Y * w + X) * 4, a = mask.a[m];
      if (bg) { d[o] += (cr - d[o]) * a; d[o + 1] += (cg - d[o + 1]) * a; d[o + 2] += (cb - d[o + 2]) * a; }
      else { d[o] = cr; d[o + 1] = cg; d[o + 2] = cb; d[o + 3] = Math.max(d[o + 3], a * 255); }
    }
  }
  g.putImageData(img, 0, 0);

  const glowAmt = num(p, "glow", 0);
  if (glowAmt > 0) {
    // Soft bloom around bright dots, like the reference's lit-screen look.
    const halo = blurOf(dst, 6 + glowAmt * 10, "dots-glow");
    g.globalCompositeOperation = "screen"; g.globalAlpha = Math.min(1, glowAmt * 1.4);
    g.drawImage(halo, 0, 0);
    g.globalAlpha = 1; g.globalCompositeOperation = "source-over";
  }
}

// ---------- VHS ----------
function vhs(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params, t: number) {
  const w = dst.width, h = dst.height, k = w / 1280, g = dst.getContext("2d")!, amt = num(p, "intensity", 0.6);
  const soft = blurOf(src, 2 + amt * 3, "vhs-soft");
  g.drawImage(soft, 0, 0);
  // Colour bleed: red and blue fringes either side.
  g.globalCompositeOperation = "lighter"; g.globalAlpha = 0.18 + amt * 0.25;
  const tint = scratch("vhs-tint", w, h), tg = tint.getContext("2d")!;
  ([["#ff0040", -4], ["#0040ff", 4]] as const).forEach(([col, dx]) => {
    tg.globalCompositeOperation = "copy"; tg.drawImage(soft, 0, 0);
    tg.globalCompositeOperation = "multiply"; tg.fillStyle = col; tg.fillRect(0, 0, w, h);
    tg.globalCompositeOperation = "destination-in"; tg.drawImage(soft, 0, 0);
    g.drawImage(tint, dx * k * (1 + amt), 0);
  });
  g.globalAlpha = 1; g.globalCompositeOperation = "source-over";
  // A tracking band that rolls down the frame, shifting the picture sideways.
  const band = ((t * 0.35) % 1.3) * h - h * 0.15, bh = h * 0.06;
  g.drawImage(src, 0, band, w, bh, (wobble(t, 1) * 18 + 10) * k * amt, band, w, bh);
  g.fillStyle = `rgba(255,255,255,${0.06 * amt})`; g.fillRect(0, band, w, bh * 0.3);
  // Scanlines and noise.
  g.fillStyle = `rgba(0,0,0,${0.18 + amt * 0.12})`;
  for (let y = 0; y < h; y += Math.max(2, 3 * k)) g.fillRect(0, y, w, Math.max(1, k));
  const r = rng(Math.floor(t * 24) + 9); g.fillStyle = "rgba(255,255,255,.5)";
  for (let i = 0; i < 160 * amt; i++) g.fillRect(r() * w, r() * h, (1 + r() * 3) * k, k);
}

// ---------- CRT ----------
function crt(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params) {
  const w = dst.width, h = dst.height, k = w / 1280, g = dst.getContext("2d")!;
  g.drawImage(src, 0, 0);
  // Glow from a blurred copy, then RGB phosphor stripes and scanlines.
  g.globalCompositeOperation = "lighter"; g.globalAlpha = num(p, "glow", 0.35); g.drawImage(blurOf(src, 14, "crt-glow"), 0, 0);
  g.globalAlpha = 1; g.globalCompositeOperation = "multiply";
  const step = Math.max(3, num(p, "pitch", 4) * k);
  for (let x = 0; x < w; x += step) {
    g.fillStyle = "#ff8080"; g.fillRect(x, 0, step / 3, h);
    g.fillStyle = "#80ff80"; g.fillRect(x + step / 3, 0, step / 3, h);
    g.fillStyle = "#8080ff"; g.fillRect(x + (2 * step) / 3, 0, step / 3, h);
  }
  g.fillStyle = "rgba(0,0,0,.45)";
  for (let y = 0; y < h; y += step) g.fillRect(0, y, w, Math.max(1, step * 0.35));
  g.globalCompositeOperation = "source-over";
  const v = g.createRadialGradient(w / 2, h / 2, h * 0.45, w / 2, h / 2, w * 0.72);
  v.addColorStop(0, "rgba(0,0,0,0)"); v.addColorStop(1, "rgba(0,0,0,.75)"); g.fillStyle = v; g.fillRect(0, 0, w, h);
}

// ---------- Camera shake ----------
function shake(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params, t: number) {
  const w = dst.width, h = dst.height, amt = num(p, "amount", 0.4), speed = num(p, "speed", 1);
  const g = dst.getContext("2d")!, tt = t * speed, z = 1 + amt * 0.08;
  g.translate(w / 2 + wobble(tt, 1) * w * 0.025 * amt, h / 2 + wobble(tt, 2) * h * 0.03 * amt);
  g.rotate(wobble(tt, 3) * 0.02 * amt); g.scale(z, z);
  g.drawImage(src, -w / 2, -h / 2);
  g.setTransform(1, 0, 0, 1, 0, 0);
}

// ---------- Mirror & kaleidoscope ----------
function mirror(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params, t: number) {
  const w = dst.width, h = dst.height, g = dst.getContext("2d")!, mode = str(p, "mode", "left");
  if (mode === "left" || mode === "right") {
    const sx = mode === "left" ? 0 : w / 2;
    g.drawImage(src, sx, 0, w / 2, h, 0, 0, w / 2, h);
    g.save(); g.translate(w, 0); g.scale(-1, 1); g.drawImage(src, sx, 0, w / 2, h, 0, 0, w / 2, h); g.restore();
  } else if (mode === "top") {
    g.drawImage(src, 0, 0, w, h / 2, 0, 0, w, h / 2);
    g.save(); g.translate(0, h); g.scale(1, -1); g.drawImage(src, 0, 0, w, h / 2, 0, 0, w, h / 2); g.restore();
  } else if (mode === "quad") {
    for (const [fx, fy] of [[1, 1], [-1, 1], [1, -1], [-1, -1]]) {
      g.save(); g.translate(fx < 0 ? w : 0, fy < 0 ? h : 0); g.scale(fx, fy);
      g.drawImage(src, 0, 0, w / 2, h / 2, 0, 0, w / 2, h / 2); g.restore();
    }
  } else {
    // Kaleidoscope: one wedge of the picture, rotated and flipped around the centre.
    const n = Math.max(3, Math.round(num(p, "segments", 6))), ang = (Math.PI * 2) / n, r = Math.hypot(w, h) / 2;
    const spin = t * num(p, "spin", 0.15);
    for (let i = 0; i < n; i++) {
      g.save(); g.translate(w / 2, h / 2); g.rotate(i * ang + spin); if (i % 2) g.scale(1, -1);
      g.beginPath(); g.moveTo(0, 0); g.arc(0, 0, r, -ang / 2 - 0.01, ang / 2 + 0.01); g.closePath(); g.clip();
      g.drawImage(src, -w / 2 + w * 0.12, -h / 2); g.restore();
    }
  }
}

// ---------- Edge sketch / neon edges / emboss (3×3 kernels at half resolution) ----------
function kernel(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params, kind: "edges" | "emboss") {
  const w = dst.width, h = dst.height, sw = Math.max(2, Math.round(w / 2)), sh = Math.max(2, Math.round(h / 2));
  const small = scratch("kern", sw, sh), sg = ctx2d(small);
  sg.clearRect(0, 0, sw, sh); sg.drawImage(src, 0, 0, sw, sh);
  const img = sg.getImageData(0, 0, sw, sh), d = img.data, out = sg.createImageData(sw, sh), o = out.data;
  const strength = num(p, "strength", 1.5), mode = str(p, "mode", "sketch");
  for (let y = 1; y < sh - 1; y++) for (let x = 1; x < sw - 1; x++) {
    const i = (y * sw + x) * 4, L = (dx: number, dy: number) => lumOf(d, i + (dy * sw + dx) * 4);
    let v: number;
    if (kind === "emboss") v = 0.5 + (L(1, 1) - L(-1, -1) + (L(1, 0) - L(-1, 0)) * 0.5) * strength;
    else {
      const gx = -L(-1, -1) - 2 * L(-1, 0) - L(-1, 1) + L(1, -1) + 2 * L(1, 0) + L(1, 1);
      const gy = -L(-1, -1) - 2 * L(0, -1) - L(1, -1) + L(-1, 1) + 2 * L(0, 1) + L(1, 1);
      v = Math.min(1, Math.hypot(gx, gy) * strength);
    }
    if (kind === "edges" && mode === "neon") { o[i] = d[i] * v * 1.6; o[i + 1] = d[i + 1] * v * 1.6; o[i + 2] = d[i + 2] * v * 1.6; }
    else { const c = kind === "edges" && mode === "sketch" ? 255 * (1 - v) : kind === "edges" ? 255 * v : 255 * v; o[i] = o[i + 1] = o[i + 2] = c; }
    o[i + 3] = 255;
  }
  sg.putImageData(out, 0, 0);
  const g = dst.getContext("2d")!;
  if (kind === "edges" && mode === "sketch") { g.fillStyle = "#f4f1ea"; g.fillRect(0, 0, w, h); g.globalCompositeOperation = "multiply"; }
  g.imageSmoothingQuality = "high"; g.drawImage(small, 0, 0, w, h); g.globalCompositeOperation = "source-over";
}

// ---------- Gradient maps (thermal, night vision, infrared …) ----------
function gradientMap(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params, t: number) {
  const g = ctx2d(dst), w = dst.width, h = dst.height; g.drawImage(src, 0, 0);
  const map = GRADIENT_MAPS[str(p, "map", "thermal")] ?? GRADIENT_MAPS.thermal, c = num(p, "contrast", 1.2);
  const lut = new Uint8ClampedArray(256 * 3), stops = map.stops.map(hexRgb);
  for (let i = 0; i < 256; i++) {
    const x = (i / 255) * (stops.length - 1), j = Math.min(stops.length - 2, Math.floor(x)), f = x - j;
    for (let ch = 0; ch < 3; ch++) lut[i * 3 + ch] = stops[j][ch] + (stops[j + 1][ch] - stops[j][ch]) * f;
  }
  const img = g.getImageData(0, 0, w, h), d = img.data, r = map.noise ? rng(Math.floor(t * 24) + 5) : null;
  for (let i = 0; i < d.length; i += 4) {
    let l = clamp01((lumOf(d, i) - 0.5) * c + 0.5);
    if (r) l = clamp01(l + (r() - 0.5) * 0.12);
    const k = Math.round(l * 255) * 3; d[i] = lut[k]; d[i + 1] = lut[k + 1]; d[i + 2] = lut[k + 2];
  }
  g.putImageData(img, 0, 0);
  if (map.noise) {
    const v = g.createRadialGradient(w / 2, h / 2, h * 0.3, w / 2, h / 2, w * 0.6);
    v.addColorStop(0, "rgba(0,0,0,0)"); v.addColorStop(1, "rgba(0,0,0,.85)"); g.fillStyle = v; g.fillRect(0, 0, w, h);
  }
}

// ---------- Zoom blur, wave, tilt-shift, glow ----------
function zoomblur(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params) {
  const w = dst.width, h = dst.height, g = dst.getContext("2d")!, amt = num(p, "amount", 0.5), n = 10;
  g.drawImage(src, 0, 0);
  for (let i = 1; i <= n; i++) {
    const k = 1 + (i / n) * amt * 0.35;
    g.globalAlpha = 0.5 / i + 0.05; g.drawImage(src, (w - w * k) / 2, (h - h * k) / 2, w * k, h * k);
  }
  g.globalAlpha = 1;
}
function wave(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params, t: number) {
  const w = dst.width, h = dst.height, k = w / 1280, g = dst.getContext("2d")!;
  const amp = num(p, "amplitude", 18) * k, freq = num(p, "frequency", 3), speed = num(p, "speed", 2), vertical = p.direction === "vertical";
  const step = Math.max(1, Math.round(2 * k));
  if (!vertical) for (let y = 0; y < h; y += step) g.drawImage(src, 0, y, w, step, Math.sin((y / h) * freq * Math.PI * 2 + t * speed) * amp, y, w, step);
  else for (let x = 0; x < w; x += step) g.drawImage(src, x, 0, step, h, x, Math.sin((x / w) * freq * Math.PI * 2 + t * speed) * amp, step, h);
}
function tiltshift(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params) {
  const w = dst.width, h = dst.height, g = dst.getContext("2d")!;
  const focus = num(p, "focus", 0.55), band = num(p, "band", 0.18);
  g.drawImage(src, 0, 0);
  const m = scratch("tilt-mask", w, h), mg = m.getContext("2d")!;
  mg.globalCompositeOperation = "copy"; mg.drawImage(blurOf(src, num(p, "blur", 10), "tilt-blur"), 0, 0);
  const grad = mg.createLinearGradient(0, 0, 0, h), a = Math.max(0, focus - band), b = Math.min(1, focus + band);
  grad.addColorStop(0, "#000"); grad.addColorStop(Math.max(0, a - 0.12), "#000"); grad.addColorStop(a, "rgba(0,0,0,0)");
  grad.addColorStop(b, "rgba(0,0,0,0)"); grad.addColorStop(Math.min(1, b + 0.12), "#000"); grad.addColorStop(1, "#000");
  mg.globalCompositeOperation = "destination-in"; mg.fillStyle = grad; mg.fillRect(0, 0, w, h);
  g.drawImage(m, 0, 0);
  if (p.saturate === true) { g.globalCompositeOperation = "saturation"; g.fillStyle = "hsl(0,80%,50%)"; g.globalAlpha = 0.25; g.fillRect(0, 0, w, h); g.globalAlpha = 1; g.globalCompositeOperation = "source-over"; }
}
function glow(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params) {
  const g = dst.getContext("2d")!;
  g.drawImage(src, 0, 0);
  g.globalCompositeOperation = "screen"; g.globalAlpha = num(p, "strength", 0.7);
  g.drawImage(blurOf(src, num(p, "radius", 18), "glow"), 0, 0);
  g.globalAlpha = 1; g.globalCompositeOperation = "source-over";
}

// ---------- Threshold & solarize ----------
function threshold(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params) {
  const g = ctx2d(dst); g.drawImage(src, 0, 0);
  const lv = num(p, "level", 0.5), [r0, g0, b0] = hexRgb(str(p, "dark", "#000000")), [r1, g1, b1] = hexRgb(str(p, "light", "#ffffff"));
  const img = g.getImageData(0, 0, dst.width, dst.height), d = img.data;
  for (let i = 0; i < d.length; i += 4) {
    const on = lumOf(d, i) >= lv;
    d[i] = on ? r1 : r0; d[i + 1] = on ? g1 : g0; d[i + 2] = on ? b1 : b0;
  }
  g.putImageData(img, 0, 0);
}
function solarize(src: HTMLCanvasElement, dst: HTMLCanvasElement, p: Params) {
  const g = ctx2d(dst); g.drawImage(src, 0, 0);
  const th = num(p, "threshold", 0.5) * 255;
  const img = g.getImageData(0, 0, dst.width, dst.height), d = img.data;
  for (let i = 0; i < d.length; i += 4) for (let c = 0; c < 3; c++) if (d[i + c] > th) d[i + c] = 255 - d[i + c];
  g.putImageData(img, 0, 0);
}

function css(src: HTMLCanvasElement, dst: HTMLCanvasElement, filter: string) {
  const g = dst.getContext("2d")!;
  if (nativeCanvasFilter()) { g.filter = filter; g.drawImage(src, 0, 0); g.filter = "none"; }
  else { g.drawImage(src, 0, 0); applyFilterToCanvas(dst, filter); }
}

/**
 * Run a clip's effect stack over `src` and return the canvas holding the result.
 * Stylize effects blend with what came before by their amount; colour effects use it as strength.
 */
// Engines that paint pixels where the input was transparent (backgrounds, noise, vignettes, kernels).
// On an object they get cut back to its shape plus a soft margin.
const FILLS = new Set<string>(["ascii", "dots", "vhs", "crt", "glitch", "edges", "emboss", "gradientmap"]);
function maskToObject(out: HTMLCanvasElement, shape: HTMLCanvasElement) {
  const w = out.width, h = out.height;
  const mask = scratch("obj-mask", w, h), mg = mask.getContext("2d")!;
  mg.globalAlpha = 1; mg.globalCompositeOperation = "copy"; mg.drawImage(blurOf(shape, 10, "obj-halo"), 0, 0);
  mg.globalCompositeOperation = "source-over";
  mg.drawImage(mask, 0, 0); mg.drawImage(mask, 0, 0); // firm up the halo
  mg.drawImage(shape, 0, 0); // and keep the object itself fully covered
  const g = out.getContext("2d")!;
  g.globalCompositeOperation = "destination-in"; g.drawImage(mask, 0, 0); g.globalCompositeOperation = "source-over";
}

/**
 * Run a clip's effect stack over `src`. With `object`, effects stay on the clip's own shape
 * (a title, a placed photo) instead of filling the whole frame.
 */
export function runEffects(src: HTMLCanvasElement, stack: ClipEffect[], t: number, opts: { object?: boolean } = {}): HTMLCanvasElement {
  const object = !!opts.object;
  const w = src.width, h = src.height;
  let cur = src, flip = 0;
  for (const fx of stack) {
    if (!fx.enabled) continue;
    const def = effectDef(fx.id); if (!def) continue;
    const out = scratch("fx" + (flip ^= 1), w, h), g = out.getContext("2d")!;
    g.setTransform(1, 0, 0, 1, 0, 0); g.globalAlpha = 1; g.globalCompositeOperation = "source-over";
    g.clearRect(0, 0, w, h);
    if (def.engine === "css") { css(cur, out, def.css!(fx.amount)); cur = out; continue; }
    switch (def.engine) {
      case "ascii": ascii(cur, out, fx.params, t, object); break;
      case "pixelate": pixelate(cur, out, fx.params); break;
      case "dither": dither(cur, out, fx.params); break;
      case "rgbsplit": rgbsplit(cur, out, fx.params, t); break;
      case "glitch": glitch(cur, out, fx.params, t); break;
      case "posterize": posterize(cur, out, fx.params); break;
      case "duotone": duotone(cur, out, fx.params); break;
      case "dots": dotsEngine(cur, out, fx.params, object); break;
      case "vhs": vhs(cur, out, fx.params, t); break;
      case "crt": crt(cur, out, fx.params); break;
      case "shake": shake(cur, out, fx.params, t); break;
      case "mirror": mirror(cur, out, fx.params, t); break;
      case "edges": kernel(cur, out, fx.params, "edges"); break;
      case "emboss": kernel(cur, out, fx.params, "emboss"); break;
      case "gradientmap": gradientMap(cur, out, fx.params, t); break;
      case "zoomblur": zoomblur(cur, out, fx.params); break;
      case "wave": wave(cur, out, fx.params, t); break;
      case "tiltshift": tiltshift(cur, out, fx.params); break;
      case "glow": glow(cur, out, fx.params); break;
      case "threshold": threshold(cur, out, fx.params); break;
      case "solarize": solarize(cur, out, fx.params); break;
    }
    if (fx.amount < 0.999) {
      // Mix: previous result underneath, effect on top at `amount`.
      const mix = scratch("fx-mix", w, h), mg = mix.getContext("2d")!;
      mg.globalCompositeOperation = "copy"; mg.globalAlpha = 1; mg.drawImage(cur, 0, 0);
      mg.globalCompositeOperation = "source-over"; mg.globalAlpha = fx.amount; mg.drawImage(out, 0, 0); mg.globalAlpha = 1;
      g.globalCompositeOperation = "copy"; g.drawImage(mix, 0, 0); g.globalCompositeOperation = "source-over";
    }
    if (object && FILLS.has(def.engine)) maskToObject(out, cur);
    cur = out;
  }
  return cur;
}

