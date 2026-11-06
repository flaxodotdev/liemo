// Asset library: procedural sample clips, music, stills, overlays, fonts, text styles and effects.
import { W, H, rng, lerp, tc } from "./time";

type Ctx = CanvasRenderingContext2D;
export type AssetType = "scene" | "video" | "image" | "photo" | "overlay" | "text" | "music" | "audio";

export interface TextProps {
  content: string;
  font: string;
  size: number;
  weight: number;
  color: string;
  align: CanvasTextAlign;
  anim: "none" | "fade" | "rise" | "slide" | "type";
  box: boolean;
  y: number;
}

export interface Asset {
  id: string;
  type: AssetType;
  name: string;
  sub: string;
  dur: number;
  draw?: (c: Ctx, t: number) => void;
  blend?: GlobalCompositeOperation;
  bitmap?: CanvasImageSource;
  thumb?: string;
  url?: string;
  text?: TextProps;
  // music
  bpm?: number;
  roots?: number[];
  quality?: keyof typeof CHORDS;
  hue?: string;
}

export const CHORDS = { maj: [0, 4, 7], min: [0, 3, 7], maj7: [0, 4, 7, 11], min7: [0, 3, 7, 10], sus: [0, 5, 7, 14] };

// ---------- Sample scenes (stand-ins for footage) ----------
export const SCENES: Asset[] = [
  { id: "sc-coast", type: "scene", name: "Coastline at dusk", sub: "4K · 24p · Lisbon", dur: 6, draw(c, t) {
    const g = c.createLinearGradient(0, 0, 0, H * 0.62);
    g.addColorStop(0, "#2b1d5c"); g.addColorStop(0.55, "#d4507a"); g.addColorStop(1, "#ff9a4d");
    c.fillStyle = g; c.fillRect(0, 0, W, H);
    c.fillStyle = "#ffd9a0"; c.beginPath(); c.arc(W * 0.62, H * 0.5 + t * 14, 70, 0, Math.PI * 2); c.fill();
    const s = c.createLinearGradient(0, H * 0.6, 0, H); s.addColorStop(0, "#3a2a6a"); s.addColorStop(1, "#0d0b24");
    c.fillStyle = s; c.fillRect(0, H * 0.6, W, H * 0.4);
    c.strokeStyle = "rgba(255,200,150,.35)"; c.lineWidth = 2;
    for (let i = 0; i < 14; i++) {
      const y = H * 0.62 + i * i * 1.9; c.beginPath();
      for (let x = 0; x <= W; x += 20) c.lineTo(x, y + Math.sin(x * 0.02 + t * 1.6 + i) * (1 + i * 0.4));
      c.stroke();
    }
  } },
  { id: "sc-drive", type: "scene", name: "Night drive", sub: "HD · 30p · Dashcam", dur: 5, draw(c, t) {
    c.fillStyle = "#07070d"; c.fillRect(0, 0, W, H);
    const r = rng(7);
    for (let i = 0; i < 40; i++) {
      const bw = 40 + r() * 90, bh = 120 + r() * 260;
      const x = (((i * 70 - t * 160) % (W + 200)) + W + 200) % (W + 200) - 100;
      c.fillStyle = "#12121e"; c.fillRect(x, H * 0.55 - bh, bw, bh);
      for (let k = 0; k < 18; k++) if (r() > 0.55) {
        c.fillStyle = r() > 0.5 ? "#ffcf6b" : "#6bc8ff"; c.globalAlpha = 0.7;
        c.fillRect(x + 6 + (k % 3) * (bw / 3.2), H * 0.55 - bh + 10 + Math.floor(k / 3) * 22, 6, 9);
      }
      c.globalAlpha = 1;
    }
    c.fillStyle = "#0e0e16"; c.beginPath(); c.moveTo(0, H); c.lineTo(W * 0.46, H * 0.55); c.lineTo(W * 0.54, H * 0.55); c.lineTo(W, H); c.fill();
    c.fillStyle = "#f5f5f5";
    for (let i = 0; i < 8; i++) {
      const p = (i / 8 + t * 0.6) % 1;
      c.fillRect(W / 2 - lerp(2, 18, p * p) / 2, lerp(H * 0.56, H, p * p), lerp(2, 18, p * p), lerp(4, 70, p * p));
    }
    const glow = c.createRadialGradient(W * 0.3, H * 0.5, 0, W * 0.3, H * 0.5, 300);
    glow.addColorStop(0, "rgba(255,60,90,.25)"); glow.addColorStop(1, "rgba(0,0,0,0)");
    c.fillStyle = glow; c.fillRect(0, 0, W, H);
  } },
  { id: "sc-aurora", type: "scene", name: "Aurora field", sub: "4K · 24p · Timelapse", dur: 6, draw(c, t) {
    c.fillStyle = "#04060f"; c.fillRect(0, 0, W, H);
    const r = rng(3); c.fillStyle = "#fff";
    for (let i = 0; i < 160; i++) { c.globalAlpha = 0.3 + 0.7 * Math.abs(Math.sin(t * 2 + i)); c.fillRect(r() * W, r() * H * 0.7, 1.5, 1.5); }
    c.globalAlpha = 1; c.globalCompositeOperation = "screen";
    ([["rgba(60,255,170,.22)", 0], ["rgba(106,76,245,.28)", 2], ["rgba(212,77,240,.18)", 4]] as const).forEach(([col, o]) => {
      c.fillStyle = col;
      for (let b = 0; b < 3; b++) {
        c.beginPath(); c.moveTo(0, H);
        for (let x = 0; x <= W; x += 16) c.lineTo(x, H * 0.35 + Math.sin(x * 0.004 + t * 0.7 + o + b) * 80 + Math.sin(x * 0.011 - t + b) * 30 - b * 25);
        c.lineTo(W, H); c.fill();
      }
    });
    c.globalCompositeOperation = "source-over";
    c.fillStyle = "#02030a"; c.beginPath(); c.moveTo(0, H);
    for (let x = 0; x <= W; x += 40) c.lineTo(x, H * 0.82 - Math.abs(Math.sin(x * 0.007)) * 90 - Math.sin(x * 0.03) * 12);
    c.lineTo(W, H); c.fill();
  } },
  { id: "sc-studio", type: "scene", name: "Studio sweep", sub: "HD · 60p · Product", dur: 4, draw(c, t) {
    const g = c.createLinearGradient(0, 0, 0, H); g.addColorStop(0, "#1a1a1a"); g.addColorStop(0.7, "#3a3a3a"); g.addColorStop(1, "#202020");
    c.fillStyle = g; c.fillRect(0, 0, W, H);
    const lx = W / 2 + Math.sin(t * 0.9) * 380;
    const s = c.createRadialGradient(lx, H * 0.5, 10, lx, H * 0.5, 520);
    s.addColorStop(0, "rgba(255,255,255,.35)"); s.addColorStop(1, "rgba(255,255,255,0)");
    c.fillStyle = s; c.fillRect(0, 0, W, H);
    c.save(); c.translate(W / 2, H * 0.58); c.rotate(t * 0.5); c.fillStyle = "#f2f2f2"; c.fillRect(-80, -80, 160, 160); c.restore();
    c.fillStyle = "rgba(0,0,0,.35)"; c.beginPath(); c.ellipse(W / 2, H * 0.78, 150, 18, 0, 0, Math.PI * 2); c.fill();
  } },
  { id: "sc-ink", type: "scene", name: "Ink in water", sub: "4K · 120p · Macro", dur: 5, draw(c, t) {
    c.fillStyle = "#f3f0ea"; c.fillRect(0, 0, W, H);
    const r = rng(11);
    for (let i = 0; i < 9; i++) {
      const x = W * (0.2 + r() * 0.6), y = H * (0.2 + r() * 0.5) + t * 20 * (r() + 0.3), rad = 40 + t * (30 + r() * 50);
      const col = i % 3 === 0 ? "106,76,245" : i % 3 === 1 ? "212,77,240" : "20,20,40";
      const g = c.createRadialGradient(x, y, 0, x, y, rad);
      g.addColorStop(0, `rgba(${col},.55)`); g.addColorStop(1, `rgba(${col},0)`);
      c.fillStyle = g; c.beginPath(); c.arc(x, y, rad, 0, Math.PI * 2); c.fill();
    }
  } },
];

