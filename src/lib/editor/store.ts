"use client";

import { create } from "zustand";
import { toast } from "sonner";
import { ASSETS, type AssetType, type TextProps } from "./assets";
import { transitionDef } from "./fx/transitions";
import { effectDef, makeClipEffect, type ClipEffect, type Params } from "./fx/registry";
import { FPS, snapFrame, tc } from "./time";

export type TrackKind = "text" | "overlay" | "video" | "audio";
export interface Track { id: string; name: string; kind: TrackKind; hidden: boolean; muted: boolean; locked: boolean }
export interface Clip {
  id: string; asset: string; type: AssetType; name: string; track: string;
  start: number; dur: number; in: number;
  x: number; y: number; scale: number; rot: number; opacity: number;
  /** Transition into this clip from the clip before it (or from what's below when there is none). */
  transition?: { id: string; dur: number };
  /** Where effects apply. Unset means automatic: the clip's own shape for text and placed clips, the frame otherwise. */
  fxScope?: "clip" | "frame";
  effects: ClipEffect[]; fadeIn: number; fadeOut: number; volume: number;
  text?: TextProps;
}

export const TRACK_KINDS: Record<TrackKind, AssetType[]> = {
  text: ["text"],
  overlay: ["overlay", "image", "photo", "video", "scene"],
  video: ["scene", "video", "image", "photo"],
  audio: ["music", "audio"],
};
export const DEFAULT_TRACK: Record<AssetType, string> = { text: "T1", overlay: "V2", scene: "V1", video: "V1", image: "V1", photo: "V1", music: "A1", audio: "A2" };
export const TRACK_HEIGHT: Record<TrackKind, number> = { text: 44, overlay: 48, video: 64, audio: 52 };

interface Snapshot { clips: Clip[]; selected: string | null }

export interface EditorState {
  name: string;
  time: number;
  playing: boolean;
  zoom: number;
  snap: boolean;
  loop: boolean;
  guides: boolean;
  /** insert: make room by moving clips. overwrite: cut into whatever is underneath. */
  editMode: "insert" | "overwrite";
  selected: string | null;
  tracks: Track[];
  clips: Clip[];
  pool: string[];
  past: Snapshot[];
  future: Snapshot[];
  editing: boolean;

  set: (patch: Partial<EditorState>) => void;
  commit: () => void;
  beginEdit: () => void;
  endEdit: () => void;
  undo: () => void;
  redo: () => void;
  select: (id: string | null) => void;
  addClip: (assetId: string, trackId?: string, start?: number) => Clip | null;
  updateClip: (id: string, patch: Partial<Clip>) => void;
  updateText: (id: string, patch: Partial<TextProps>) => void;
  split: () => void;
  remove: () => void;
  duplicate: () => void;
  applyEffect: (effectId: string, clipId?: string) => void;
  updateEffect: (clipId: string, uid: string, patch: Partial<Omit<ClipEffect, "params">> & { params?: Params }) => void;
  removeEffect: (clipId: string, uid: string) => void;
  moveEffect: (clipId: string, uid: string, dir: -1 | 1) => void;
  /** "fadein" | "fadeout" | "both" set clip fades; any other id is a transition into the clip. */
  applyTransition: (id: string, clipId?: string) => void;
  toggleTrack: (id: string, key: "hidden" | "muted" | "locked") => void;
  setZoom: (z: number) => void;
  /** Overwrite whatever the clip now covers on its track (no new history entry). */
  settle: (id: string, snapIntoPlace?: boolean) => void;
  clipboard: Clip | null;
  copy: () => Clip | null;
  cut: () => void;
  paste: (clip?: Clip, at?: number) => void;
}

const fitFades = (c: Clip): Clip => ({ ...c, fadeIn: Math.min(c.fadeIn, c.dur / 2), fadeOut: Math.min(c.fadeOut, c.dur / 2) });
const EPS = 1e-6;

/**
 * Overwrite edit: clips on the same track under `id` are trimmed, split or removed
 * so nothing overlaps it. This is what NLEs do when you drop a clip onto another.
 */
