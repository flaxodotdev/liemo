"use client";

import * as React from "react";
import { ChevronLeft, ChevronRight, Pause, Play, Repeat, Scan, SkipBack, SkipForward } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { IconButton, Panel, Timecode } from "./primitives";
import { StageOverlay } from "./stage-overlay";
import { cn } from "@/lib/utils";
import { attachStage, importFiles } from "@/lib/editor/render";
import { pause, seek, step, togglePlay } from "@/lib/editor/playback";
import { projectEnd, useEditor } from "@/lib/editor/store";
import { useDrag } from "@/lib/editor/dnd";

export function PreviewPanel() {
  const wrap = React.useRef<HTMLDivElement>(null);
  const [box, setBox] = React.useState({ w: 0, h: 0 });
  const playing = useEditor((s) => s.playing);
  const loop = useEditor((s) => s.loop);
  const guides = useEditor((s) => s.guides);
  const set = useEditor((s) => s.set);
  const overStage = useDrag((s) => s.target?.kind === "stage");

  // Fit a 16:9 stage inside whatever space the panel has.
  React.useEffect(() => {
    const el = wrap.current; if (!el) return;
    const ro = new ResizeObserver(([e]) => {
      const pw = e.contentRect.width, ph = e.contentRect.height;
      let w = pw, h = (w * 9) / 16;
      if (h > ph) { h = ph; w = (h * 16) / 9; }
      setBox({ w: Math.max(0, w), h: Math.max(0, h) });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  return (
    <Panel aria-label="Preview">
      <div ref={wrap} data-drop-stage className="relative m-4 grid min-h-0 flex-1 place-items-center"
        onDragOver={(e) => { if (e.dataTransfer.types.includes("Files")) e.preventDefault(); }}
        onDrop={(e) => {
          if (!e.dataTransfer.files.length) return;
          e.preventDefault(); e.stopPropagation();
          importFiles(e.dataTransfer.files, { start: useEditor.getState().time });
        }}>
        <div className={cn("relative overflow-hidden rounded-sm bg-black shadow-[0_0_0_0.5px_rgb(255_255_255/.1),0_10px_30px_rgb(0_0_0/.4)] transition-shadow",
          overStage && "shadow-[0_0_0_2px_var(--signal-blue),0_10px_30px_rgb(0_0_0/.4)]")} style={{ width: box.w, height: box.h }}>
          <canvas ref={attachStage} className="block size-full" aria-label="Video preview" />
          <StageOverlay width={box.w} />
          {guides && (
            <div className="pointer-events-none absolute inset-0" aria-hidden>
              <div className="absolute inset-[5%] border border-dashed border-white/25" />
              <div className="absolute inset-[10%] border border-dashed border-signal/40" />
            </div>
          )}
        </div>
      </div>

      <div className="flex items-center gap-2 border-t border-hairline-soft px-4 py-2.5">
        <NowTimecode />
        <div className="flex flex-1 items-center justify-center gap-1.5">
          <IconButton label="Go to start" shortcut="Home" size="icon-sm" variant="ghost" onClick={() => { pause(); seek(0); }}><SkipBack /></IconButton>
          <IconButton label="Previous frame" shortcut="←" size="icon-sm" variant="ghost" onClick={() => step(-1)}><ChevronLeft /></IconButton>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button size="icon-lg" aria-label={playing ? "Pause" : "Play"} onClick={togglePlay} className="size-11">
                {playing ? <Pause className="size-[18px] fill-current" /> : <Play className="ml-0.5 size-[18px] fill-current" />}
              </Button>
            </TooltipTrigger>
            <TooltipContent>{playing ? "Pause" : "Play"} <kbd className="ml-2 type-timecode text-[11px] opacity-70">Space</kbd></TooltipContent>
          </Tooltip>
          <IconButton label="Next frame" shortcut="→" size="icon-sm" variant="ghost" onClick={() => step(1)}><ChevronRight /></IconButton>
          <IconButton label="Go to end" shortcut="End" size="icon-sm" variant="ghost" onClick={() => { pause(); seek(projectEnd(useEditor.getState().clips) - 1 / 30); }}><SkipForward /></IconButton>
        </div>
        <IconButton label="Loop playback" shortcut="L" size="icon-sm" variant="ghost" aria-pressed={loop} onClick={() => set({ loop: !loop })}
          className={cn(loop && "bg-surface-3 text-ink")}><Repeat /></IconButton>
        <IconButton label="Safe-area guides" shortcut="G" size="icon-sm" variant="ghost" aria-pressed={guides} onClick={() => set({ guides: !guides })}
          className={cn(guides && "bg-surface-3 text-ink")}><Scan /></IconButton>
      </div>
    </Panel>
  );
}

function NowTimecode() {
  const time = useEditor((s) => s.time);
  const end = useEditor((s) => projectEnd(s.clips));
  return (
    <div className="min-w-0 text-[15px]">
      <Timecode seconds={time} className="text-ink" />
      <span className="type-timecode text-ink-muted"> / </span>
      <Timecode seconds={end} className="text-ink-muted" />
    </div>
  );
}
