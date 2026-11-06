"use client";

import { toast } from "sonner";
import { ASSETS, FONTS, SCENES, fontStack, type Asset } from "./assets";
import { H, W, rng } from "./time";
import { effectsOnObject, useEditor, trackOf, type Clip } from "./store";
import { runEffects } from "./fx/engines";
import { effectDef, makeClipEffect } from "./fx/registry";
import { transitionDef, type TransitionDef } from "./fx/transitions";
import { putMedia, type MediaRecord } from "@/lib/projects/db";

type Ctx = CanvasRenderingContext2D;

// ---------- Helpers ----------
export function drawCover(c: Ctx, src: CanvasImageSource, w: number, h: number) {
  const s = src as HTMLVideoElement & HTMLImageElement & HTMLCanvasElement;
  const sw = s.videoWidth || s.naturalWidth || s.width, sh = s.videoHeight || s.naturalHeight || s.height;
  if (!sw || !sh) return;
  const k = Math.max(w / sw, h / sh);
  c.drawImage(src, (w - sw * k) / 2, (h - sh * k) / 2, sw * k, sh * k);
}
export function fadeAt(c: Pick<Clip, "start" | "dur" | "fadeIn" | "fadeOut">, t: number) {
  const lt = t - c.start;
  let f = 1;
  if (c.fadeIn > 0) f = Math.min(f, lt / c.fadeIn);
  if (c.fadeOut > 0) f = Math.min(f, (c.dur - lt) / c.fadeOut);
  return Math.max(0, Math.min(1, f));
}
const ease = (p: number) => 1 - Math.pow(1 - p, 3);

function bitmapOf(a: Asset) {
  if (!a.bitmap && a.draw) {
    const cv = document.createElement("canvas"); cv.width = W; cv.height = H;
    a.draw(cv.getContext("2d")!, 0); a.bitmap = cv;
  }
  return a.bitmap;
}

// ---------- Text ----------
export function drawText(c: Ctx, clip: Pick<Clip, "text" | "opacity">, lt: number, alpha = 1) {
  const T = clip.text!;
  const f = FONTS.find((x) => x.family === T.font) ?? FONTS[0];
  const e = ease(Math.min(1, lt / 0.6));
  let ox = 0, oy = 0, a = 1;
  if (T.anim === "fade") a = e;
  else if (T.anim === "rise") { a = e; oy = (1 - e) * 40; }
  else if (T.anim === "slide") { a = e; ox = -(1 - e) * 80; }
  let lines = T.content.split("\n");
  if (T.anim === "type") {
    let n = Math.floor(lt / 0.045);
    lines = lines.map((l) => { const s = l.slice(0, Math.max(0, n)); n -= l.length; return s; });
  }
  const stack = fontStack(T.font);
  const sizes = lines.map((_, i) => (i > 0 && T.box && lines.length > 1 ? T.size * 0.6 : T.size));
  const lh = sizes.map((s) => s * (T.font === "Bebas Neue" ? 0.95 : 1.12));
  const total = lh.reduce((x, y) => x + y, 0);
  const setFont = (i: number) => {
    c.font = `${i ? 500 : T.weight} ${sizes[i]}px ${stack}`;
    if ("letterSpacing" in c) (c as Ctx & { letterSpacing: string }).letterSpacing = `${f.track * sizes[i]}px`;
  };
  c.textBaseline = "middle"; c.textAlign = T.align;
  const baseX = T.align === "left" ? 120 : T.align === "right" ? W - 120 : W / 2;
  let y = H / 2 + T.y - total / 2 + oy;
  const widths = lines.map((l, i) => { setFont(i); return c.measureText(l).width; });
  if (T.box) {
    const bw = Math.max(...widths) + 48;
    const bx = T.align === "left" ? baseX - 24 : T.align === "right" ? baseX - bw + 24 : baseX - bw / 2;
    c.globalAlpha = alpha * a;
    c.fillStyle = "rgba(0,0,0,.55)"; c.beginPath(); c.roundRect(bx + ox, y - 14, bw, total + 28, 12); c.fill();
    if (T.align === "left") { c.fillStyle = T.color === "#ffffff" ? "#ff7a3d" : T.color; c.fillRect(bx + ox, y - 14, 6, total + 28); }
  }
  lines.forEach((l, i) => {
    setFont(i);
    c.fillStyle = T.color; c.globalAlpha = (i ? 0.75 : 1) * a * alpha;
    c.fillText(l, baseX + ox, y + lh[i] / 2); y += lh[i];
  });
  if ("letterSpacing" in c) (c as Ctx & { letterSpacing: string }).letterSpacing = "0px";
}

