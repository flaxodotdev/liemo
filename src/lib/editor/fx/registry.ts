// Effect catalogue. Each library entry is a preset of an engine: the engine does the pixel work,
// the preset picks its starting parameters. Users can change every parameter in the inspector.

export type ParamValue = number | string | boolean;
export type Params = Record<string, ParamValue>;

export type ParamDef =
  | { key: string; label: string; type: "range"; min: number; max: number; step: number; unit?: string }
  | { key: string; label: string; type: "select"; options: [string, string][] }
  | { key: string; label: string; type: "color" }
  | { key: string; label: string; type: "toggle" };

export type Engine =
  | "css" | "ascii" | "dots" | "pixelate" | "dither" | "rgbsplit" | "glitch" | "posterize" | "duotone"
  | "vhs" | "crt" | "shake" | "mirror" | "edges" | "emboss" | "gradientmap" | "zoomblur" | "wave" | "tiltshift" | "glow" | "threshold" | "solarize";
export type EffectGroup = "stylize" | "retro" | "distort" | "color";
export const EFFECT_GROUPS: [EffectGroup, string][] = [["stylize", "Stylize"], ["retro", "Retro"], ["distort", "Distort & camera"], ["color", "Color & look"]];

export interface EffectDef {
  id: string;
  name: string;
  group: EffectGroup;
  engine: Engine;
  /** For css effects: the filter string at a given strength (0–1). */
  css?: (amount: number) => string;
  defaults: Params;
  params: ParamDef[];
}

export interface ClipEffect { uid: string; id: string; enabled: boolean; amount: number; params: Params }

// ---------- Glyph sets for the ASCII engine ----------
export type GlyphSet =
  | { kind: "chars"; label: string; chars: string; emoji?: boolean; rain?: boolean; font?: "mono" | "sans" }
  | { kind: "shape"; label: string; shape: "circle" | "square" | "diamond" | "cross" | "hatch" | "ring"; levels: number };

export const GLYPH_SETS: Record<string, GlyphSet> = {
  classic: { kind: "chars", label: "Classic ASCII", chars: " .:-=+*#%@" },
  detailed: { kind: "chars", label: "Detailed ASCII", chars: " .'`^\",:;Il!i~+_-?]}1)|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@" },
  blocks: { kind: "chars", label: "Blocks", chars: " ░▒▓█" },
  braille: { kind: "chars", label: "Braille", chars: " ⠁⠂⠃⠉⠋⠛⠟⠿⡿⣿" },
  binary: { kind: "chars", label: "Binary", chars: " .01" },
  rain: { kind: "chars", label: "Katakana rain", chars: "ｦｱｳｴｵｶｷｹｺｻｼｽｾｿﾀﾂﾃﾅﾆﾇﾈﾊﾋﾎﾏﾐﾑﾒﾓﾔﾕﾗﾘﾜ0123456789", rain: true },
  moon: { kind: "chars", label: "Moon phases", chars: "🌑🌘🌗🌖🌕", emoji: true },
  dots: { kind: "shape", label: "Halftone dots", shape: "circle", levels: 12 },
  squares: { kind: "shape", label: "Pixel squares", shape: "square", levels: 12 },
  diamonds: { kind: "shape", label: "Diamonds", shape: "diamond", levels: 12 },
  crosses: { kind: "shape", label: "Crosses", shape: "cross", levels: 10 },
  hatch: { kind: "shape", label: "Hatch lines", shape: "hatch", levels: 10 },
  rings: { kind: "shape", label: "Rings", shape: "ring", levels: 10 },
};

export const GRADIENT_MAPS: Record<string, { label: string; stops: string[]; noise?: boolean }> = {
  thermal: { label: "Thermal", stops: ["#07001a", "#2b0a5c", "#a1158d", "#ff4d2e", "#ffd23f", "#ffffff"] },
  night: { label: "Night vision", stops: ["#000800", "#0b3d0b", "#2fbf2f", "#c8ffc8"], noise: true },
  infrared: { label: "Infrared", stops: ["#12001f", "#5c1f73", "#ff3d8b", "#ffd1e6", "#ffffff"] },
  ocean: { label: "Deep sea", stops: ["#000814", "#003566", "#0099ff", "#90e0ef"] },
  ember: { label: "Ember", stops: ["#0d0000", "#5a0b0b", "#ff5a1f", "#ffd9a0"] },
};