export function resolveOverlaps(clips: Clip[], id: string): Clip[] {
  const m = clips.find((c) => c.id === id);
  if (!m) return clips;
  const s = m.start, e = m.start + m.dur, out: Clip[] = [];
  for (const c of clips) {
    const cs = c.start, ce = c.start + c.dur;
    if (c.id === id || c.track !== m.track || ce <= s + EPS || cs >= e - EPS) { out.push(c); continue; }
    if (cs >= s - EPS && ce <= e + EPS) continue; // fully covered
    if (cs < s && ce > e) { // covered in the middle: keep both ends
      out.push(fitFades({ ...c, dur: s - cs, fadeOut: 0 }));
      out.push(fitFades({ ...clone(c), id: nextId(), start: e, in: c.in + (e - cs), dur: ce - e, fadeIn: 0 }));
    } else if (cs < s) out.push(fitFades({ ...c, dur: s - cs }));
    else out.push(fitFades({ ...c, start: e, in: c.in + (e - cs), dur: ce - e }));
  }
  return out;
}

/**
 * Insert edit: make room for `id` without cutting anything.
 * With `snapIntoPlace`, a clip dropped inside another lands before it (first half) or after it
 * (second half). Clips it would overlap shift right, as little as needed; gaps absorb the shift.
 * Returns the new clip list and how many clips moved.
 */
export function pushOverlaps(clips: Clip[], id: string, snapIntoPlace = true): { clips: Clip[]; moved: number } {
  const m0 = clips.find((c) => c.id === id);
  if (!m0) return { clips, moved: 0 };
  const m = { ...m0 };
  const others = clips.filter((c) => c.track === m.track && c.id !== id).sort((a, b) => a.start - b.start);
  if (snapIntoPlace) {
    const host = others.find((k) => m.start > k.start + EPS && m.start < k.start + k.dur - EPS);
    if (host) m.start = m.start - host.start < host.dur / 2 ? host.start : host.start + host.dur;
  }
  const shifted = new Map<string, number>();
  let cursor = m.start + m.dur;
  for (const k of others) {
    if (k.start + k.dur <= m.start + EPS) continue; // entirely before the clip
    if (k.start < m.start - EPS) { cursor = Math.max(cursor, k.start + k.dur); continue; } // started earlier; leave it
    if (k.start < cursor - EPS) { shifted.set(k.id, cursor); cursor += k.dur; }
    else cursor = k.start + k.dur;
  }
  return {
    clips: clips.map((c) => (c.id === id ? m : shifted.has(c.id) ? { ...c, start: snapFrame(shifted.get(c.id)!) } : c)),
    moved: shifted.size,
  };
}

/** First start time at or after `from` where a clip of `dur` fits on `track` without overlapping. */
export function freeSlot(clips: Clip[], track: string, from: number, dur: number) {
  let start = from;
  for (let moved = true; moved; ) {
    moved = false;
    for (const k of clips) {
      if (k.track === track && k.start < start + dur - EPS && k.start + k.dur > start + EPS) { start = k.start + k.dur; moved = true; }
    }
  }
  return start;
}

let seq = 1;
const nextId = () => "c" + seq++;
/** Keep new clip ids above the ones in a loaded project. */
export function syncClipSeq(clips: Clip[]) {
  for (const c of clips) { const n = parseInt(c.id.slice(1), 10); if (n >= seq) seq = n + 1; }
}

export const DEFAULT_TRACKS: Track[] = [
  { id: "T1", name: "Titles", kind: "text", hidden: false, muted: false, locked: false },
  { id: "V2", name: "Overlays", kind: "overlay", hidden: false, muted: false, locked: false },
  { id: "V1", name: "Main video", kind: "video", hidden: false, muted: false, locked: false },
  { id: "A1", name: "Music", kind: "audio", hidden: false, muted: false, locked: false },
  { id: "A2", name: "Voice & SFX", kind: "audio", hidden: false, muted: false, locked: false },
];
const clone = <T,>(v: T): T => JSON.parse(JSON.stringify(v));