// ---------- Media elements (one per clip so clips can sit at different source times) ----------
const mediaEls = new Map<string, HTMLVideoElement | HTMLAudioElement>();
export function mediaElFor(clip: Clip) {
  let el = mediaEls.get(clip.id);
  if (!el) {
    const a = ASSETS.get(clip.asset);
    if (!a?.url) return null;
    el = document.createElement(a.type === "video" ? "video" : "audio");
    el.src = a.url; el.preload = "auto";
    if (el instanceof HTMLVideoElement) el.playsInline = true;
    el.addEventListener("seeked", () => { if (!useEditor.getState().playing) requestRender(); });
    el.addEventListener("loadeddata", requestRender);
    mediaEls.set(clip.id, el);
  }
  return el;
}

export function syncMedia() {
  const s = useEditor.getState(), t = s.time, live = new Set(s.clips.map((c) => c.id));
  mediaEls.forEach((el, id) => { if (!live.has(id)) { el.pause(); mediaEls.delete(id); } });
  s.clips.forEach((c) => {
    if (c.type !== "video" && c.type !== "audio") return;
    const el = mediaElFor(c); if (!el) return;
    const active = t >= c.start && t < c.start + c.dur, target = t - c.start + c.in;
    el.muted = trackOf(s, c.track).muted;
    el.volume = Math.max(0, Math.min(1, c.volume * fadeAt(c, t)));
    if (!active) {
      if (!el.paused) el.pause();
      // Park upcoming clips on their first frame so transitions into them show the right picture.
      if (t < c.start && c.start - t < 2 && Math.abs(el.currentTime - c.in) > 0.05) el.currentTime = c.in;
      return;
    }
    if (s.playing) {
      if (el.paused) el.play().catch(() => {});
      if (Math.abs(el.currentTime - target) > 0.3) el.currentTime = target;
    } else {
      if (!el.paused) el.pause();
      if (Math.abs(el.currentTime - target) > 0.04) el.currentTime = target;
    }
  });
}

// ---------- Compositor ----------
let stageCtx: Ctx | null = null;
export function attachStage(canvas: HTMLCanvasElement | null) {
  stageCtx = canvas ? canvas.getContext("2d") : null;
  if (canvas) { canvas.width = W; canvas.height = H; requestRender(); }
}

/** Draw a clip's content with its transform, on whatever context it's given. */
function drawContent(c: Ctx, clip: Clip, a: Asset, t: number, alpha: number) {
  const lt = t - clip.start, st = lt + clip.in;
  c.translate(W / 2 + clip.x, H / 2 + clip.y); c.rotate((clip.rot * Math.PI) / 180); c.scale(clip.scale, clip.scale); c.translate(-W / 2, -H / 2);
  if (a.type === "scene" || a.type === "overlay") a.draw!(c, st);
  else if (a.type === "image") c.drawImage(bitmapOf(a)!, 0, 0, W, H);
  else if (a.type === "photo" && a.bitmap) drawCover(c, a.bitmap, W, H);
  else if (a.type === "video") { const el = mediaElFor(clip) as HTMLVideoElement | null; if (el && el.readyState >= 2) drawCover(c, el, W, H); }
  else if (a.type === "text") drawText(c, clip, lt, alpha);
}

