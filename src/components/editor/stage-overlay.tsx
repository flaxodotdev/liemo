"use client";

// Direct manipulation on the preview: click to select, drag to move, corners to scale,
// the round handle to rotate. Coordinates are in canvas pixels (1280×720) and scaled by `k`.
import * as React from "react";
import { toast } from "sonner";
import { cn } from "@/lib/utils";
import { H, W } from "@/lib/editor/time";
import { trackOf, useEditor, type Clip } from "@/lib/editor/store";
import { clipBox, hitTest, localRect, toCanvas, toLocal, type Box } from "@/lib/editor/geometry";

type Guide = { axis: "x" | "y"; at: number };
const SNAP_PX = 7; // screen pixels
const SAFE = 0.05;

const boxStyle = (b: Box, k: number): React.CSSProperties => ({
  left: b.cx * k, top: b.cy * k, width: Math.max(2, b.w * k), height: Math.max(2, b.h * k),
  transform: `translate(-50%, -50%) rotate(${b.rot}deg)`,
});

export function StageOverlay({ width }: { width: number }) {
  const k = width / W;
  const ref = React.useRef<HTMLDivElement>(null);
  const time = useEditor((s) => s.time);
  const clip = useEditor((s) => s.clips.find((c) => c.id === s.selected));
  const trackHidden = useEditor((s) => (clip ? trackOf(s, clip.track).hidden : false));
  const [hover, setHover] = React.useState<Clip | null>(null);
  const [guides, setGuides] = React.useState<Guide[]>([]);
  const [readout, setReadout] = React.useState<string | null>(null);
  const dragging = React.useRef(false);

  const visual = clip && clip.type !== "music" && clip.type !== "audio";
  const onScreen = visual && !trackHidden && time >= clip.start && time < clip.start + clip.dur;
  const box = onScreen ? clipBox(clip) : null;

  const pointFrom = (e: { clientX: number; clientY: number }) => {
    const r = ref.current!.getBoundingClientRect();
    return { x: ((e.clientX - r.left) / r.width) * W, y: ((e.clientY - r.top) / r.height) * H };
  };

  /** Shared drag plumbing: one history entry per gesture, cleanup on release. */
  const drag = (e: React.PointerEvent, onMove: (p: { x: number; y: number }, ev: PointerEvent) => void) => {
    e.preventDefault(); e.stopPropagation();
    const start = pointFrom(e);
    let begun = false;
    dragging.current = true;
    const move = (ev: PointerEvent) => {
      const p = pointFrom(ev);
      if (!begun) {
        if (Math.hypot((p.x - start.x) * k, (p.y - start.y) * k) < 2) return;
        begun = true; useEditor.getState().beginEdit();
      }
      onMove(p, ev);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      dragging.current = false;
      if (begun) useEditor.getState().endEdit();
      setGuides([]); setReadout(null);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up, { once: true });
    return start;
  };

  const locked = (c: Clip) => {
    const t = trackOf(useEditor.getState(), c.track);
    if (t.locked) toast(`${t.name} is locked. Unlock it to move things on it.`);
    return t.locked;
  };

  // ---------- Move (with snapping to centre lines and safe margins) ----------
  const startMove = (e: React.PointerEvent, c: Clip) => {
    if (locked(c)) return;
    const o = { x: c.x, y: c.y }, b0 = clipBox(c);
    const start = drag(e, (p, ev) => {
      let nx = o.x + (p.x - start.x), ny = o.y + (p.y - start.y);
      const found: Guide[] = [];
      if (useEditor.getState().snap && !ev.altKey) {
        const tol = SNAP_PX / k, cx = b0.cx + (nx - o.x), cy = b0.cy + (ny - o.y);
        const half = { w: b0.w / 2, h: b0.h / 2 }, rotated = Math.abs(b0.rot % 90) > 0.5;
        const xs: [number, number][] = [[W / 2, cx]]; // [target, current]
        const ys: [number, number][] = [[H / 2, cy]];
        if (!rotated) {
          xs.push([W * SAFE, cx - half.w], [W * (1 - SAFE), cx + half.w]);
          ys.push([H * SAFE, cy - half.h], [H * (1 - SAFE), cy + half.h]);
        }
        const xHit = xs.find(([target, cur]) => Math.abs(target - cur) < tol);
        const yHit = ys.find(([target, cur]) => Math.abs(target - cur) < tol);
        if (xHit) { nx += xHit[0] - xHit[1]; found.push({ axis: "x", at: xHit[0] }); }
        if (yHit) { ny += yHit[0] - yHit[1]; found.push({ axis: "y", at: yHit[0] }); }
      }
      nx = Math.round(nx); ny = Math.round(ny);
      useEditor.getState().updateClip(c.id, { x: nx, y: ny });
      setGuides(found);
      setReadout(`X ${nx}  Y ${ny}`);
    });
  };

  // ---------- Scale from a corner, about the clip's centre ----------
  const startScale = (e: React.PointerEvent) => {
    if (!clip || !box || locked(clip)) return;
    const s0 = clip.scale, c = { x: box.cx, y: box.cy };
    const start = drag(e, (p) => {
      const d0 = Math.max(1, Math.hypot(start.x - c.x, start.y - c.y));
      const s = Math.min(8, Math.max(0.05, (s0 * Math.hypot(p.x - c.x, p.y - c.y)) / d0));
      // Keep the box centre where it is: compensate the offset for the scale change.
      const r = localRect(clip), lc = { x: r.x + r.w / 2 - W / 2, y: r.y + r.h / 2 - H / 2 };
      const a = (clip.rot * Math.PI) / 180, ds = s - s0;
      const shift = { x: (lc.x * Math.cos(a) - lc.y * Math.sin(a)) * ds, y: (lc.x * Math.sin(a) + lc.y * Math.cos(a)) * ds };
      useEditor.getState().updateClip(clip.id, { scale: +s.toFixed(3), x: Math.round(clip.x - shift.x), y: Math.round(clip.y - shift.y) });
      setReadout(`${Math.round(s * 100)}%`);
    });
  };

  // ---------- Rotate around the box centre ----------
  const startRotate = (e: React.PointerEvent) => {
    if (!clip || !box || locked(clip)) return;
    const r0 = clip.rot, c = { x: box.cx, y: box.cy };
    const start = drag(e, (p, ev) => {
      const a0 = Math.atan2(start.y - c.y, start.x - c.x), a1 = Math.atan2(p.y - c.y, p.x - c.x);
      let r = r0 + ((a1 - a0) * 180) / Math.PI;
      r = ((((r + 180) % 360) + 360) % 360) - 180;
      if (ev.shiftKey) r = Math.round(r / 15) * 15;
      r = Math.round(r);
      // Rotation in drawClip pivots on the canvas centre; move the clip so it turns about its own box.
      const rect = localRect(clip), lc = { x: rect.x + rect.w / 2, y: rect.y + rect.h / 2 };
      const before = toCanvas(clip, lc.x, lc.y), after = toCanvas({ ...clip, rot: r }, lc.x, lc.y);
      useEditor.getState().updateClip(clip.id, { rot: r, x: Math.round(clip.x + before.x - after.x), y: Math.round(clip.y + before.y - after.y) });
      setReadout(`${r}°`);
    });
  };

  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    const p = pointFrom(e), s = useEditor.getState();
    // Prefer the selected clip when it's under the pointer, so a clip behind a title stays grabbable once selected.
    const hit = clip && box && insideBox(clip, p) ? clip : hitTest(p.x, p.y, s.time);
    if (!hit) { s.select(null); return; }
    if (s.selected !== hit.id) s.select(hit.id);
    startMove(e, hit);
  };

  const onDoubleClick = (e: React.MouseEvent) => {
    const p = pointFrom(e), hit = hitTest(p.x, p.y, useEditor.getState().time);
    if (!hit?.text) return;
    useEditor.getState().select(hit.id);
    // The inspector re-renders for the new selection; focus its text box on the next frame.
    requestAnimationFrame(() => requestAnimationFrame(() => {
      const ta = document.getElementById("f-text") as HTMLTextAreaElement | null;
      ta?.focus(); ta?.select(); ta?.scrollIntoView({ block: "nearest" });
    }));
  };

  const onHover = (e: React.PointerEvent) => {
    if (dragging.current) return;
    const p = pointFrom(e), hit = hitTest(p.x, p.y, useEditor.getState().time);
    setHover((h) => (h?.id === hit?.id ? h : hit));
  };

  const hoverBox = hover && hover.id !== clip?.id ? clipBox(hover) : null;
  const handle = "absolute size-2.5 rounded-[2px] border border-signal bg-white shadow-[0_1px_3px_rgb(0_0_0/.5)]";

  return (
    <div ref={ref} className={cn("absolute inset-0 touch-none select-none", hover || box ? "cursor-move" : "cursor-default")}
      onPointerDown={onPointerDown} onPointerMove={onHover} onPointerLeave={() => setHover(null)} onDoubleClick={onDoubleClick}
      aria-label="Preview. Click to select, drag to move.">
      {guides.map((g) => (
        <div key={g.axis + g.at} className="pointer-events-none absolute bg-signal"
          style={g.axis === "x" ? { left: g.at * k, top: 0, bottom: 0, width: 1 } : { top: g.at * k, left: 0, right: 0, height: 1 }} />
      ))}
      {hoverBox && <div className="pointer-events-none absolute border border-white/45" style={boxStyle(hoverBox, k)} />}
      {box && (
        <div className="pointer-events-none absolute outline-[1.5px] outline-signal outline-solid" style={boxStyle(box, k)}>
          {(["nw", "ne", "sw", "se"] as const).map((corner) => (
            <span key={corner} onPointerDown={startScale}
              className={cn(handle, "pointer-events-auto",
                corner[0] === "n" ? "-top-[5px]" : "-bottom-[5px]", corner[1] === "w" ? "-left-[5px]" : "-right-[5px]",
                corner === "nw" || corner === "se" ? "cursor-nwse-resize" : "cursor-nesw-resize")} />
          ))}
          <span className="absolute -top-6 left-1/2 h-6 w-px -translate-x-1/2 bg-signal" />
          <span onPointerDown={startRotate} title="Drag to rotate. Hold Shift to snap to 15°."
            className="pointer-events-auto absolute -top-[30px] left-1/2 size-3 -translate-x-1/2 cursor-grab rounded-full border border-signal bg-white shadow-[0_1px_3px_rgb(0_0_0/.5)]" />
        </div>
      )}
      {readout && box && (
        <div className="pointer-events-none absolute -translate-x-1/2 rounded-full bg-black/75 px-2 py-0.5 type-timecode text-[10px] text-white"
          style={{ left: box.cx * k, top: Math.min(box.cy * k + (box.h * k) / 2 + 10, H * k - 22) }}>{readout}</div>
      )}
    </div>
  );
}

function insideBox(c: Clip, p: { x: number; y: number }) {
  const r = localRect(c), q = toLocal(c, p.x, p.y);
  return q.x >= r.x && q.x <= r.x + r.w && q.y >= r.y && q.y <= r.y + r.h;
}
