"use client";

// Where each clip sits on the 1280×720 canvas, so the preview can be clicked, dragged,
// scaled and rotated directly. Mirrors the transforms in render.ts drawClip/drawText.
import { FONTS, fontStack } from "./assets";
import { H, W } from "./time";
import { trackOf, useEditor, type Clip } from "./store";

export interface Rect { x: number; y: number; w: number; h: number }
/** A clip's box on the canvas: centre, size after scaling, rotation in degrees. */
export interface Box { cx: number; cy: number; w: number; h: number; rot: number }

let measure: CanvasRenderingContext2D | null = null;
function measurer() {
  if (!measure) measure = document.createElement("canvas").getContext("2d")!;
  return measure;
}

/** The untransformed rectangle a text clip draws into (matches drawText's layout). */
export function textRect(clip: Clip): Rect {
  const T = clip.text!;
  const f = FONTS.find((x) => x.family === T.font) ?? FONTS[0];
  const c = measurer(), stack = fontStack(T.font);
  const lines = T.content.split("\n");
  const sizes = lines.map((_, i) => (i > 0 && T.box && lines.length > 1 ? T.size * 0.6 : T.size));
  const total = sizes.reduce((sum, s) => sum + s * (T.font === "Bebas Neue" ? 0.95 : 1.12), 0);
  const widths = lines.map((l, i) => {
    c.font = `${i ? 500 : T.weight} ${sizes[i]}px ${stack}`;
    if ("letterSpacing" in c) (c as CanvasRenderingContext2D & { letterSpacing: string }).letterSpacing = `${f.track * sizes[i]}px`;
    return c.measureText(l || " ").width;
  });
  const baseX = T.align === "left" ? 120 : T.align === "right" ? W - 120 : W / 2;
  const top = H / 2 + T.y - total / 2;
  const maxW = Math.max(...widths, 8);
  if (T.box) {
    const bw = maxW + 48, bx = T.align === "left" ? baseX - 24 : T.align === "right" ? baseX - bw + 24 : baseX - bw / 2;
    return { x: bx, y: top - 14, w: bw, h: total + 28 };
  }
  const x = T.align === "left" ? baseX : T.align === "right" ? baseX - maxW : baseX - maxW / 2;
  return { x: x - 8, y: top - 4, w: maxW + 16, h: total + 8 };
}

export const localRect = (clip: Clip): Rect => (clip.text ? textRect(clip) : { x: 0, y: 0, w: W, h: H });

/** Map a local point through the clip's transform: translate to centre + offset, rotate, scale. */
export function toCanvas(clip: Clip, px: number, py: number) {
  const a = (clip.rot * Math.PI) / 180, s = clip.scale, dx = (px - W / 2) * s, dy = (py - H / 2) * s;
  return { x: W / 2 + clip.x + dx * Math.cos(a) - dy * Math.sin(a), y: H / 2 + clip.y + dx * Math.sin(a) + dy * Math.cos(a) };
}
export function toLocal(clip: Clip, px: number, py: number) {
  const a = (-clip.rot * Math.PI) / 180, s = clip.scale || 1, dx = px - W / 2 - clip.x, dy = py - H / 2 - clip.y;
  return { x: W / 2 + (dx * Math.cos(a) - dy * Math.sin(a)) / s, y: H / 2 + (dx * Math.sin(a) + dy * Math.cos(a)) / s };
}

export function clipBox(clip: Clip): Box {
  const r = localRect(clip), c = toCanvas(clip, r.x + r.w / 2, r.y + r.h / 2);
  return { cx: c.x, cy: c.y, w: r.w * clip.scale, h: r.h * clip.scale, rot: clip.rot };
}

/** Visual clips on screen at `t`, front-most first: titles, then picture-in-picture, then footage, overlays last. */
export function visibleClips(t: number): Clip[] {
  const s = useEditor.getState();
  const at = (track: string) => s.clips
    .filter((c) => c.track === track && !trackOf(s, track).hidden && t >= c.start && t < c.start + c.dur)
    .sort((a, b) => b.start - a.start);
  const v2 = at("V2");
  return [...at("T1"), ...v2.filter((c) => c.type !== "overlay"), ...at("V1"), ...v2.filter((c) => c.type === "overlay")];
}

/** The clip under a canvas point, front-most first. */
export function hitTest(px: number, py: number, t: number): Clip | null {
  for (const c of visibleClips(t)) {
    const r = localRect(c), p = toLocal(c, px, py);
    if (p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h) return c;
  }
  return null;
}