let layer: HTMLCanvasElement | null = null;
/**
 * Draw one clip onto `c`. The target can be any 16:9 size; everything is laid out in 1280×720
 * units and scaled, so text and graphics stay sharp at 1080p and 4K.
 */
function drawClip(c: Ctx, clip: Clip, t: number) {
  const a = ASSETS.get(clip.asset); if (!a) return;
  const alpha = clip.opacity * fadeAt(clip, t);
  const cw = c.canvas.width, ch = c.canvas.height, s = cw / W;

  if (clip.effects?.some((e) => e.enabled)) {
    // Render the clip alone, run its effect stack, then composite the result like any other clip.
    if (!layer || layer.width !== cw || layer.height !== ch) layer = Object.assign(document.createElement("canvas"), { width: cw, height: ch });
    const l = layer.getContext("2d", { willReadFrequently: true })!;
    l.setTransform(1, 0, 0, 1, 0, 0); l.globalAlpha = 1; l.globalCompositeOperation = "source-over";
    l.clearRect(0, 0, cw, ch);
    l.save(); l.setTransform(s, 0, 0, s, 0, 0); drawContent(l, clip, a, t, 1); l.restore();
    const out = runEffects(layer, clip.effects, t - clip.start, { object: effectsOnObject(clip) });
    c.save();
    c.setTransform(1, 0, 0, 1, 0, 0);
    c.globalAlpha = alpha;
    if (a.blend) c.globalCompositeOperation = a.blend;
    c.drawImage(out, 0, 0);
    c.restore();
    return;
  }

  c.save();
  c.setTransform(s, 0, 0, s, 0, 0);
  c.globalAlpha = alpha;
  if (a.blend) c.globalCompositeOperation = a.blend;
  drawContent(c, clip, a, t, alpha);
  c.restore();
}

export function render() {
  if (stageCtx) renderFrame(stageCtx, useEditor.getState().time);
}

/** Composite every visible clip at time `t` onto a 1280×720 context. */
export function renderFrame(c: Ctx, t: number) {
  const s = useEditor.getState();
  c.save(); c.setTransform(1, 0, 0, 1, 0, 0); c.globalAlpha = 1; c.filter = "none"; c.fillStyle = "#000"; c.fillRect(0, 0, c.canvas.width, c.canvas.height); c.restore();
  for (const tid of ["V1", "V2", "T1"]) {
    if (trackOf(s, tid).hidden) continue;
    const track = s.clips.filter((k) => k.track === tid).sort((x, y) => x.start - y.start);
    const handled = new Set<string>();
    for (const w of transitionWindows(track)) {
      if (t < w.from || t >= w.to) continue;
      drawTransition(c, w, (t - w.from) / (w.to - w.from), t);
      if (w.a) handled.add(w.a.id);
      handled.add(w.b.id);
    }
    track.filter((k) => !handled.has(k.id) && t >= k.start && t < k.start + k.dur).forEach((k) => drawClip(c, k, t));
  }
}

// ---------- Transitions ----------
interface TransitionWindow { a: Clip | null; b: Clip; def: TransitionDef; from: number; to: number }

/**
 * Where each transition plays. Between two touching clips it straddles the cut; with nothing
 * right before the clip it plays over the clip's first moments, from whatever is below.
 */
export function transitionWindows(track: Clip[]): TransitionWindow[] {
  const out: TransitionWindow[] = [];
  track.forEach((b, i) => {
    if (!b.transition) return;
    const def = transitionDef(b.transition.id); if (!def) return;
    const prev = i > 0 ? track[i - 1] : null;
    const a = prev && Math.abs(prev.start + prev.dur - b.start) < 1.5 / 30 ? prev : null;
    const d = Math.max(1 / 30, Math.min(b.transition.dur, b.dur, a ? a.dur : Infinity));
    const from = a ? b.start - d / 2 : b.start;
    out.push({ a, b, def, from, to: from + d });
  });
  return out;
}