export const trackOf = (s: Pick<EditorState, "tracks">, id: string) => s.tracks.find((t) => t.id === id)!;
export const accepts = (s: Pick<EditorState, "tracks">, trackId: string, type: AssetType) => {
  const t = s.tracks.find((x) => x.id === trackId);
  return !!t && TRACK_KINDS[t.kind].includes(type);
};
export const projectEnd = (clips: Clip[]) => clips.reduce((m, c) => Math.max(m, c.start + c.dur), 0);

export function makeClip(s: Pick<EditorState, "tracks" | "time">, assetId: string, trackId?: string, start?: number): Clip | null {
  const a = ASSETS.get(assetId);
  if (!a) return null;
  const track = trackId && accepts(s, trackId, a.type) ? trackId : DEFAULT_TRACK[a.type];
  return {
    id: nextId(), asset: assetId, type: a.type, name: a.name, track,
    start: snapFrame(Math.max(0, start ?? s.time)), dur: a.type === "music" ? Math.min(a.dur, 24) : a.dur, in: 0,
    x: 0, y: 0, scale: 1, rot: 0, opacity: a.type === "overlay" && a.blend !== "source-over" ? 0.7 : 1,
    effects: [], fadeIn: 0, fadeOut: 0, volume: a.type === "music" ? 0.7 : 1,
    text: a.text ? clone(a.text) : undefined,
  };
}

