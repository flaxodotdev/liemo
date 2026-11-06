"use client";

// Pointer-driven drag from the library to the timeline, preview or a clip.
// Native HTML drag-and-drop is only used for files coming from the desktop.
import type * as React from "react";
import { create } from "zustand";
import { toast } from "sonner";
import { ASSETS } from "./assets";
import { DEFAULT_TRACK, accepts, trackOf, useEditor } from "./store";
import { snapFrame } from "./time";

export type DragPayload = { kind: "asset" | "effect" | "transition"; id: string; label: string; thumb?: string };
export type DropTarget =
  | { kind: "lane"; track: string; time: number; ok: boolean }
  | { kind: "clip"; clipId: string }
  | { kind: "stage" }
  | null;

interface DragState { payload: DragPayload | null; x: number; y: number; target: DropTarget }
export const useDrag = create<DragState>(() => ({ payload: null, x: 0, y: 0, target: null }));

function hitTest(x: number, y: number, p: DragPayload): DropTarget {
  const el = document.elementFromPoint(x, y) as HTMLElement | null;
  if (!el) return null;
  if (p.kind !== "asset") {
    const clip = el.closest<HTMLElement>("[data-clip]");
    return clip ? { kind: "clip", clipId: clip.dataset.clip! } : null;
  }
  const lane = el.closest<HTMLElement>("[data-track]");
  if (lane) {
    const s = useEditor.getState(), track = lane.dataset.track!;
    let time = snapFrame(Math.max(0, (x - lane.getBoundingClientRect().left) / s.zoom));
    if (s.editMode === "insert") {
      // Same rule as the drop itself: inside a clip lands before it (first half) or after it.
      const host = s.clips.find((k) => k.track === track && time > k.start && time < k.start + k.dur);
      if (host) time = time - host.start < host.dur / 2 ? host.start : host.start + host.dur;
    }
    return { kind: "lane", track, time, ok: accepts(s, track, ASSETS.get(p.id)!.type) && !trackOf(s, track).locked };
  }
  if (el.closest("[data-drop-stage]")) return { kind: "stage" };
  return null;
}

function drop(p: DragPayload, t: DropTarget) {
  const s = useEditor.getState();
  if (!t) return;
  if (t.kind === "clip") {
    if (p.kind === "effect") s.applyEffect(p.id, t.clipId);
    else if (p.kind === "transition") s.applyTransition(p.id, t.clipId);
    return;
  }
  if (p.kind !== "asset") return;
  if (t.kind === "stage") { s.addClip(p.id); return; }
  const type = ASSETS.get(p.id)!.type, track = trackOf(s, t.track);
  if (track.locked) { toast(`${track.name} is locked. Unlock it to drop clips there.`); return; }
  if (!t.ok) toast(`${track.name} doesn't take ${type} clips, so it went on ${trackOf(s, DEFAULT_TRACK[type]).name}.`);
  s.addClip(p.id, t.ok ? t.track : undefined, t.time);
}

/** Call from onPointerDown. A press without movement stays a normal click. */
export function startLibraryDrag(e: React.PointerEvent, payload: DragPayload) {
  if (e.button !== 0 || e.pointerType === "touch") return;
  const x0 = e.clientX, y0 = e.clientY;
  let active = false;
  const move = (ev: PointerEvent) => {
    if (!active) {
      if (Math.hypot(ev.clientX - x0, ev.clientY - y0) < 5) return;
      active = true;
      document.documentElement.style.cursor = "grabbing";
    }
    useDrag.setState({ payload, x: ev.clientX, y: ev.clientY, target: hitTest(ev.clientX, ev.clientY, payload) });
  };
  const end = (ev: PointerEvent) => {
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", end);
    window.removeEventListener("pointercancel", end);
    window.removeEventListener("keydown", esc);
    document.documentElement.style.cursor = "";
    if (active && ev.type === "pointerup") {
      drop(payload, hitTest(ev.clientX, ev.clientY, payload));
      // Swallow the click that follows a drag so the card's own click handler doesn't fire.
      const swallow = (c: MouseEvent) => { c.stopPropagation(); c.preventDefault(); };
      window.addEventListener("click", swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener("click", swallow, true), 0);
    }
    useDrag.setState({ payload: null, target: null });
  };
  const esc = (k: KeyboardEvent) => {
    if (k.key !== "Escape") return;
    active = false;
    end(new PointerEvent("pointercancel"));
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", end);
  window.addEventListener("pointercancel", end);
  window.addEventListener("keydown", esc);
}