const frames = new Map<string, HTMLCanvasElement>();
/** Render one clip alone, at time `t` held inside the clip, onto a transparent frame. */
function clipFrame(name: string, clip: Clip, t: number, w: number, h: number) {
  let f = frames.get(name);
  if (!f || f.width !== w || f.height !== h) { f = document.createElement("canvas"); f.width = w; f.height = h; frames.set(name, f); }
  const g = f.getContext("2d")!;
  g.setTransform(1, 0, 0, 1, 0, 0); g.globalAlpha = 1; g.globalCompositeOperation = "source-over"; g.clearRect(0, 0, w, h);
  // Outside its own range a clip holds its first or last frame. drawClip scales to the frame size itself.
  drawClip(g, clip, Math.min(Math.max(t, clip.start), clip.start + clip.dur - 1e-3));
  return f;
}

function drawTransition(c: Ctx, w: TransitionWindow, p: number, t: number) {
  const cw = c.canvas.width, ch = c.canvas.height;
  const a = w.a ? clipFrame("tr-a", w.a, t, cw, ch) : null;
  const b = clipFrame("tr-b", w.b, t, cw, ch);
  c.save(); c.setTransform(1, 0, 0, 1, 0, 0);
  w.def.draw(c, a, b, Math.max(0, Math.min(1, p)), t);
  c.restore();
}

/** Thumbnail for a transition: two sample scenes, caught halfway through. */
export function transitionThumb(id: string) {
  const key = "tr:" + id;
  if (!thumbCache.has(key)) {
    const def = transitionDef(id)!;
    const mk = (scene: number) => {
      const cv = document.createElement("canvas"); cv.width = 384; cv.height = 216;
      const g = cv.getContext("2d")!; g.scale(384 / W, 216 / H); SCENES[scene].draw!(g, 1.4); return cv;
    };
    const out = document.createElement("canvas"); out.width = 384; out.height = 216;
    const g = out.getContext("2d")!; g.fillStyle = "#000"; g.fillRect(0, 0, 384, 216);
    def.draw(g, mk(0), mk(2), id === "pixelate" ? 0.42 : id === "dip-black" || id === "dip-white" ? 0.3 : 0.5, 1.2);
    thumbCache.set(key, out.toDataURL("image/jpeg", 0.85));
  }
  return thumbCache.get(key)!;
}
let renderQueued = false;
export function requestRender() {
  if (renderQueued) return;
  renderQueued = true;
  requestAnimationFrame(() => { renderQueued = false; render(); });
}

// ---------- Thumbnails ----------
const thumbCache = new Map<string, string>();
function thumbOf(draw: (c: Ctx) => void, w = 320) {
  const cv = document.createElement("canvas"); cv.width = W; cv.height = H;
  draw(cv.getContext("2d")!);
  const out = document.createElement("canvas"); out.width = w; out.height = Math.round((w * 9) / 16);
  out.getContext("2d")!.drawImage(cv, 0, 0, out.width, out.height);
  return out.toDataURL("image/jpeg", 0.8);
}
export function assetThumb(id: string, fresh = false): string | undefined {
  const a = ASSETS.get(id); if (!a) return;
  if (a.thumb) return a.thumb;
  if (!fresh && thumbCache.has(id)) return thumbCache.get(id);
  let url: string | undefined;
  if (a.type === "scene" || a.type === "image") url = thumbOf((c) => a.draw!(c, 1.2));
  else if (a.type === "overlay") url = thumbOf((c) => { SCENES[3].draw!(c, 1.2); c.globalCompositeOperation = a.blend!; a.draw!(c, 1.2); });
  else if (a.type === "text") url = thumbOf((c) => {
    SCENES[0].draw!(c, 1.5); c.fillStyle = "rgba(0,0,0,.35)"; c.fillRect(0, 0, W, H);
    drawText(c, { text: { ...a.text!, anim: "none" }, opacity: 1 }, 2);
  });
  if (url) thumbCache.set(id, url);
  return url;
}
export function effectThumb(effectId: string) {
  const key = "fx:" + effectId;
  if (!thumbCache.has(key)) {
    const fx = makeClipEffect(effectId, effectId === "blur" ? 0.25 : 1);
    const cv = document.createElement("canvas"); cv.width = 384; cv.height = 216;
    const c = cv.getContext("2d", { willReadFrequently: true })!;
    c.scale(384 / W, 216 / H); SCENES[0].draw!(c, 1.4);
    // Thumbnails are small, so enlarge cells and pixels to keep the look readable.
    if (fx) for (const pd of effectDef(effectId)?.params ?? []) {
      if (pd.type === "range" && pd.unit === "px" && typeof fx.params[pd.key] === "number") fx.params[pd.key] = (fx.params[pd.key] as number) * 2.2;
    }
    const out = fx ? runEffects(cv, [fx], 1.4) : cv;
    thumbCache.set(key, out.toDataURL("image/jpeg", 0.85));
  }
  return thumbCache.get(key)!;
}
const waveCache = new Map<number, string>();
export function waveURL(seed: number) {
  if (!waveCache.has(seed)) {
    const cv = document.createElement("canvas"); cv.width = 240; cv.height = 40;
    const c = cv.getContext("2d")!, r = rng(seed);
    c.fillStyle = "rgba(120,230,160,.9)";
    for (let x = 0; x < 240; x += 3) { const h = 4 + Math.abs(Math.sin(x * 0.09 + seed)) * 18 + r() * 14; c.fillRect(x, 20 - h / 2, 2, h); }
    waveCache.set(seed, cv.toDataURL());
  }
  return waveCache.get(seed)!;
}