// ---------- Music (synthesized in the browser) ----------
export const MUSIC: Asset[] = [
  { id: "mu-golden", type: "music", name: "Golden Hour", sub: "Warm · Lo-fi", bpm: 84, dur: 32, roots: [57, 53, 60, 55], quality: "maj7", hue: "#ff7a3d" },
  { id: "mu-neon", type: "music", name: "Neon Tide", sub: "Synthwave · Drive", bpm: 108, dur: 30, roots: [45, 41, 48, 43], quality: "min", hue: "#d44df0" },
  { id: "mu-paper", type: "music", name: "Paper Planes", sub: "Acoustic · Bright", bpm: 96, dur: 28, roots: [60, 55, 57, 53], quality: "maj", hue: "#22c55e" },
  { id: "mu-orbit", type: "music", name: "Slow Orbit", sub: "Ambient · Cinematic", bpm: 70, dur: 40, roots: [50, 46, 53, 48], quality: "sus", hue: "#6a4cf5" },
  { id: "mu-tape", type: "music", name: "Tape Machine", sub: "Hip-hop · Dusty", bpm: 90, dur: 24, roots: [52, 48, 55, 50], quality: "min7", hue: "#ff5577" },
];

// ---------- Stills ----------
function spot(c: Ctx, a: string, b: string, fx: number, fy: number) {
  c.fillStyle = b; c.fillRect(0, 0, W, H);
  const g = c.createRadialGradient(W * fx, H * fy, 0, W * fx, H * fy, W * 0.8);
  g.addColorStop(0, a); g.addColorStop(0.45, a + "88"); g.addColorStop(1, b + "00");
  c.fillStyle = g; c.fillRect(0, 0, W, H);
}
export const IMAGES: Asset[] = [
  { id: "im-magenta", type: "image", name: "Magenta bloom", sub: "Gradient still", dur: 5, draw: (c) => spot(c, "#d44df0", "#3b0a52", 0.3, 0.35) },
  { id: "im-violet", type: "image", name: "Violet field", sub: "Gradient still", dur: 5, draw: (c) => spot(c, "#6a4cf5", "#0b0838", 0.7, 0.4) },
  { id: "im-sunset", type: "image", name: "Sunset wash", sub: "Gradient still", dur: 5, draw: (c) => spot(c, "#ff7a3d", "#4a0f2a", 0.5, 0.8) },
  { id: "im-coral", type: "image", name: "Coral glow", sub: "Gradient still", dur: 5, draw: (c) => spot(c, "#ff5577", "#1d0710", 0.2, 0.7) },
  { id: "im-grid", type: "image", name: "Blueprint grid", sub: "Pattern", dur: 5, draw: (c) => {
    c.fillStyle = "#0b1220"; c.fillRect(0, 0, W, H); c.strokeStyle = "rgba(0,153,255,.25)"; c.lineWidth = 1;
    for (let x = 0; x < W; x += 40) { c.beginPath(); c.moveTo(x + 0.5, 0); c.lineTo(x + 0.5, H); c.stroke(); }
    for (let y = 0; y < H; y += 40) { c.beginPath(); c.moveTo(0, y + 0.5); c.lineTo(W, y + 0.5); c.stroke(); }
  } },
  { id: "im-dots", type: "image", name: "Halftone", sub: "Pattern", dur: 5, draw: (c) => {
    c.fillStyle = "#111"; c.fillRect(0, 0, W, H); c.fillStyle = "#ff7a3d";
    for (let y = 0; y < H; y += 24) for (let x = 0; x < W; x += 24) {
      c.beginPath(); c.arc(x + 12, y + 12, 1 + 9 * (x / W) * (1 - y / H), 0, Math.PI * 2); c.fill();
    }
  } },
];

