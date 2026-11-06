"use client";

import * as React from "react";
import { Blend, Copy, Eye, EyeOff, Lock, LockOpen, Magnet, Scissors, Trash2, Volume2, VolumeX, ZoomIn, ZoomOut } from "lucide-react";
import { toast } from "sonner";
import { Slider } from "@/components/ui/slider";
import { Separator } from "@/components/ui/separator";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { IconButton, Panel } from "./primitives";
import { cn } from "@/lib/utils";
import { ASSETS, fontStack } from "@/lib/editor/assets";
import { effectDef } from "@/lib/editor/fx/registry";
import { assetThumb, importFiles, transitionWindows, waveURL } from "@/lib/editor/render";
import { pause, play, restartAudio, seek } from "@/lib/editor/playback";
import { MIN_CLIP, TRACK_HEIGHT, accepts, projectEnd, trackOf, useEditor, type Clip, type Track } from "@/lib/editor/store";
import { useDrag } from "@/lib/editor/dnd";
import { snapFrame } from "@/lib/editor/time";

const RULER_H = 28;
const CLIP_BG: Record<Clip["type"], string> = {
  scene: "bg-clip-video", video: "bg-clip-video", image: "bg-clip-video", photo: "bg-clip-video",
  overlay: "bg-clip-overlay", text: "bg-clip-text", music: "bg-clip-audio", audio: "bg-clip-audio",
};

function useHeadWidth() {
  const [w, setW] = React.useState(148);
  React.useEffect(() => {
    const mq = window.matchMedia("(max-width: 767px)");
    const on = () => setW(mq.matches ? 96 : 148);
    on(); mq.addEventListener("change", on);
    return () => mq.removeEventListener("change", on);
  }, []);
  return w;
}