export const PALETTES: Record<string, { label: string; colors: string[] }> = {
  "1bit": { label: "1-bit", colors: ["#000000", "#ffffff"] },
  gameboy: { label: "Game Boy", colors: ["#0f380f", "#306230", "#8bac0f", "#9bbc0f"] },
  cga: { label: "CGA", colors: ["#000000", "#55ffff", "#ff55ff", "#ffffff"] },
  pico8: { label: "PICO-8", colors: ["#000000", "#1d2b53", "#7e2553", "#008751", "#ab5236", "#5f574f", "#c2c3c7", "#fff1e8", "#ff004d", "#ffa300", "#ffec27", "#00e436", "#29adff", "#83769c", "#ff77a8", "#ffccaa"] },
  sunset: { label: "Sunset", colors: ["#1b0f2e", "#6a4cf5", "#d44df0", "#ff5577", "#ff7a3d", "#ffd9a0"] },
};

const ASCII_PARAMS: ParamDef[] = [
  { key: "set", label: "Glyphs", type: "select", options: Object.entries(GLYPH_SETS).map(([k, v]) => [k, v.label]) },
  { key: "cell", label: "Cell size", type: "range", min: 4, max: 48, step: 1, unit: "px" },
  { key: "color", label: "Colour", type: "select", options: [["source", "Original"], ["mono", "White"], ["tint", "Tint"]] },
  { key: "tint", label: "Tint", type: "color" },
  { key: "bg", label: "Background", type: "select", options: [["black", "Black"], ["dim", "Dimmed video"], ["none", "None"]] },
  { key: "contrast", label: "Contrast", type: "range", min: 0.5, max: 3, step: 0.05, unit: "×" },
  { key: "invert", label: "Invert", type: "toggle" },
];
export const DOT_PALETTES: Record<string, { label: string; stops: string[] }> = {
  ice: { label: "Ice blue", stops: ["#0b1d4a", "#1d4ed8", "#3b82f6", "#93c5fd", "#ffffff"] },
  amber: { label: "Amber", stops: ["#2a1200", "#b45309", "#f59e0b", "#fde68a", "#fffbeb"] },
  phosphor: { label: "Phosphor green", stops: ["#021a06", "#15803d", "#22c55e", "#bbf7d0", "#f0fff4"] },
  sunset: { label: "Sunset", stops: ["#2b0a3d", "#6a4cf5", "#d44df0", "#ff7a3d", "#ffd9a0"] },
};
const DOT_PARAMS: ParamDef[] = [
  { key: "pattern", label: "Pattern", type: "select", options: [["halftone", "Rounded halftone"], ["braille", "Braille dot grid"]] },
  { key: "cell", label: "Cell size", type: "range", min: 4, max: 48, step: 1, unit: "px" },
  { key: "size", label: "Dot size", type: "range", min: 0.3, max: 1, step: 0.01 },
  { key: "color", label: "Colour", type: "select", options: [["white", "White"], ["source", "Original"], ...Object.entries(DOT_PALETTES).map(([k, v]): [string, string] => [k, v.label])] },
  { key: "bg", label: "Background", type: "select", options: [["black", "Black"], ["midnight", "Midnight"], ["none", "None"]] },
  { key: "contrast", label: "Contrast", type: "range", min: 0.5, max: 3, step: 0.05, unit: "×" },
  { key: "glow", label: "Glow", type: "range", min: 0, max: 1, step: 0.01 },
  { key: "invert", label: "Invert", type: "toggle" },
];
const dots = (id: string, name: string, p: Params): EffectDef => ({
  id, name, group: "stylize", engine: "dots", params: DOT_PARAMS,
  defaults: { pattern: "halftone", cell: 14, size: 0.92, color: "white", bg: "black", contrast: 1.15, glow: 0, invert: false, ...p },
});
const ascii = (id: string, name: string, p: Params): EffectDef => ({
  id, name, group: "stylize", engine: "ascii", params: ASCII_PARAMS,
  defaults: { set: "classic", cell: 12, color: "source", tint: "#22c55e", bg: "black", contrast: 1.3, invert: false, ...p },
});