// ---------- Overlays ----------
let grainTile: HTMLCanvasElement | null = null;
export const OVERLAYS: Asset[] = [
  { id: "ov-leak", type: "overlay", name: "Light leak", sub: "Screen blend", dur: 6, blend: "screen", draw(c, t) {
    const x = W * (0.2 + 0.6 * (0.5 + 0.5 * Math.sin(t * 0.6)));
    const g = c.createRadialGradient(x, H * 0.3, 0, x, H * 0.3, W * 0.7);
    g.addColorStop(0, "rgba(255,140,60,.9)"); g.addColorStop(0.4, "rgba(255,60,110,.35)"); g.addColorStop(1, "rgba(0,0,0,0)");
    c.fillStyle = g; c.fillRect(0, 0, W, H);
  } },
  { id: "ov-grain", type: "overlay", name: "Film grain", sub: "Overlay blend", dur: 6, blend: "overlay", draw(c, t) {
    if (!grainTile) {
      grainTile = document.createElement("canvas"); grainTile.width = grainTile.height = 256;
      const g = grainTile.getContext("2d")!, d = g.createImageData(256, 256), r = rng(5);
      for (let i = 0; i < d.data.length; i += 4) { const v = r() * 255; d.data[i] = d.data[i + 1] = d.data[i + 2] = v; d.data[i + 3] = 90; }
      g.putImageData(d, 0, 0);
    }
    const f = Math.floor(t * 24); c.save(); c.translate(-(f * 73) % 256, -(f * 151) % 256);
    c.fillStyle = c.createPattern(grainTile, "repeat")!; c.fillRect(0, 0, W + 256, H + 256); c.restore();
  } },
  { id: "ov-letterbox", type: "overlay", name: "Letterbox 2.39:1", sub: "Cinema bars", dur: 6, blend: "source-over", draw(c) {
    const bar = (H - W / 2.39) / 2; c.fillStyle = "#000"; c.fillRect(0, 0, W, bar); c.fillRect(0, H - bar, W, bar);
  } },
  { id: "ov-vignette", type: "overlay", name: "Vignette", sub: "Edge falloff", dur: 6, blend: "source-over", draw(c) {
    const g = c.createRadialGradient(W / 2, H / 2, H * 0.35, W / 2, H / 2, W * 0.65);
    g.addColorStop(0, "rgba(0,0,0,0)"); g.addColorStop(1, "rgba(0,0,0,.85)"); c.fillStyle = g; c.fillRect(0, 0, W, H);
  } },
  { id: "ov-scan", type: "overlay", name: "VHS scanlines", sub: "Multiply", dur: 6, blend: "multiply", draw(c, t) {
    c.fillStyle = "rgba(0,0,0,.35)"; for (let y = 0; y < H; y += 4) c.fillRect(0, y, W, 2);
    c.fillStyle = "rgba(255,255,255,.08)"; c.fillRect(0, (t * 220) % H, W, 30);
  } },
  { id: "ov-dust", type: "overlay", name: "Dust & scratches", sub: "Screen blend", dur: 6, blend: "screen", draw(c, t) {
    const r = rng(Math.floor(t * 12) + 1); c.strokeStyle = "rgba(255,255,255,.5)"; c.fillStyle = "rgba(255,255,255,.6)";
    for (let i = 0; i < 6; i++) { const x = r() * W; c.lineWidth = r() * 1.5; c.beginPath(); c.moveTo(x, 0); c.lineTo(x + r() * 10, H); c.stroke(); }
    for (let i = 0; i < 25; i++) { c.beginPath(); c.arc(r() * W, r() * H, r() * 2.5, 0, Math.PI * 2); c.fill(); }
  } },
  { id: "ov-frame", type: "overlay", name: "REC frame", sub: "Camera HUD", dur: 6, blend: "source-over", draw(c, t) {
    c.strokeStyle = "rgba(255,255,255,.85)"; c.lineWidth = 3; const m = 40, l = 50;
    ([[m, m, 1, 1], [W - m, m, -1, 1], [m, H - m, 1, -1], [W - m, H - m, -1, -1]] as const).forEach(([x, y, dx, dy]) => {
      c.beginPath(); c.moveTo(x, y + l * dy); c.lineTo(x, y); c.lineTo(x + l * dx, y); c.stroke();
    });
    if (Math.floor(t * 2) % 2 === 0) { c.fillStyle = "#ff3b3b"; c.beginPath(); c.arc(m + 30, m + 34, 9, 0, Math.PI * 2); c.fill(); }
    c.fillStyle = "#fff"; c.font = `600 24px ${fontStack("JetBrains Mono")}`; c.fillText("REC", m + 50, m + 43);
    c.fillText(tc(t), W - m - 200, m + 43);
  } },
];

