"use client";

import { useDrag } from "@/lib/editor/dnd";
import { trackOf, useEditor } from "@/lib/editor/store";
import { tc } from "@/lib/editor/time";
import { cn } from "@/lib/utils";

/** Follows the pointer while something is dragged out of the library and says where it will land. */
export function DragGhost() {
  const { payload, x, y, target } = useDrag();
  const tracks = useEditor((s) => s.tracks);
  if (!payload) return null;

  let hint = payload.kind === "asset" ? "Drop on a track or the preview" : "Drop on a clip";
  let bad = false;
  if (target?.kind === "lane") {
    const t = trackOf({ tracks }, target.track);
    if (t.locked) { hint = `${t.name} is locked`; bad = true; }
    else if (!target.ok) { hint = `${t.name} doesn't take this. Drops on its own track.`; bad = true; }
    else hint = `${t.name} at ${tc(target.time)}`;
  } else if (target?.kind === "stage") hint = "Add at the playhead";
  else if (target?.kind === "clip") hint = payload.kind === "effect" ? "Apply to this clip" : "Add fade to this clip";

  return (
    <div className="pointer-events-none fixed top-0 left-0 z-50 flex w-52 flex-col overflow-hidden rounded-md bg-surface-2 shadow-[0_0_0_0.5px_rgb(255_255_255/.1),0_10px_30px_rgb(0_0_0/.5)]"
      style={{ transform: `translate(${x + 14}px, ${y + 14}px)` }} aria-hidden>
      {payload.thumb && <div className="aspect-video bg-black bg-cover bg-center" style={{ backgroundImage: `url(${payload.thumb})` }} />}
      <div className="flex flex-col gap-0.5 px-2.5 py-2">
        <span className="truncate type-micro font-medium">{payload.label}</span>
        <span className={cn("truncate type-micro text-[11px]", bad ? "text-coral" : target ? "text-signal" : "text-ink-muted")}>{hint}</span>
      </div>
    </div>
  );
}
