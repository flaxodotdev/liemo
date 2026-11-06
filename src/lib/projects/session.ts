"use client";

// Connects the editor store to project storage: creating projects, opening one into the
// editor, and saving it automatically while you work.
import { create } from "zustand";
import { DEFAULT_TRACKS, projectEnd, sampleClips, syncClipSeq, useEditor } from "@/lib/editor/store";
import { resetMedia, restoreMedia, stageSnapshot } from "@/lib/editor/render";
import { getMedia, getProject, newId, putProject, type ProjectRecord } from "./db";

export type Template = "blank" | "sample";

export async function createProject(template: Template, name?: string): Promise<string> {
  const clips = template === "sample" ? sampleClips() : [];
  const now = Date.now();
  const p: ProjectRecord = {
    id: newId(),
    name: name ?? (template === "sample" ? "Lisbon summer reel" : "Untitled project"),
    createdAt: now, updatedAt: now,
    duration: projectEnd(clips), clipCount: clips.length,
    clips, tracks: JSON.parse(JSON.stringify(DEFAULT_TRACKS)), pool: [],
  };
  await putProject(p);
  return p.id;
}

// ---------- Save status, shown in the editor's top bar ----------
type SaveState = "idle" | "saving" | "saved" | "error";
export const useSaveStatus = create<{ state: SaveState; at: number; projectId: string | null }>(() => ({ state: "idle", at: 0, projectId: null }));

let current: ProjectRecord | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
let pending = false;
let unsubscribe: (() => void) | null = null;
let openToken = 0;

/** Load a project into the editor. Resolves false when it doesn't exist. */
export async function openProject(id: string): Promise<boolean> {
  closeProject();
  const token = ++openToken;
  const p = await getProject(id);
  if (token !== openToken) return true; // a newer open took over
  if (!p) return false;

  // Media first, so clips that use it can draw straight away.
  const media = await Promise.all(p.pool.map((mid) => getMedia(mid).catch(() => undefined)));
  if (token !== openToken) return true;
  current = p;
  resetMedia();
  media.forEach((m) => m && restoreMedia(m));
  const pool = media.filter(Boolean).map((m) => m!.id);

  syncClipSeq(p.clips);
  useEditor.setState({
    name: p.name, clips: p.clips, tracks: p.tracks?.length ? p.tracks : JSON.parse(JSON.stringify(DEFAULT_TRACKS)),
    pool, selected: null, time: 0, playing: false, past: [], future: [], editing: false,
  });
  useSaveStatus.setState({ state: "saved", at: p.updatedAt, projectId: id });

  unsubscribe = useEditor.subscribe((s, prev) => {
    if (s.clips !== prev.clips || s.tracks !== prev.tracks || s.name !== prev.name || s.pool !== prev.pool) scheduleSave();
  });
  // New projects have no card thumbnail yet; make one once fonts and media have had a moment to load.
  if (!p.thumb && p.clips.length) setTimeout(() => { if (token === openToken) void saveNow(); }, 1500);
  return true;
}

function scheduleSave() {
  useSaveStatus.setState({ state: "saving" });
  pending = true;
  clearTimeout(timer);
  timer = setTimeout(saveNow, 600);
}

export async function saveNow() {
  clearTimeout(timer);
  pending = false;
  const base = current;
  if (!base) return;
  const s = useEditor.getState();
  const record: ProjectRecord = {
    ...base,
    name: s.name.trim() || "Untitled project",
    clips: s.clips, tracks: s.tracks, pool: s.pool,
    duration: projectEnd(s.clips), clipCount: s.clips.length,
    thumb: s.clips.length ? stageSnapshot() ?? base.thumb : undefined,
    updatedAt: Date.now(),
  };
  try {
    await putProject(record);
    if (current?.id === record.id) { current = record; useSaveStatus.setState({ state: "saved", at: record.updatedAt }); }
  } catch {
    useSaveStatus.setState({ state: "error" });
  }
}

/** Flush pending changes and detach from the editor. */
export function closeProject() {
  openToken++;
  if (pending && current) void saveNow();
  unsubscribe?.(); unsubscribe = null;
  current = null;
  useSaveStatus.setState({ state: "idle", projectId: null });
}

export function relativeTime(ts: number, now = Date.now()) {
  const s = Math.round((now - ts) / 1000);
  if (s < 45) return "just now";
  const m = Math.round(s / 60); if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60); if (h < 24) return `${h} hr ago`;
  const d = Math.round(h / 24); if (d < 7) return d === 1 ? "yesterday" : `${d} days ago`;
  return new Date(ts).toLocaleDateString(undefined, { day: "numeric", month: "short", year: new Date(ts).getFullYear() === new Date(now).getFullYear() ? undefined : "numeric" });
}

// Files picked on the home screen ("Start from a video") are handed to the editor once it opens.
let pendingFiles: File[] | null = null;
export function setPendingImport(files: FileList | File[]) { pendingFiles = [...files]; }
export function takePendingImport() { const f = pendingFiles; pendingFiles = null; return f; }