// ---------- Fonts & text styles ----------
export interface FontDef { family: string; cssVar: string; role: string; weight: number; track: number; fallback: string }
export const FONTS: FontDef[] = [
  { family: "Mona Sans", cssVar: "--font-mona", role: "Display · Geometric", weight: 700, track: -0.045, fallback: "sans-serif" },
  { family: "Inter", cssVar: "--font-inter", role: "Sans · Interface", weight: 600, track: -0.02, fallback: "sans-serif" },
  { family: "Instrument Serif", cssVar: "--font-instrument", role: "Serif · Editorial", weight: 400, track: -0.01, fallback: "serif" },
  { family: "Bebas Neue", cssVar: "--font-bebas", role: "Condensed · Poster", weight: 400, track: 0.02, fallback: "sans-serif" },
  { family: "JetBrains Mono", cssVar: "--font-jetbrains", role: "Mono · Technical", weight: 500, track: 0, fallback: "monospace" },
  { family: "Caveat", cssVar: "--font-caveat", role: "Script · Handwritten", weight: 700, track: 0, fallback: "cursive" },
];
// next/font registers each family under its own name; read it from the CSS variable so canvas text matches the UI.
const stackCache = new Map<string, string>();
export function fontStack(family: string) {
  const def = FONTS.find((f) => f.family === family) ?? FONTS[0];
  if (stackCache.has(def.family)) return stackCache.get(def.family)!;
  const v = typeof document !== "undefined" ? getComputedStyle(document.documentElement).getPropertyValue(def.cssVar).trim() : "";
  const stack = v || `"${def.family}", ${def.fallback}`;
  if (v) stackCache.set(def.family, stack);
  return stack;
}

