"use client";

import * as React from "react";
import Link from "next/link";
import { Check, ChevronLeft, CloudAlert, Loader2, Redo2, Undo2, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { IconButton, Timecode } from "./primitives";
import { ExportDialog } from "./export-dialog";
import { importFiles } from "@/lib/editor/render";
import { projectEnd, useEditor } from "@/lib/editor/store";
import { relativeTime, saveNow, useSaveStatus } from "@/lib/projects/session";
import { BrandMark } from "@/components/brand/brand-mark";

export function TopBar() {
  const name = useEditor((s) => s.name);
  const canUndo = useEditor((s) => s.past.length > 0);
  const canRedo = useEditor((s) => s.future.length > 0);
  const end = useEditor((s) => projectEnd(s.clips));
  const { set, undo, redo } = useEditor.getState();
  const input = React.useRef<HTMLInputElement>(null);

  return (
    <header className="flex min-w-0 items-center gap-4">
      <Link href="/" title="All projects" className="group flex items-center gap-2 rounded-md font-display text-[22px] font-semibold tracking-[-1px] outline-none focus-visible:ring-2 focus-visible:ring-ring">
        <ChevronLeft className="-mr-1 size-4 text-ink-muted transition-transform group-hover:-translate-x-0.5" aria-hidden />
        <BrandMark />
        Liemo
      </Link>
      <input id="project-name" aria-label="Project name" value={name} onChange={(e) => set({ name: e.target.value })}
        className="w-56 min-w-0 rounded-sm bg-transparent px-2 py-1.5 type-body-sm text-ink-muted outline-none hover:bg-surface-1 focus:bg-surface-1 focus:text-ink focus:ring-1 focus:ring-ring" />
      <Badge variant="secondary" className="hidden h-7 rounded-full bg-surface-1 px-3 type-caption text-ink-muted md:inline-flex">
        1280 × 720 · <span className="type-timecode text-[12px] text-ink">30 fps</span>
      </Badge>
      <Badge variant="secondary" className="hidden h-7 rounded-full bg-surface-1 px-3 type-caption text-ink-muted md:inline-flex">
        Length <Timecode seconds={end} className="text-[12px] text-ink" />
      </Badge>
      <SaveIndicator />
      <div className="flex-1" />
      <IconButton label="Undo" shortcut="⌘Z" disabled={!canUndo} onClick={undo} className="bg-surface-1"><Undo2 /></IconButton>
      <IconButton label="Redo" shortcut="⇧⌘Z" disabled={!canRedo} onClick={redo} className="bg-surface-1"><Redo2 /></IconButton>
      <Button size="lg" variant="secondary" className="bg-surface-1 px-4 hover:bg-surface-2" onClick={() => input.current?.click()}><Upload />Import</Button>
      <ExportDialog />
      <input ref={input} type="file" accept="video/*,audio/*,image/*" multiple hidden
        onChange={(e) => { if (e.target.files?.length) importFiles(e.target.files); e.target.value = ""; }} />
    </header>
  );
}

function SaveIndicator() {
  const { state, at } = useSaveStatus();
  const [, tick] = React.useState(0);
  React.useEffect(() => { const t = setInterval(() => tick((n) => n + 1), 30000); return () => clearInterval(t); }, []);
  if (state === "idle") return null;
  if (state === "error") {
    return (
      <button type="button" onClick={() => void saveNow()} className="flex items-center gap-1.5 rounded-full px-2 py-1 type-caption text-coral hover:bg-surface-1">
        <CloudAlert className="size-3.5" />Couldn&apos;t save. Retry
      </button>
    );
  }
  return (
    <span className="hidden items-center gap-1.5 type-caption text-ink-muted sm:flex" role="status" aria-live="polite">
      {state === "saving" ? <><Loader2 className="size-3.5 animate-spin" />Saving</> : <><Check className="size-3.5 text-success" />Saved {relativeTime(at)}</>}
    </span>
  );
}