export const EFFECTS: EffectDef[] = [
  // Stylize
  ascii("ascii", "ASCII", { set: "classic", color: "mono" }),
  ascii("ascii-color", "ASCII colour", { set: "detailed", cell: 9 }),
  ascii("blocks", "Blocks", { set: "blocks", cell: 10 }),
  dots("rounded-dots", "Rounded dots", {}),
  dots("braille", "Braille", { pattern: "braille", cell: 9, size: 0.86, color: "ice", bg: "midnight", contrast: 1.35, glow: 0.45 }),
  ascii("rain", "Code rain", { set: "rain", cell: 14, color: "tint", tint: "#22c55e", contrast: 1.6 }),
  ascii("halftone", "Halftone", { set: "dots", cell: 10, contrast: 1.1 }),
  ascii("pixel-squares", "Pixel squares", { set: "squares", cell: 14, contrast: 1 }),
  ascii("hatch", "Hatching", { set: "hatch", cell: 10, color: "tint", tint: "#ffffff", bg: "black" }),
  ascii("moon", "Moon phases", { set: "moon", cell: 22, contrast: 1.4 }),
  { id: "pixelate", name: "Pixelate", group: "stylize", engine: "pixelate", defaults: { size: 16 },
    params: [{ key: "size", label: "Pixel size", type: "range", min: 2, max: 80, step: 1, unit: "px" }] },
  { id: "dither", name: "Dither", group: "stylize", engine: "dither", defaults: { palette: "1bit", pixel: 3, spread: 1 },
    params: [
      { key: "palette", label: "Palette", type: "select", options: Object.entries(PALETTES).map(([k, v]) => [k, v.label]) },
      { key: "pixel", label: "Pixel size", type: "range", min: 1, max: 12, step: 1, unit: "px" },
      { key: "spread", label: "Spread", type: "range", min: 0, max: 2, step: 0.05, unit: "×" },
    ] },
  { id: "gameboy", name: "Game Boy", group: "stylize", engine: "dither", defaults: { palette: "gameboy", pixel: 5, spread: 0.8 },
    params: [] as ParamDef[] },
  { id: "rgbsplit", name: "RGB split", group: "stylize", engine: "rgbsplit", defaults: { offset: 10, jitter: false },
    params: [
      { key: "offset", label: "Offset", type: "range", min: 0, max: 60, step: 1, unit: "px" },
      { key: "jitter", label: "Jitter", type: "toggle" },
    ] },
  { id: "glitch", name: "Glitch", group: "stylize", engine: "glitch", defaults: { intensity: 0.5, slices: 14, speed: 12 },
    params: [
      { key: "intensity", label: "Intensity", type: "range", min: 0, max: 1, step: 0.01 },
      { key: "slices", label: "Slices", type: "range", min: 2, max: 40, step: 1 },
      { key: "speed", label: "Speed", type: "range", min: 1, max: 30, step: 1, unit: "/s" },
    ] },
  { id: "posterize", name: "Posterize", group: "stylize", engine: "posterize", defaults: { levels: 4 },
    params: [{ key: "levels", label: "Levels", type: "range", min: 2, max: 12, step: 1 }] },
  { id: "duotone", name: "Duotone", group: "stylize", engine: "duotone", defaults: { dark: "#1b0f2e", light: "#ff7a3d", contrast: 1.1 },
    params: [
      { key: "dark", label: "Shadows", type: "color" },
      { key: "light", label: "Highlights", type: "color" },
      { key: "contrast", label: "Contrast", type: "range", min: 0.5, max: 2.5, step: 0.05, unit: "×" },
    ] },

  // Colour & look (CSS filter strings; strength is the amount)
  ...([
    ["bw", "Black & white", (a: number) => `grayscale(${a})`],
    ["sepia", "Sepia", (a: number) => `sepia(${a})`],
    ["warm", "Warm", (a: number) => `sepia(${0.35 * a}) saturate(${1 + 0.4 * a}) hue-rotate(${-12 * a}deg)`],
    ["cool", "Cool", (a: number) => `saturate(${1 - 0.2 * a}) hue-rotate(${18 * a}deg) brightness(${1 + 0.05 * a})`],
    ["vivid", "Vivid", (a: number) => `saturate(${1 + a}) contrast(${1 + 0.15 * a})`],
    ["faded", "Faded film", (a: number) => `contrast(${1 - 0.3 * a}) brightness(${1 + 0.1 * a}) saturate(${1 - 0.3 * a})`],
    ["noir", "Noir", (a: number) => `grayscale(${a}) contrast(${1 + 0.6 * a}) brightness(${1 - 0.1 * a})`],
    ["blur", "Soft blur", (a: number) => `blur(${a * 8}px)`],
    ["dream", "Dream", (a: number) => `blur(${a * 2}px) brightness(${1 + 0.2 * a}) saturate(${1 + 0.3 * a})`],
    ["hue", "Hue shift", (a: number) => `hue-rotate(${a * 180}deg)`],
    ["invert", "Invert", (a: number) => `invert(${a})`],
  ] as const).map(([id, name, css]): EffectDef => ({ id, name, group: "color", engine: "css", css, defaults: {}, params: [] })),
];