export const useEditor = create<EditorState>()((set, get) => {
  const snapshot = (): Snapshot => ({ clips: clone(get().clips), selected: get().selected });
  // Fit a clip into its track according to the edit mode, and say what moved.
  const place = (clips: Clip[], id: string, snapIntoPlace = true, announce = true): Clip[] => {
    if (get().editMode === "overwrite") return resolveOverlaps(clips, id);
    const r = pushOverlaps(clips, id, snapIntoPlace);
    if (announce && r.moved) toast(r.moved === 1 ? "Moved 1 clip along to make room" : `Moved ${r.moved} clips along to make room`);
    return r.clips;
  };
  const withHistory = (fn: (s: EditorState) => Partial<EditorState>) => {
    const s = get();
    set({ past: [...s.past.slice(-119), snapshot()], future: [], ...fn(s) });
  };
  const locked = (trackId: string) => {
    const t = trackOf(get(), trackId);
    if (t.locked) toast(`${t.name} is locked. Unlock it to edit.`);
    return t.locked;
  };

  return {
    name: "Lisbon summer reel",
    time: 0, playing: false, zoom: 60, snap: true, loop: false, guides: false, editMode: "insert",
    selected: null,
    tracks: clone(DEFAULT_TRACKS),
    clips: [], pool: [], past: [], future: [], editing: false,

    set: (patch) => set(patch),
    commit: () => set((s) => ({ past: [...s.past.slice(-119), snapshot()], future: [] })),
    beginEdit: () => { if (!get().editing) set((s) => ({ editing: true, past: [...s.past.slice(-119), snapshot()], future: [] })); },
    endEdit: () => set({ editing: false }),
    undo: () => {
      const s = get(); const prev = s.past.at(-1); if (!prev) return;
      set({ past: s.past.slice(0, -1), future: [...s.future, snapshot()], ...prev });
      toast("Undone");
    },
    redo: () => {
      const s = get(); const next = s.future.at(-1); if (!next) return;
      set({ future: s.future.slice(0, -1), past: [...s.past, snapshot()], ...next });
      toast("Redone");
    },
    select: (id) => set({ selected: id }),

    addClip: (assetId, trackId, start) => {
      const c = makeClip(get(), assetId, trackId, start);
      if (!c || locked(c.track)) return null;
      withHistory((s) => ({ clips: place([...s.clips, c], c.id), selected: c.id }));
      return get().clips.find((k) => k.id === c.id) ?? c;
    },
    updateClip: (id, patch) => set((s) => ({ clips: s.clips.map((c) => (c.id === id ? { ...c, ...patch } : c)) })),
    updateText: (id, patch) => set((s) => ({ clips: s.clips.map((c) => (c.id === id && c.text ? { ...c, text: { ...c.text, ...patch } } : c)) })),

    split: () => {
      const s = get(); const t = snapFrame(s.time);
      const inside = (c: Clip) => t > c.start && t < c.start + c.dur;
      let c = s.clips.find((k) => k.id === s.selected);
      if (!c || !inside(c)) c = s.clips.find((k) => k.track === "V1" && inside(k));
      if (!c) { toast("Move the playhead over a clip to split it"); return; }
      if (locked(c.track)) return;
      const cut = t - c.start, target = c;
      const b: Clip = { ...clone(c), id: nextId(), start: t, in: c.in + cut, dur: c.dur - cut, fadeIn: 0, transition: undefined };
      if (b.text) b.text.anim = "none";
      withHistory((st) => ({ clips: [...st.clips.map((k) => (k.id === target.id ? { ...k, dur: cut, fadeOut: 0 } : k)), b], selected: b.id }));
      toast(`Split at ${tc(t)}`);
    },
    remove: () => {
      const s = get(); const c = s.clips.find((k) => k.id === s.selected);
      if (!c || locked(c.track)) return;
      withHistory((st) => ({ clips: st.clips.filter((k) => k.id !== c.id), selected: null }));
    },
    duplicate: () => {
      const s = get(); const c = s.clips.find((k) => k.id === s.selected); if (!c) return;
      const d = { ...clone(c), id: nextId(), start: s.editMode === "insert" ? c.start + c.dur : freeSlot(s.clips, c.track, c.start + c.dur, c.dur) };
      withHistory((st) => ({ clips: place([...st.clips, d], d.id, false, false), selected: d.id }));
      toast(`Duplicated to ${tc(d.start)}`);
    },
    settle: (id, snapIntoPlace = true) => set((s) => ({ clips: place(s.clips, id, snapIntoPlace) })),
    clipboard: null,
    copy: () => {
      const s = get(); const c = s.clips.find((k) => k.id === s.selected);
      if (!c) { toast("Select a clip to copy"); return null; }
      set({ clipboard: clone(c) }); toast(`Copied ${c.name}`);
      return c;
    },
    cut: () => {
      const s = get(); const c = s.clips.find((k) => k.id === s.selected);
      if (!c || locked(c.track)) return;
      set({ clipboard: clone(c) });
      withHistory((st) => ({ clips: st.clips.filter((k) => k.id !== c.id), selected: null }));
      toast(`Cut ${c.name}`);
    },
    paste: (clip, at) => {
      const s = get(); const src = clip ?? s.clipboard;
      if (!src) { toast("Nothing to paste. Copy a clip first."); return; }
      if (!ASSETS.has(src.asset)) { toast("That clip's media isn't in this project. Import the file, then paste again."); return; }
      if (locked(src.track)) return;
      const n: Clip = { ...clone(src), effects: clone(src.effects ?? []), id: nextId(), start: snapFrame(Math.max(0, at ?? s.time)) };
      withHistory((st) => ({ clips: place([...st.clips, n], n.id), selected: n.id }));
      toast(`Pasted ${n.name} at ${tc(get().clips.find((k) => k.id === n.id)?.start ?? n.start)}`);
    },
    applyEffect: (effectId, clipId) => {
      const s = get(); const c = s.clips.find((k) => k.id === (clipId ?? s.selected));
      if (!c || c.type === "music" || c.type === "audio") { toast("Select a video, image, overlay or text clip first"); return; }
      const fx = makeClipEffect(effectId); if (!fx) return;
      withHistory((st) => ({ selected: c.id, clips: st.clips.map((k) => (k.id === c.id ? { ...k, effects: [...k.effects, fx] } : k)) }));
      toast(`${effectDef(effectId)?.name} added to ${c.name}`);
    },
    updateEffect: (clipId, uid, patch) => set((s) => ({ clips: s.clips.map((c) => c.id !== clipId ? c : {
      ...c, effects: c.effects.map((e) => (e.uid !== uid ? e : { ...e, ...patch, params: patch.params ? { ...e.params, ...patch.params } : e.params })),
    }) })),
    removeEffect: (clipId, uid) => withHistory((s) => ({ clips: s.clips.map((c) => (c.id !== clipId ? c : { ...c, effects: c.effects.filter((e) => e.uid !== uid) })) })),
    moveEffect: (clipId, uid, dir) => withHistory((s) => ({ clips: s.clips.map((c) => {
      if (c.id !== clipId) return c;
      const i = c.effects.findIndex((e) => e.uid === uid), j = i + dir;
      if (i < 0 || j < 0 || j >= c.effects.length) return c;
      const effects = [...c.effects]; [effects[i], effects[j]] = [effects[j], effects[i]];
      return { ...c, effects };
    }) })),
    applyTransition: (id, clipId) => {
      const s = get(); const c = s.clips.find((k) => k.id === (clipId ?? s.selected));
      if (!c) { toast("Select a clip first"); return; }
      if (id === "fadein" || id === "fadeout" || id === "both") {
        const d = Math.min(0.5, c.dur / 2);
        withHistory((st) => ({ selected: c.id, clips: st.clips.map((k) => k.id !== c.id ? k : {
          ...k, fadeIn: id !== "fadeout" ? d : k.fadeIn, fadeOut: id !== "fadein" ? d : k.fadeOut,
        }) }));
        toast(`Fade added to ${c.name}`);
        return;
      }
      const def = transitionDef(id); if (!def) return;
      if (c.type === "music" || c.type === "audio") { toast("Transitions are for video, image, overlay and text clips. Use a fade on audio."); return; }
      const dur = Math.min(c.transition?.dur ?? 0.8, c.dur);
      withHistory((st) => ({ selected: c.id, clips: st.clips.map((k) => (k.id !== c.id ? k : { ...k, transition: { id, dur } })) }));
      toast(`${def.name} transition added to the start of ${c.name}`);
    },
    toggleTrack: (id, key) => set((s) => ({ tracks: s.tracks.map((t) => (t.id === id ? { ...t, [key]: !t[key] } : t)) })),
    setZoom: (z) => set({ zoom: Math.max(12, Math.min(240, z)) }),
  };
});