export function TimelinePanel() {
  const zoom = useEditor((s) => s.zoom);
  const snap = useEditor((s) => s.snap);
  const editMode = useEditor((s) => s.editMode);
  const tracks = useEditor((s) => s.tracks);
  const clips = useEditor((s) => s.clips);
  const { set, setZoom, split, duplicate, remove } = useEditor.getState();
  const scroller = React.useRef<HTMLDivElement>(null);
  const grid = React.useRef<HTMLDivElement>(null);
  const [snapAt, setSnapAt] = React.useState<number | null>(null);
  const headW = useHeadWidth();
  const width = Math.max(projectEnd(clips) + 20, 60) * zoom;

  const timeAt = React.useCallback(
    (clientX: number) => Math.max(0, (clientX - grid.current!.getBoundingClientRect().left - headW) / useEditor.getState().zoom),
    [headW],
  );

  // Keep the time under the cursor (or the centre of the view) still while zooming.
  const zoomTo = React.useCallback((z: number, anchorX?: number) => {
    const el = scroller.current!, old = useEditor.getState().zoom;
    const ax = anchorX ?? (el.clientWidth - headW) / 2, t = (el.scrollLeft + ax) / old;
    setZoom(z);
    requestAnimationFrame(() => { el.scrollLeft = t * useEditor.getState().zoom - ax; });
  }, [setZoom, headW]);
  React.useEffect(() => {
    const el = scroller.current!;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey && !e.metaKey) return;
      e.preventDefault();
      zoomTo(useEditor.getState().zoom * (e.deltaY < 0 ? 1.12 : 0.89), e.clientX - el.getBoundingClientRect().left - headW);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    const onZoomKey = (e: Event) => zoomTo(useEditor.getState().zoom * (e as CustomEvent<number>).detail);
    window.addEventListener("rayna:zoom", onZoomKey);
    return () => { el.removeEventListener("wheel", onWheel); window.removeEventListener("rayna:zoom", onZoomKey); };
  }, [zoomTo, headW]);

  const scrub = React.useCallback((e: React.PointerEvent) => {
    if (e.button !== 0) return;
    const wasPlaying = useEditor.getState().playing;
    if (wasPlaying) pause();
    seek(snapFrame(timeAt(e.clientX)));
    const mv = (ev: PointerEvent) => seek(snapFrame(timeAt(ev.clientX)));
    window.addEventListener("pointermove", mv);
    window.addEventListener("pointerup", () => { window.removeEventListener("pointermove", mv); if (wasPlaying) play(); }, { once: true });
  }, [timeAt]);

  return (
    <Panel aria-label="Timeline">
      <div className="flex flex-wrap items-center gap-1.5 border-b border-hairline-soft px-3 py-2">
        <IconButton label="Split at playhead" shortcut="S" size="sm" onClick={split} className="w-auto px-3"><Scissors />Split</IconButton>
        <IconButton label="Duplicate clip" shortcut="⌘D" size="icon-sm" onClick={duplicate}><Copy /></IconButton>
        <IconButton label="Delete clip" shortcut="⌫" size="icon-sm" onClick={remove}><Trash2 /></IconButton>
        <Separator orientation="vertical" className="mx-1 h-[18px]!" />
        <ToggleGroup type="single" size="sm" variant="outline" spacing={0} value={editMode} aria-label="Edit mode"
          onValueChange={(v) => { if (!v) return; set({ editMode: v as "insert" | "overwrite" }); toast(v === "insert" ? "Insert: clips move along to make room" : "Overwrite: new clips cut into what's underneath"); }}>
          <ToggleGroupItem value="insert" className="px-2.5 type-micro" title="Existing clips move along to make room">Insert</ToggleGroupItem>
          <ToggleGroupItem value="overwrite" className="px-2.5 type-micro" title="New clips cut into what's underneath">Overwrite</ToggleGroupItem>
        </ToggleGroup>
        <IconButton label={snap ? "Turn snapping off" : "Turn snapping on"} shortcut="N" size="sm" aria-pressed={snap}
          variant={snap ? "secondary" : "ghost"} onClick={() => set({ snap: !snap })}
          className={cn("w-auto px-3", snap && "bg-surface-3 shadow-[inset_0_0_0_1px_var(--hairline)]")}><Magnet />Snap</IconButton>
        <div className="flex-1" />
        <div className="flex w-44 items-center gap-2 text-ink-muted">
          <ZoomOut className="size-4 shrink-0" />
          <Slider aria-label="Timeline zoom" min={12} max={240} step={1} value={[zoom]} onValueChange={([v]) => zoomTo(v)} />
          <ZoomIn className="size-4 shrink-0" />
        </div>
      </div>

      {/* One scroll area: the header column is sticky on the left and the ruler sticky on top, so they always line up. */}
      <div ref={scroller} className="relative min-h-0 flex-1 overflow-auto overscroll-contain">
        <div ref={grid} className="relative grid min-h-full content-start" style={{ gridTemplateColumns: `${headW}px ${width}px`, gridTemplateRows: `${RULER_H}px`, width: headW + width }}>
          <div className="sticky top-0 left-0 z-[7] border-r border-b border-hairline-soft bg-surface-1" />
          <Ruler zoom={zoom} seconds={width / zoom} onPointerDown={scrub} />
          {tracks.map((t) => (
            <React.Fragment key={t.id}>
              <TrackHead track={t} />
              <Lane track={t} clips={clips} zoom={zoom} timeAt={timeAt} onSnap={setSnapAt} onScrub={scrub} />
            </React.Fragment>
          ))}
          {snapAt !== null && <div className="pointer-events-none absolute bottom-0 z-[4] w-px bg-signal" style={{ top: RULER_H, left: headW + snapAt * zoom }} />}
          <Playhead scroller={scroller} offset={headW} />
        </div>
      </div>
    </Panel>
  );
}

// ---------- Ruler ----------
function Ruler({ zoom, seconds, onPointerDown }: { zoom: number; seconds: number; onPointerDown: (e: React.PointerEvent) => void }) {
  const steps = [0.5, 1, 2, 5, 10, 15, 30, 60, 120];
  const major = steps.find((s) => s * zoom >= 90) ?? 120;
  const minor = major / (major >= 5 ? 5 : major >= 1 ? 4 : 5);
  const ticks: number[] = [];
  for (let t = 0; t <= seconds; t += major) ticks.push(t);
  return (
    <div onPointerDown={onPointerDown} aria-label="Timeline ruler, drag to scrub"
      className="sticky top-0 z-[4] cursor-ew-resize border-b border-hairline-soft bg-surface-1">
      <div className="absolute inset-x-0 bottom-0 h-1.5" style={{ background: `repeating-linear-gradient(to right, var(--surface-3) 0 1px, transparent 1px ${minor * zoom}px)` }} />
      {ticks.map((t) => (
        <div key={t} className="absolute bottom-0 h-3 border-l border-neutral-600" style={{ left: t * zoom }}>
          <span className="absolute -top-1 left-1.5 whitespace-nowrap type-timecode text-[10px] text-ink-muted">
            {Math.floor(t / 60)}:{String(Math.floor(t % 60)).padStart(2, "0")}{t % 1 ? ".5" : ""}
          </span>
        </div>
      ))}
    </div>
  );
}