// ---------- Media pool: importing and restoring files ----------
type MediaMeta = Omit<MediaRecord, "blob">;

/** Forget per-clip media elements, e.g. when switching projects (clip ids repeat across projects). */
export function resetMedia() {
  mediaEls.forEach((el) => { el.pause(); el.removeAttribute("src"); el.load(); });
  mediaEls.clear();
}

/**
 * Register a file as a library asset. `fresh` means it was just imported: measure it, make a
 * thumbnail and save it to project storage. Otherwise it is being restored from storage.
 */
function registerMedia(meta: MediaMeta, blob: Blob, fresh: boolean, onReady?: (id: string) => void) {
  const url = URL.createObjectURL(blob);
  const asset: Asset = { id: meta.id, type: meta.type, name: meta.name, sub: meta.sub, url, dur: meta.dur, thumb: meta.thumb };
  ASSETS.set(meta.id, asset);
  const bump = () => useEditor.setState((s) => ({ pool: [...s.pool] }));
  const save = () => {
    if (!fresh) return;
    putMedia({ id: asset.id, type: meta.type, name: asset.name, sub: asset.sub, dur: asset.dur, thumb: asset.thumb, blob })
      .catch(() => toast(`${asset.name} is in this session but couldn't be saved to the project. Storage may be full.`));
  };

  if (meta.type === "photo") {
    const img = new Image();
    img.onload = () => {
      asset.bitmap = img;
      if (!asset.thumb) {
        const cv = document.createElement("canvas"); cv.width = 320; cv.height = 180;
        drawCover(cv.getContext("2d")!, img, 320, 180); asset.thumb = cv.toDataURL("image/jpeg", 0.8);
      }
      save(); bump(); onReady?.(asset.id); requestRender();
    };
    img.onerror = () => toast(`${asset.name} couldn't be read as an image`);
    img.src = url;
    return;
  }
  if (!fresh) { bump(); return; }
  const el = document.createElement(meta.type); el.preload = "metadata"; el.muted = true; el.src = url;
  el.onerror = () => toast(`${asset.name} couldn't be read. Try an MP4, WebM, MP3 or WAV file.`);
  el.onloadedmetadata = () => {
    asset.dur = isFinite(el.duration) && el.duration > 0 ? el.duration : 5;
    bump(); onReady?.(asset.id);
    if (el instanceof HTMLVideoElement) {
      el.onseeked = () => {
        const cv = document.createElement("canvas"); cv.width = 320; cv.height = 180;
        drawCover(cv.getContext("2d")!, el, 320, 180);
        asset.thumb = cv.toDataURL("image/jpeg", 0.8); el.onseeked = null; bump(); save();
      };
      el.currentTime = Math.min(1, asset.dur / 3);
    } else save();
  };
}

