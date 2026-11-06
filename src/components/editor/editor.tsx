"use client";

import * as React from "react";
import { toast } from "sonner";
import { TopBar } from "./top-bar";
import { LibraryPanel } from "./library-panel";
import { PreviewPanel } from "./preview-panel";
import { InspectorPanel } from "./inspector-panel";
import { TimelinePanel } from "./timeline-panel";
import { DragGhost } from "./drag-ghost";
import { FONTS, fontStack } from "@/lib/editor/assets";
import { importFiles, requestRender } from "@/lib/editor/render";
import { seek, startLoop, step, togglePlay } from "@/lib/editor/playback";
import Link from "next/link";
import { Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { projectEnd, useEditor, type Clip } from "@/lib/editor/store";
import { closeProject, openProject, saveNow, takePendingImport } from "@/lib/projects/session";

/** Clips go on the system clipboard as text with this prefix, so they paste across tabs. */
const CLIP_MARK = "rayna-clip:";

/** Elements where typing, arrows and clipboard keys belong to the element, not the editor. */
function isTextEntry(el: EventTarget | null) {
  if (!(el instanceof HTMLElement)) return false;
  if (el instanceof HTMLInputElement) return !["range", "checkbox", "radio", "button", "file"].includes(el.type);
  return !!el.closest("textarea, [contenteditable=''], [contenteditable='true'], [role=combobox], [role=listbox], [role=option], [role=menu]");
}

type LoadState = "loading" | "ready" | "missing" | "error";

/** Opens a saved project, then shows the workspace. Saves on the way out. */
export function Editor({ projectId }: { projectId: string }) {
  const [status, setStatus] = React.useState<LoadState>("loading");

  React.useEffect(() => {
    let alive = true;
    setStatus("loading");
    openProject(projectId)
      .then((ok) => { if (alive) setStatus(ok ? "ready" : "missing"); })
      .catch(() => { if (alive) setStatus("error"); });
    // Save straight away when the tab is hidden or closed, instead of waiting for the autosave delay.
    const flush = () => { void saveNow(); };
    const onHide = () => { if (document.visibilityState === "hidden") flush(); };
    window.addEventListener("pagehide", flush);
    document.addEventListener("visibilitychange", onHide);
    return () => { alive = false; window.removeEventListener("pagehide", flush); document.removeEventListener("visibilitychange", onHide); closeProject(); };
  }, [projectId]);

  if (status === "ready") return <Workspace />;
  return (
    <div className="grid h-dvh place-items-center px-4">
      {status === "loading" ? (
        <div className="flex items-center gap-2 type-caption text-ink-muted"><Loader2 className="size-4 animate-spin" />Opening project</div>
      ) : (
        <div className="flex max-w-sm flex-col items-center gap-3 text-center">
          <h1 className="type-display">{status === "missing" ? "Project not found" : "Couldn't open this project"}</h1>
          <p className="type-body text-ink-muted">
            {status === "missing"
              ? "It may have been deleted, or it was made in a different browser. Projects are saved in the browser you made them in."
              : "Project storage isn't available. Private windows and some browser settings block it."}
          </p>
          <Button asChild size="lg" className="px-4"><Link href="/">Back to projects</Link></Button>
        </div>
      )}
    </div>
  );
}

function Workspace() {
  const [fontsReady, setFontsReady] = React.useState(false);

  React.useEffect(() => {
    const files = takePendingImport();
    if (files?.length) importFiles(files, { start: 0 });
    const stop = startLoop();
    // Canvas text needs the web fonts loaded before it draws.
    Promise.all(FONTS.map((f) => document.fonts.load(`${f.weight} 40px ${fontStack(f.family)}`).catch(() => null)))
      .then(() => { setFontsReady(true); requestRender(); });
    return stop;
  }, []);

  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTextEntry(e.target)) return;
      const s = useEditor.getState(), mod = e.metaKey || e.ctrlKey, k = e.key.toLowerCase();
      const onSlider = (e.target as HTMLElement).closest?.("[role=slider]");
      const zoom = (f: number) => window.dispatchEvent(new CustomEvent("rayna:zoom", { detail: f }));

      if (k === " ") { e.preventDefault(); togglePlay(); return; } // also stops a focused button from firing
      if (mod) {
        if (k === "z") { e.preventDefault(); if (e.shiftKey) s.redo(); else s.undo(); }
        else if (k === "y") { e.preventDefault(); s.redo(); }
        else if (k === "d") { e.preventDefault(); s.duplicate(); }
        else if (k === "c") s.copy();   // the copy event below puts it on the system clipboard
        else if (k === "x") s.cut();
        return;                         // ⌘V is handled by the paste event, which carries the data
      }
      if (onSlider && ["arrowleft", "arrowright", "arrowup", "arrowdown", "home", "end", "pageup", "pagedown"].includes(k)) return;
      if (e.altKey) {
        // Option + arrows nudge the selected picture or text on screen (Shift for 10px).
        const c = s.clips.find((k) => k.id === s.selected), d = e.shiftKey ? 10 : 1;
        const dir = ({ arrowleft: [-d, 0], arrowright: [d, 0], arrowup: [0, -d], arrowdown: [0, d] } as Record<string, [number, number]>)[k];
        if (c && dir && c.type !== "music" && c.type !== "audio") {
          e.preventDefault();
          if (!e.repeat) s.commit();
          s.updateClip(c.id, { x: c.x + dir[0], y: c.y + dir[1] });
        }
        return;
      }
      if (k === "s") s.split();
      else if (k === "delete" || k === "backspace") { e.preventDefault(); s.remove(); }
      else if (k === "arrowleft") { e.preventDefault(); step(e.shiftKey ? -10 : -1); }
      else if (k === "arrowright") { e.preventDefault(); step(e.shiftKey ? 10 : 1); }
      else if (k === "home") { e.preventDefault(); seek(0); }
      else if (k === "end") { e.preventDefault(); seek(projectEnd(s.clips) - 1 / 30); }
      else if (k === "n") s.set({ snap: !s.snap });
      else if (k === "l") s.set({ loop: !s.loop });
      else if (k === "g") s.set({ guides: !s.guides });
      else if (k === "=" || k === "+") zoom(1.25);
      else if (k === "-") zoom(0.8);
      else if (k === "escape") s.select(null);
    };

    const writeClipboard = (e: ClipboardEvent) => {
      if (isTextEntry(e.target) || window.getSelection()?.toString()) return;
      const c = useEditor.getState().clipboard;
      if (!c || !e.clipboardData) return;
      e.clipboardData.setData("text/plain", CLIP_MARK + JSON.stringify(c));
      e.preventDefault();
    };

    const onPaste = (e: ClipboardEvent) => {
      if (isTextEntry(e.target)) return;
      const s = useEditor.getState(), dt = e.clipboardData;
      if (!dt) { s.paste(); return; }
      e.preventDefault();
      // 1. Files (a copied image, a screenshot, a video from Finder) go straight onto the timeline.
      const files = [...dt.files];
      if (files.length) { importFiles(files, { start: s.time }); return; }
      const text = dt.getData("text/plain");
      // 2. A clip copied from Rayna, in this tab or another.
      if (text.startsWith(CLIP_MARK)) {
        try { s.paste(JSON.parse(text.slice(CLIP_MARK.length)) as Clip); }
        catch { toast("That clipboard content couldn't be read as a clip"); }
        return;
      }
      // 3. Plain text becomes a text clip at the playhead.
      const clean = text.trim();
      if (clean) {
        const short = clean.length <= 40 && !clean.includes("\n");
        const c = s.addClip(short ? "tx-title" : "tx-caption", "T1");
        if (c) {
          useEditor.getState().updateText(c.id, { content: clean.slice(0, 280) });
          useEditor.getState().updateClip(c.id, { name: short ? clean : "Pasted text" });
          toast("Added text from the clipboard");
        }
        return;
      }
      // 4. Nothing usable on the system clipboard: fall back to the last copied clip.
      s.paste();
    };

    // Files dropped outside a drop zone go to the media pool instead of opening in the tab.
    const onDragOver = (e: DragEvent) => { if (e.dataTransfer?.types.includes("Files")) e.preventDefault(); };
    const onDrop = (e: DragEvent) => { if (e.dataTransfer?.files.length) { e.preventDefault(); importFiles(e.dataTransfer.files); } };

    window.addEventListener("keydown", onKey);
    document.addEventListener("copy", writeClipboard);
    document.addEventListener("cut", writeClipboard);
    document.addEventListener("paste", onPaste);
    window.addEventListener("dragover", onDragOver);
    window.addEventListener("drop", onDrop);
    return () => {
      window.removeEventListener("keydown", onKey);
      document.removeEventListener("copy", writeClipboard);
      document.removeEventListener("cut", writeClipboard);
      document.removeEventListener("paste", onPaste);
      window.removeEventListener("dragover", onDragOver);
      window.removeEventListener("drop", onDrop);
    };
  }, []);

  return (
    <div className="grid h-dvh grid-rows-[56px_minmax(0,1fr)_minmax(240px,40%)] gap-2 px-4 pb-3 select-none max-lg:h-auto max-lg:min-h-dvh max-lg:grid-rows-[56px_auto_340px]">
      <TopBar />
      <div className="grid min-h-0 grid-cols-[minmax(280px,330px)_minmax(0,1fr)_minmax(260px,300px)] gap-2 max-lg:grid-cols-[minmax(260px,300px)_minmax(0,1fr)] max-md:grid-cols-1 max-lg:[&>*]:min-h-[380px] max-lg:[&>.inspector]:col-span-full max-lg:[&>.inspector]:max-h-[360px]">
        <LibraryPanel fontsReady={fontsReady} />
        <div className="contents max-md:block max-md:-order-1"><PreviewPanel /></div>
        <InspectorPanel />
      </div>
      <TimelinePanel />
      <DragGhost />
    </div>
  );
}