// ---------- Track header ----------
function TrackHead({ track: t }: { track: Track }) {
  const toggle = useEditor((s) => s.toggleTrack);
  const btn = "grid size-6 place-items-center rounded-sm text-ink-muted hover:bg-surface-2 hover:text-ink aria-pressed:text-coral [&_svg]:size-3.5";
  return (
    <div className="sticky left-0 z-[6] flex items-center gap-1.5 border-r border-b border-hairline-soft bg-surface-1 pr-1.5 pl-3" style={{ height: TRACK_HEIGHT[t.kind] }}>
      <span className="w-[22px] type-timecode text-[11px] text-ink-muted">{t.id}</span>
      <span className="flex-1 truncate type-micro font-medium max-md:hidden">{t.name}</span>
      {t.kind === "audio" ? (
        <button type="button" className={btn} aria-pressed={t.muted} aria-label={`${t.muted ? "Unmute" : "Mute"} ${t.name}`} title={t.muted ? "Unmute" : "Mute"}
          onClick={() => { toggle(t.id, "muted"); restartAudio(); }}>{t.muted ? <VolumeX /> : <Volume2 />}</button>
      ) : (
        <button type="button" className={btn} aria-pressed={t.hidden} aria-label={`${t.hidden ? "Show" : "Hide"} ${t.name}`} title={t.hidden ? "Show" : "Hide"}
          onClick={() => toggle(t.id, "hidden")}>{t.hidden ? <EyeOff /> : <Eye />}</button>
      )}
      <button type="button" className={btn} aria-pressed={t.locked} aria-label={`${t.locked ? "Unlock" : "Lock"} ${t.name}`} title={t.locked ? "Unlock" : "Lock"}
        onClick={() => toggle(t.id, "locked")}>{t.locked ? <Lock /> : <LockOpen />}</button>
    </div>
  );
}

// ---------- Lane ----------
function Lane({ track, clips, zoom, timeAt, onSnap, onScrub }: {
  track: Track; clips: Clip[]; zoom: number; timeAt: (x: number) => number;
  onSnap: (t: number | null) => void; onScrub: (e: React.PointerEvent) => void;
}) {
  const target = useDrag((s) => (s.target?.kind === "lane" && s.target.track === track.id ? s.target : null));
  const dragId = useDrag((s) => s.payload?.id);
  const [fileOver, setFileOver] = React.useState(false);
  const mine = clips.filter((c) => c.track === track.id).sort((a, b) => a.start - b.start);
  const ghostDur = dragId ? ASSETS.get(dragId)?.dur ?? 0 : 0;

  return (
    <div data-track={track.id}
      className={cn("relative border-b border-hairline-soft",
        (target?.ok || fileOver) && "bg-signal/[.06]", target && !target.ok && "bg-coral/[.05]",
        (track.hidden || track.muted) && "[&>[data-clip]]:opacity-40")}
      style={{ height: TRACK_HEIGHT[track.kind] }}
      onPointerDown={(e) => { if (e.target !== e.currentTarget) return; useEditor.getState().select(null); onScrub(e); }}
      onDragOver={(e) => { if (e.dataTransfer.types.includes("Files")) { e.preventDefault(); setFileOver(true); } }}
      onDragLeave={() => setFileOver(false)}
      onDrop={(e) => {
        if (!e.dataTransfer.files.length) return;
        e.preventDefault(); e.stopPropagation(); setFileOver(false);
        if (track.locked) { toast(`${track.name} is locked. Unlock it to drop clips there.`); return; }
        importFiles(e.dataTransfer.files, { start: snapFrame(timeAt(e.clientX)), track: track.id });
      }}>
      {mine.map((c) => <ClipView key={c.id} clip={c} zoom={zoom} onSnap={onSnap} />)}
      {transitionWindows(mine).map((w) => (
        <button key={"tr-" + w.b.id} type="button" title={`${w.def.name} · ${(w.to - w.from).toFixed(2)}s. Click to edit.`}
          aria-label={`${w.def.name} transition into ${w.b.name}`}
          onPointerDown={(e) => { e.stopPropagation(); useEditor.getState().select(w.b.id); }}
          className="absolute top-1/2 z-[3] flex h-5 -translate-y-1/2 items-center justify-center gap-1 overflow-hidden rounded-full bg-ink px-1.5 text-[10px] font-medium text-black shadow-[0_2px_8px_rgb(0_0_0/.5)] hover:bg-white focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          style={{ left: w.from * zoom, width: Math.max(20, (w.to - w.from) * zoom) }}>
          <Blend className="size-3 shrink-0" />{(w.to - w.from) * zoom > 64 && <span className="truncate">{w.def.name}</span>}
        </button>
      ))}
      {target?.ok && (
        <div className="pointer-events-none absolute top-1 bottom-1 z-[3] rounded-sm border border-dashed border-signal bg-signal/10"
          style={{ left: target.time * zoom, width: Math.max(6, Math.min(ghostDur, 24) * zoom) }} />
      )}
    </div>
  );
}

