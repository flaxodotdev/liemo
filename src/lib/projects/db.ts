"use client";

// Projects live in the browser's IndexedDB: one record per project, plus the imported media
// files (as Blobs) so a project reopens with its footage, photos and audio intact.
import type { AssetType } from "@/lib/editor/assets";
import type { Clip, Track } from "@/lib/editor/store";

export interface ProjectRecord {
  id: string;
  name: string;
  createdAt: number;
  updatedAt: number;
  duration: number;
  clipCount: number;
  thumb?: string;
  clips: Clip[];
  tracks: Track[];
  /** Imported media this project uses (ids into the media store). */
  pool: string[];
}
export type ProjectSummary = Omit<ProjectRecord, "clips" | "tracks" | "pool">;

export interface MediaRecord {
  id: string;
  type: Extract<AssetType, "video" | "audio" | "photo">;
  name: string;
  sub: string;
  dur: number;
  thumb?: string;
  blob: Blob;
}

const DB_NAME = "rayna";
const VERSION = 1;
let dbPromise: Promise<IDBDatabase> | null = null;

function open(): Promise<IDBDatabase> {
  if (dbPromise) return dbPromise;
  dbPromise = new Promise((resolve, reject) => {
    if (typeof indexedDB === "undefined") return reject(new Error("This browser can't store projects (IndexedDB is unavailable)."));
    const req = indexedDB.open(DB_NAME, VERSION);
    req.onupgradeneeded = () => {
      const db = req.result;
      if (!db.objectStoreNames.contains("projects")) db.createObjectStore("projects", { keyPath: "id" });
      if (!db.objectStoreNames.contains("media")) db.createObjectStore("media", { keyPath: "id" });
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => { dbPromise = null; reject(req.error ?? new Error("Couldn't open project storage.")); };
    req.onblocked = () => reject(new Error("Project storage is busy in another tab. Close other Liemo tabs and try again."));
  });
  return dbPromise;
}

function run<T>(store: "projects" | "media", mode: IDBTransactionMode, fn: (s: IDBObjectStore) => IDBRequest<T> | void): Promise<T> {
  return open().then((db) => new Promise<T>((resolve, reject) => {
    const tx = db.transaction(store, mode), s = tx.objectStore(store);
    const req = fn(s);
    let result: T;
    if (req) req.onsuccess = () => { result = req.result; };
    tx.oncomplete = () => resolve(result);
    tx.onerror = () => reject(tx.error ?? new Error("Storage error"));
    tx.onabort = () => reject(tx.error ?? new Error("Storage was interrupted. This can happen when the disk is full."));
  }));
}

export const newId = () => Date.now().toString(36) + Math.random().toString(36).slice(2, 8);

// ---------- Projects ----------
export async function listProjects(): Promise<ProjectSummary[]> {
  const all = await run<ProjectRecord[]>("projects", "readonly", (s) => s.getAll());
  return (all ?? []).map((p) => ({
    id: p.id, name: p.name, createdAt: p.createdAt, updatedAt: p.updatedAt,
    duration: p.duration, clipCount: p.clipCount, thumb: p.thumb,
  }));
}
export const getProject = (id: string) => run<ProjectRecord | undefined>("projects", "readonly", (s) => s.get(id));
export const putProject = (p: ProjectRecord) => run("projects", "readwrite", (s) => s.put(p));

export async function deleteProject(id: string) {
  const p = await getProject(id);
  await run("projects", "readwrite", (s) => s.delete(id));
  if (p?.pool.length) await collectMedia(p.pool);
}
export async function renameProject(id: string, name: string) {
  const p = await getProject(id); if (!p) return;
  await putProject({ ...p, name, updatedAt: Date.now() });
}
export async function duplicateProject(id: string): Promise<string | null> {
  const p = await getProject(id); if (!p) return null;
  const copy: ProjectRecord = { ...p, id: newId(), name: `${p.name} copy`, createdAt: Date.now(), updatedAt: Date.now() };
  await putProject(copy); // media records are shared by id, so nothing to copy
  return copy.id;
}

// ---------- Media ----------
export const putMedia = (m: MediaRecord) => run("media", "readwrite", (s) => s.put(m));
export const getMedia = (id: string) => run<MediaRecord | undefined>("media", "readonly", (s) => s.get(id));

/** Delete media files no remaining project uses. */
async function collectMedia(candidates: string[]) {
  const all = await run<ProjectRecord[]>("projects", "readonly", (s) => s.getAll());
  const used = new Set((all ?? []).flatMap((p) => p.pool));
  const orphans = candidates.filter((id) => !used.has(id));
  if (orphans.length) await run("media", "readwrite", (s) => { orphans.forEach((id) => s.delete(id)); });
}