export const MIN_CLIP = 3 / FPS;

/** The "Lisbon summer reel" sample, used as a project template. */
export function sampleClips(): Clip[] {
  const s = { tracks: DEFAULT_TRACKS, time: 0 };
  const clips: Clip[] = [];
  const put = (asset: string, track: string, start: number, dur: number, o: Partial<Clip> = {}) => {
    const c = makeClip(s, asset, track, start)!; Object.assign(c, { dur }, o); clips.push(c);
  };
  put("sc-coast", "V1", 0, 6, { fadeIn: 0.5 });
  put("sc-drive", "V1", 6, 5, { effects: [makeClipEffect("cool", 0.8)!], transition: { id: "whip", dur: 0.6 } });
  put("sc-aurora", "V1", 11, 6, { effects: [makeClipEffect("vivid", 0.6)!], transition: { id: "dissolve", dur: 1 } });
  put("sc-studio", "V1", 17, 4, { fadeOut: 0.8, effects: [makeClipEffect("ascii-color")!], transition: { id: "glitch", dur: 0.5 } });
  put("ov-leak", "V2", 0, 6, { opacity: 0.55, fadeOut: 1 });
  put("ov-grain", "V2", 6, 15, { opacity: 0.45 });
  put("tx-title", "T1", 0.5, 4, { fadeOut: 0.4 });
  put("tx-lower", "T1", 6.5, 4, { fadeOut: 0.3 });
  put("tx-quote", "T1", 12, 4, { fadeOut: 0.5 });
  put("tx-poster", "T1", 17.5, 3, { fadeOut: 0.3 });
  put("mu-golden", "A1", 0, 21, { fadeOut: 1.5 });
  return clips;
}

/** Whether a clip's effects stay on its own shape rather than the whole frame. */
export function effectsOnObject(c: Pick<Clip, "fxScope" | "type" | "scale" | "x" | "y" | "rot">) {
  if (c.fxScope) return c.fxScope === "clip";
  return c.type === "text" || c.scale < 0.999 || c.x !== 0 || c.y !== 0 || c.rot !== 0;
}