/** Put a stored media file back into the library when a project opens. */
export function restoreMedia(rec: MediaRecord) {
  if (ASSETS.has(rec.id)) return;
  const { blob, ...meta } = rec;
  registerMedia(meta, blob, false);
}

/**
 * Add files to the media pool. With `place`, each file also lands on the timeline once its
 * duration is known: the first at `place.start`, the rest one after another.
 */
export function importFiles(files: FileList | File[], place?: { start: number; track?: string }) {
  let count = 0;
  let cursor = place?.start ?? 0;
  const placeOnTimeline = (id: string) => {
    if (!place) return;
    const c = useEditor.getState().addClip(id, place.track, cursor);
    if (c) cursor = c.start + c.dur;
  };
  [...files].forEach((file) => {
    const type = file.type.startsWith("video") ? "video" : file.type.startsWith("audio") ? "audio" : file.type.startsWith("image") ? "photo" : null;
    if (!type) { toast(`${file.name} isn't a video, audio or image file`); return; }
    count++;
    const id = "u" + Date.now().toString(36) + Math.random().toString(36).slice(2, 6);
    useEditor.setState((s) => ({ pool: [...s.pool, id] }));
    registerMedia({ id, type, name: file.name.replace(/\.[^.]+$/, ""), sub: `${(file.size / 1048576).toFixed(1)} MB · imported`, dur: 5 }, file, true, placeOnTimeline);
  });
  if (count) {
    const where = place ? "the timeline" : "the media pool";
    toast(count === 1 ? `Added 1 file to ${where}` : `Added ${count} files to ${where}`);
  }
}

/** A small JPEG of the edit a quarter of the way in, for project cards. */
export function stageSnapshot(w = 384) {
  const s = useEditor.getState();
  const end = s.clips.reduce((m, c) => Math.max(m, c.start + c.dur), 0);
  if (!end) return undefined;
  const src = document.createElement("canvas"); src.width = W; src.height = H;
  renderFrame(src.getContext("2d")!, Math.min(end - 1 / 30, Math.max(1, end * 0.25)));
  const out = document.createElement("canvas"); out.width = w; out.height = Math.round((w * 9) / 16);
  out.getContext("2d")!.drawImage(src, 0, 0, out.width, out.height);
  return out.toDataURL("image/jpeg", 0.75);
}

/**
 * For export: put every video clip that's visible at `t` on exactly the right frame and wait
 * until that frame can be drawn. Preview playback is approximate; export must not be.
 */
export async function seekVideosForExport(t: number) {
  const s = useEditor.getState();
  const waits: Promise<void>[] = [];
  for (const c of s.clips) {
    if (c.type !== "video" || trackOf(s, c.track).hidden) continue;
    const near = t >= c.start - 1 && t < c.start + c.dur; // includes clips about to transition in
    if (!near) continue;
    const el = mediaElFor(c) as HTMLVideoElement | null; if (!el) continue;
    el.pause(); el.muted = true;
    const target = Math.min(Math.max(t, c.start), c.start + c.dur - 1e-3) - c.start + c.in;
    if (Math.abs(el.currentTime - target) < 1e-3 && el.readyState >= 2) continue;
    waits.push(new Promise<void>((resolve) => {
      const done = () => { el.removeEventListener("seeked", done); clearTimeout(timer); resolve(); };
      const timer = setTimeout(done, 3000);
      el.addEventListener("seeked", done);
      el.currentTime = target;
    }));
  }
  await Promise.all(waits);
}

/** The file behind an imported asset, for decoding its audio during export. */
export async function assetBlob(id: string): Promise<Blob | null> {
  const a = ASSETS.get(id);
  if (!a?.url) return null;
  try { return await (await fetch(a.url)).blob(); } catch { return null; }
}