export const TEXT_PRESETS: Asset[] = [
  { id: "tx-title", type: "text", name: "Title", sub: "Centered display", dur: 4, text: { content: "Summer, slowly", font: "Mona Sans", size: 120, weight: 700, color: "#ffffff", align: "center", anim: "rise", box: false, y: 0 } },
  { id: "tx-lower", type: "text", name: "Lower third", sub: "Name + role", dur: 4, text: { content: "Marta Silva\nDirector of photography", font: "Inter", size: 44, weight: 600, color: "#ffffff", align: "left", anim: "slide", box: true, y: 200 } },
  { id: "tx-caption", type: "text", name: "Caption", sub: "Subtitle line", dur: 3, text: { content: "The light changes every ten minutes here.", font: "Inter", size: 36, weight: 500, color: "#ffffff", align: "center", anim: "fade", box: true, y: 260 } },
  { id: "tx-quote", type: "text", name: "Serif quote", sub: "Editorial", dur: 4, text: { content: "“We came for the sea,\nwe stayed for the light.”", font: "Instrument Serif", size: 76, weight: 400, color: "#ffffff", align: "center", anim: "fade", box: false, y: 0 } },
  { id: "tx-poster", type: "text", name: "Poster", sub: "Condensed caps", dur: 3, text: { content: "CHAPTER TWO", font: "Bebas Neue", size: 150, weight: 400, color: "#ff7a3d", align: "center", anim: "type", box: false, y: 0 } },
  { id: "tx-note", type: "text", name: "Handwritten", sub: "Script note", dur: 3, text: { content: "day three", font: "Caveat", size: 90, weight: 700, color: "#ffd9a0", align: "center", anim: "rise", box: false, y: -180 } },
];
export const TEXT_COLORS = ["#ffffff", "#000000", "#ffd9a0", "#ff7a3d", "#ff5577", "#d44df0", "#6a4cf5", "#0099ff", "#22c55e"];
export const TEXT_ANIMS: [TextProps["anim"], string][] = [["none", "None"], ["fade", "Fade"], ["rise", "Rise"], ["slide", "Slide"], ["type", "Type"]];

// ---------- Transitions ----------
export const TRANSITIONS = [
  { id: "fadein", name: "Fade in", sub: "0.5 s from black" },
  { id: "fadeout", name: "Fade out", sub: "0.5 s to black" },
  { id: "both", name: "Fade both", sub: "0.5 s each end" },
] as const;
export type TransitionId = (typeof TRANSITIONS)[number]["id"];

// ---------- Registry ----------
export const ASSETS = new Map<string, Asset>();
[...SCENES, ...MUSIC, ...IMAGES, ...OVERLAYS, ...TEXT_PRESETS].forEach((a) => ASSETS.set(a.id, a));