// ---------- Clip ----------
const ClipView = React.memo(function ClipView({ clip: c, zoom, onSnap }: { clip: Clip; zoom: number; onSnap: (t: number | null) => void }) {
  const selected = useEditor((s) => s.selected === c.id);
  const fxTarget = useDrag((s) => s.target?.kind === "clip" && s.target.clipId === c.id);
  const [dragging, setDragging] = React.useState(false);
  const a = ASSETS.get(c.asset);
  const thumb = c.type !== "text" && c.type !== "music" && c.type !== "audio" ? assetThumb(c.asset) : undefined;
  const on = c.effects.filter((e) => e.enabled);
  const fx = on.length === 0 ? null : on.length === 1 ? effectDef(on[0].id)?.name : `${on.length} effects`;

  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    const s = useEditor.getState();
    s.select(c.id);
    if (trackOf(s, c.track).locked) { toast(`${trackOf(s, c.track).name} is locked`); return; }
    const mode = ((e.target as HTMLElement).dataset.side as "l" | "r" | undefined) ?? "move";
    const x0 = e.clientX, y0 = e.clientY, o = { start: c.start, dur: c.dur, in: c.in };
    const pts = [0, s.time, ...s.clips.filter((k) => k.id !== c.id).flatMap((k) => [k.start, k.start + k.dur])];
    const media = c.type === "video" || c.type === "audio";
    let moved = false;

    const snapTo = (t: number) => {
      const st = useEditor.getState();
      if (!st.snap) return { t, hit: null as number | null };
      const tol = 8 / st.zoom; let best: number | null = null;
      for (const p of pts) if (Math.abs(p - t) < tol && (best === null || Math.abs(p - t) < Math.abs(best - t))) best = p;
      return best === null ? { t, hit: null } : { t: best, hit: best };
    };

    const move = (ev: PointerEvent) => {
      if (!moved && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 3) return;
      const st = useEditor.getState();
      if (!moved) { st.commit(); moved = true; setDragging(true); document.documentElement.style.cursor = mode === "move" ? "grabbing" : "ew-resize"; }
      const cur = st.clips.find((k) => k.id === c.id); if (!cur) return;
      const dx = (ev.clientX - x0) / st.zoom;
      let hit: number | null = null;
      const patch: Partial<Clip> = {};
      if (mode === "move") {
        let ns = Math.max(0, o.start + dx);
        const a1 = snapTo(ns), a2 = snapTo(ns + cur.dur);
        if (a1.hit !== null) { ns = a1.t; hit = a1.hit; } else if (a2.hit !== null) { ns = a2.t - cur.dur; hit = a2.hit; }
        patch.start = snapFrame(Math.max(0, ns));
        const tid = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>("[data-track]")?.dataset.track;
        if (tid && tid !== cur.track && accepts(st, tid, cur.type) && !trackOf(st, tid).locked) patch.track = tid;
      } else if (mode === "l") {
        const end = o.start + o.dur;
        const sn = snapTo(o.start + dx); hit = sn.hit;
        let ns = Math.max(0, Math.min(sn.t, end - MIN_CLIP));
        if (st.editMode === "insert") {
          // Insert mode never cuts: the start edge stops at the previous clip.
          const prevEnd = st.clips.filter((k) => k.track === cur.track && k.id !== c.id && k.start + k.dur <= o.start + 1e-6)
            .reduce((mx, k) => Math.max(mx, k.start + k.dur), 0);
          ns = Math.max(ns, prevEnd);
        }
        if (media) ns = Math.max(ns, o.start - o.in);
        patch.start = snapFrame(ns); patch.dur = end - patch.start; patch.in = Math.max(0, o.in + (patch.start - o.start));
      } else {
        const sn = snapTo(o.start + o.dur + dx); hit = sn.hit;
        let dur = Math.max(MIN_CLIP, sn.t - cur.start);
        if (media && a) dur = Math.min(dur, a.dur - cur.in);
        patch.dur = snapFrame(dur);
      }
      const dur = patch.dur ?? cur.dur;
      patch.fadeIn = Math.min(cur.fadeIn, dur / 2); patch.fadeOut = Math.min(cur.fadeOut, dur / 2);
      st.updateClip(c.id, patch);
      onSnap(hit);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointercancel", up);
      document.documentElement.style.cursor = "";
      setDragging(false); onSnap(null);
      if (moved) { useEditor.getState().settle(c.id, mode === "move"); restartAudio(); }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up, { once: true });
    window.addEventListener("pointercancel", up, { once: true });
  };

  return (
    <div data-clip={c.id} data-selected={selected} onPointerDown={onPointerDown}
      className={cn("group/clip absolute top-1 bottom-1 cursor-grab touch-none overflow-hidden rounded-sm select-none shadow-[inset_0_0_0_1px_rgb(255_255_255/.08)]",
        CLIP_BG[c.type],
        selected && "z-[2] shadow-[inset_0_0_0_2px_var(--signal-blue)]",
        fxTarget && "z-[2] shadow-[inset_0_0_0_2px_var(--signal-blue),0_0_0_3px_rgb(0_153_255/.25)]",
        dragging && "z-[3] cursor-grabbing opacity-90 shadow-[inset_0_0_0_2px_var(--signal-blue),0_10px_30px_rgb(0_0_0/.5)]")}
      style={{ left: c.start * zoom, width: Math.max(6, c.dur * zoom) }}>
      {thumb && <div className="absolute inset-x-0 top-[18px] bottom-0 bg-repeat-x opacity-85" style={{ backgroundImage: `url(${thumb})`, backgroundSize: "auto 100%" }} />}
      {(c.type === "music" || c.type === "audio") && (
        <div className="absolute inset-x-0 top-4 bottom-0.5 bg-repeat-x opacity-80"
          style={{ backgroundImage: `url(${waveURL(c.type === "music" ? c.asset.length + c.asset.charCodeAt(3) : 9)})`, backgroundSize: "240px 100%" }} />
      )}
      {c.text && (
        <div className="absolute inset-x-2 top-[18px] bottom-0 flex items-center truncate text-[13px] text-white/85"
          style={{ fontFamily: fontStack(c.text.font), fontWeight: c.text.weight }}>{c.text.content.split("\n")[0]}</div>
      )}
      <div className="absolute inset-x-0 top-0 flex h-[18px] items-center gap-1.5 overflow-hidden bg-gradient-to-b from-black/45 to-black/25 px-2 type-micro text-[11px] font-medium whitespace-nowrap">
        {c.name}
        {fx && <span className="rounded-[3px] bg-white/15 px-1 py-px type-timecode text-[9px] uppercase">{fx}</span>}
      </div>
      {c.fadeIn > 0 && <div className="clip-fade-in pointer-events-none absolute top-[18px] bottom-0 left-0" style={{ width: c.fadeIn * zoom }} />}
      {c.fadeOut > 0 && <div className="clip-fade-out pointer-events-none absolute top-[18px] right-0 bottom-0" style={{ width: c.fadeOut * zoom }} />}
      <div className="clip-handle" data-side="l" />
      <div className="clip-handle" data-side="r" />
    </div>
  );
});

// ---------- Playhead ----------
function Playhead({ scroller, offset }: { scroller: React.RefObject<HTMLDivElement | null>; offset: number }) {
  const time = useEditor((s) => s.time);
  const zoom = useEditor((s) => s.zoom);
  const playing = useEditor((s) => s.playing);
  const x = time * zoom;
  React.useEffect(() => {
    const el = scroller.current; if (!el || !playing) return;
    const view = el.clientWidth - offset;
    if (x > el.scrollLeft + view - 60 || x < el.scrollLeft) el.scrollLeft = x - 80;
  }, [x, playing, scroller, offset]);
  return (
    <div className="pointer-events-none absolute top-0 bottom-0 z-[5] w-0" style={{ left: offset + x }} aria-hidden>
      <div className="absolute inset-y-0 -left-px w-px bg-ink" />
      <div className="playhead-knob absolute top-0.5 -left-1.5 h-3.5 w-3 bg-ink" />
    </div>
  );
}