// ---------- More presets ----------
const range = (key: string, label: string, min: number, max: number, step: number, unit?: string): ParamDef => ({ key, label, type: "range", min, max, step, unit });
const MIRROR_PARAMS: ParamDef[] = [
  { key: "mode", label: "Mode", type: "select", options: [["left", "Left onto right"], ["right", "Right onto left"], ["top", "Top onto bottom"], ["quad", "Four-way"], ["kaleido", "Kaleidoscope"]] },
  range("segments", "Segments", 3, 16, 1), range("spin", "Spin", -1, 1, 0.01, "/s"),
];
const MAP_PARAMS: ParamDef[] = [
  { key: "map", label: "Look", type: "select", options: Object.entries(GRADIENT_MAPS).map(([k, v]) => [k, v.label]) },
  range("contrast", "Contrast", 0.5, 2.5, 0.05, "×"),
];
const EDGE_PARAMS: ParamDef[] = [
  { key: "mode", label: "Style", type: "select", options: [["sketch", "Pencil sketch"], ["neon", "Neon edges"], ["white", "White lines"]] },
  range("strength", "Strength", 0.5, 4, 0.05, "×"),
];
const WAVE_PARAMS: ParamDef[] = [
  range("amplitude", "Amplitude", 0, 80, 1, "px"), range("frequency", "Frequency", 0.5, 12, 0.5), range("speed", "Speed", 0, 10, 0.1),
  { key: "direction", label: "Direction", type: "select", options: [["horizontal", "Horizontal"], ["vertical", "Vertical"]] },
];

EFFECTS.push(
  // Stylize
  { id: "sketch", name: "Pencil sketch", group: "stylize", engine: "edges", defaults: { mode: "sketch", strength: 1.6 }, params: EDGE_PARAMS },
  { id: "neon-edges", name: "Neon edges", group: "stylize", engine: "edges", defaults: { mode: "neon", strength: 2.2 }, params: EDGE_PARAMS },
  { id: "emboss", name: "Emboss", group: "stylize", engine: "emboss", defaults: { strength: 1.4 }, params: [range("strength", "Strength", 0.5, 4, 0.05, "×")] },
  { id: "thermal", name: "Thermal", group: "stylize", engine: "gradientmap", defaults: { map: "thermal", contrast: 1.2 }, params: MAP_PARAMS },
  { id: "night-vision", name: "Night vision", group: "stylize", engine: "gradientmap", defaults: { map: "night", contrast: 1.4 }, params: MAP_PARAMS },
  { id: "infrared", name: "Infrared", group: "stylize", engine: "gradientmap", defaults: { map: "infrared", contrast: 1.1 }, params: MAP_PARAMS },
  { id: "threshold", name: "Threshold", group: "stylize", engine: "threshold", defaults: { level: 0.5, dark: "#000000", light: "#ffffff" },
    params: [range("level", "Level", 0.05, 0.95, 0.01), { key: "dark", label: "Dark", type: "color" }, { key: "light", label: "Light", type: "color" }] },
  { id: "solarize", name: "Solarize", group: "stylize", engine: "solarize", defaults: { threshold: 0.5 }, params: [range("threshold", "Threshold", 0, 1, 0.01)] },
  { id: "glow", name: "Glow", group: "stylize", engine: "glow", defaults: { strength: 0.7, radius: 18 },
    params: [range("strength", "Strength", 0, 1.5, 0.01), range("radius", "Radius", 2, 60, 1, "px")] },

  // Retro
  { id: "vhs", name: "VHS", group: "retro", engine: "vhs", defaults: { intensity: 0.6 }, params: [range("intensity", "Intensity", 0, 1, 0.01)] },
  { id: "crt", name: "CRT", group: "retro", engine: "crt", defaults: { pitch: 4, glow: 0.35 },
    params: [range("pitch", "Phosphor size", 3, 12, 1, "px"), range("glow", "Glow", 0, 1, 0.01)] },

  // Distort & camera
  { id: "shake", name: "Camera shake", group: "distort", engine: "shake", defaults: { amount: 0.4, speed: 1 },
    params: [range("amount", "Amount", 0, 1.5, 0.01), range("speed", "Speed", 0.2, 4, 0.05, "×")] },
  { id: "mirror", name: "Mirror", group: "distort", engine: "mirror", defaults: { mode: "left", segments: 6, spin: 0.15 }, params: MIRROR_PARAMS },
  { id: "kaleidoscope", name: "Kaleidoscope", group: "distort", engine: "mirror", defaults: { mode: "kaleido", segments: 8, spin: 0.15 }, params: MIRROR_PARAMS },
  { id: "wave", name: "Wave", group: "distort", engine: "wave", defaults: { amplitude: 18, frequency: 3, speed: 2, direction: "horizontal" }, params: WAVE_PARAMS },
  { id: "ripple", name: "Ripple", group: "distort", engine: "wave", defaults: { amplitude: 8, frequency: 9, speed: 5, direction: "vertical" }, params: WAVE_PARAMS },
  { id: "zoom-blur", name: "Zoom blur", group: "distort", engine: "zoomblur", defaults: { amount: 0.5 }, params: [range("amount", "Amount", 0, 1, 0.01)] },
  { id: "tilt-shift", name: "Tilt-shift", group: "distort", engine: "tiltshift", defaults: { focus: 0.55, band: 0.16, blur: 12, saturate: true },
    params: [range("focus", "Focus line", 0, 1, 0.01), range("band", "Sharp band", 0.02, 0.5, 0.01), range("blur", "Blur", 2, 40, 1, "px"), { key: "saturate", label: "Miniature colour", type: "toggle" }] },
);

// Older stylize presets that read better under Retro.
for (const id of ["dither", "gameboy", "rgbsplit", "glitch"]) EFFECTS.find((e) => e.id === id)!.group = "retro";

// The Game Boy preset exposes the same controls as Dither.
EFFECTS.find((e) => e.id === "gameboy")!.params = EFFECTS.find((e) => e.id === "dither")!.params;

export const effectDef = (id: string) => EFFECTS.find((e) => e.id === id);

let uidSeq = 1;
export function makeClipEffect(id: string, amount = 1): ClipEffect | null {
  const def = effectDef(id);
  if (!def) return null;
  return { uid: "fx" + Date.now().toString(36) + uidSeq++, id, enabled: true, amount, params: { ...def.defaults } };
}
