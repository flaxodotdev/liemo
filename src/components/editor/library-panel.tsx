"use client";

import * as React from "react";
import { Film, Music2, Image as ImageIcon, Layers, Type, Sparkles, Search, Plus, Play, Pause, Upload, AudioLines, Blend } from "lucide-react";
import { toast } from "sonner";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Panel, PanelHeader, SectionLabel } from "./primitives";
import { cn } from "@/lib/utils";
import { ASSETS, FONTS, IMAGES, MUSIC, OVERLAYS, SCENES, TEXT_PRESETS, TRANSITIONS as FADES, type Asset } from "@/lib/editor/assets";
import { assetThumb, effectThumb, importFiles, transitionThumb } from "@/lib/editor/render";
import { previewMusic } from "@/lib/editor/playback";
import { useEditor } from "@/lib/editor/store";
import { EFFECTS, EFFECT_GROUPS } from "@/lib/editor/fx/registry";
import { TRANSITIONS, TRANSITION_GROUPS } from "@/lib/editor/fx/transitions";
import { startLibraryDrag, type DragPayload } from "@/lib/editor/dnd";
import { shortTime, tc } from "@/lib/editor/time";

const TABS = [
  { id: "media", label: "Media", icon: Film },
  { id: "music", label: "Music", icon: Music2 },
  { id: "images", label: "Images", icon: ImageIcon },
  { id: "overlays", label: "Overlays", icon: Layers },
  { id: "text", label: "Text", icon: Type },
  { id: "effects", label: "Effects", icon: Sparkles },
  { id: "transitions", label: "Transitions", icon: Blend },
] as const;

export function LibraryPanel({ fontsReady }: { fontsReady: boolean }) {
  const [tab, setTab] = React.useState<string>("media");
  const [query, setQuery] = React.useState("");
  const pool = useEditor((s) => s.pool);
  const q = query.trim().toLowerCase();
  const match = (a: { name: string; sub?: string }) => !q || `${a.name} ${a.sub ?? ""}`.toLowerCase().includes(q);
  const mine = pool.map((id) => ASSETS.get(id)!).filter((a) => a && match(a));

  React.useEffect(() => {
    try { const t = localStorage.getItem("rayna-tab"); if (t) setTab(t); } catch {}
  }, []);
  const changeTab = (t: string) => { setTab(t); try { localStorage.setItem("rayna-tab", t); } catch {} };

  return (
    <Panel aria-label="Library" className="flex-row">
      <Tabs value={tab} onValueChange={changeTab} orientation="vertical" className="flex min-w-0 flex-1 flex-row gap-0">
        <TabsList variant="line" className="h-full w-[72px] shrink-0 justify-start gap-0.5 overflow-y-auto rounded-none border-r border-hairline-soft px-1.5 py-2 [scrollbar-width:none]">
          {TABS.map(({ id, label, icon: Icon }) => (
            <TabsTrigger key={id} value={id}
              className="h-auto w-full flex-none flex-col justify-center! gap-0.5 rounded-md py-1.5 type-micro text-[10.5px] font-medium after:hidden data-active:bg-surface-3! data-active:border-transparent! hover:bg-surface-2">
              <Icon className="size-4!" />{label}
            </TabsTrigger>
          ))}
        </TabsList>

        <div className="flex min-w-0 flex-1 flex-col">
          <PanelHeader className="px-3">
            <div className="relative flex-1">
              <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-ink-muted" />
              <Input id="lib-search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search library" aria-label="Search library" className="h-8 rounded-md bg-canvas pl-8 type-caption" />
            </div>
          </PanelHeader>

          <TabsContent value="media" className="min-h-0">
            <Scroll>
              <Section title="Media pool">
                <DropZone what="video, audio or images" />
                {mine.filter((a) => a.type !== "audio").length > 0
                  ? <Grid>{mine.filter((a) => a.type !== "audio").map((a) => <AssetCard key={a.id} asset={a} showDur={a.type === "video"} />)}</Grid>
                  : <p className="type-micro text-ink-muted">Imported files stay in this browser tab. Nothing is uploaded.</p>}
              </Section>
              <Section title="Sample footage"><Grid>{SCENES.filter(match).map((a) => <AssetCard key={a.id} asset={a} showDur />)}</Grid></Section>
            </Scroll>
          </TabsContent>

          <TabsContent value="music" className="min-h-0">
            <Scroll>
              <Section title="Music library"><Rows>{MUSIC.filter(match).map((a) => <MusicRow key={a.id} asset={a} />)}</Rows></Section>
              <Section title="Imported audio">
                {mine.some((a) => a.type === "audio")
                  ? <Rows>{mine.filter((a) => a.type === "audio").map((a) => <MusicRow key={a.id} asset={a} imported />)}</Rows>
                  : <DropZone what="audio files" />}
              </Section>
            </Scroll>
          </TabsContent>

          <TabsContent value="images" className="min-h-0">
            <Scroll>
              {mine.some((a) => a.type === "photo") && <Section title="Your images"><Grid>{mine.filter((a) => a.type === "photo").map((a) => <AssetCard key={a.id} asset={a} />)}</Grid></Section>}
              <Section title="Stills & backgrounds"><Grid>{IMAGES.filter(match).map((a) => <AssetCard key={a.id} asset={a} />)}</Grid></Section>
              {!mine.some((a) => a.type === "photo") && <Section title="Import images"><DropZone what="PNG, JPG or WebP files" /></Section>}
            </Scroll>
          </TabsContent>

          <TabsContent value="overlays" className="min-h-0">
            <Scroll>
              <Section title="Overlays"><Grid>{OVERLAYS.filter(match).map((a) => <AssetCard key={a.id} asset={a} />)}</Grid></Section>
              <p className="px-3 type-micro text-ink-muted">Overlays go on the Overlays track above your footage and blend with it. Lower the opacity in the inspector to soften them.</p>
            </Scroll>
          </TabsContent>

          <TabsContent value="text" className="min-h-0">
            <Scroll>
              <Section title="Text styles"><Grid>{TEXT_PRESETS.filter(match).map((a) => <AssetCard key={a.id + fontsReady} asset={a} showDur />)}</Grid></Section>
              <Section title="Fonts" action="Click to apply"><Rows>{FONTS.filter((f) => match({ name: f.family, sub: f.role })).map((f) => <FontCard key={f.family} family={f.family} />)}</Rows></Section>
            </Scroll>
          </TabsContent>

          <TabsContent value="effects" className="min-h-0">
            <Scroll>
              <p className="px-3 pt-3 type-micro text-ink-muted">Click an effect to add it to the selected clip, or drag it onto any clip. Effects stack, and every setting can be changed in the inspector.</p>
              {EFFECT_GROUPS.map(([group, title]) => {
                const list = EFFECTS.filter((e) => e.group === group && match(e));
                return list.length ? (
                  <Section key={group} title={title}><Grid>{list.map((e) => (
                    <FxCard key={e.id} payload={{ kind: "effect", id: e.id, label: e.name }} thumb={() => effectThumb(e.id)}
                      onActivate={() => useEditor.getState().applyEffect(e.id)} />
                  ))}</Grid></Section>
                ) : null;
              })}
            </Scroll>
          </TabsContent>

          <TabsContent value="transitions" className="min-h-0">
            <Scroll>
              <p className="px-3 pt-3 type-micro text-ink-muted">A transition goes on the start of a clip. Between two touching clips it blends across the cut. On a clip with nothing before it, the clip transitions in. Click to add to the selected clip, or drag onto one.</p>
              {TRANSITION_GROUPS.map(([group, title]) => {
                const list = TRANSITIONS.filter((t) => t.group === group && match(t));
                return list.length ? (
                  <Section key={group} title={title}><Grid>{list.map((t) => (
                    <FxCard key={t.id} payload={{ kind: "transition", id: t.id, label: t.name }} thumb={() => transitionThumb(t.id)}
                      onActivate={() => useEditor.getState().applyTransition(t.id)} />
                  ))}</Grid></Section>
                ) : null;
              })}
              <Section title="Fades"><Rows>{FADES.filter(match).map((t) => (
                <DraggableRow key={t.id} payload={{ kind: "transition", id: t.id, label: t.name }} onActivate={() => useEditor.getState().applyTransition(t.id)}>
                  <Art className="bg-surface-3 text-ink-muted"><Blend className="size-4" /></Art>
                  <RowText name={t.name} sub={t.sub} />
                </DraggableRow>
              ))}</Rows></Section>
            </Scroll>
          </TabsContent>
        </div>
      </Tabs>
    </Panel>
  );
}

// ---------- Layout pieces ----------
const Scroll = ({ children }: { children: React.ReactNode }) => <ScrollArea className="h-full"><div className="pb-3">{children}</div></ScrollArea>;
const Section = ({ title, action, children }: { title: string; action?: string; children: React.ReactNode }) => (
  <div className="flex flex-col gap-2 px-3 pt-3"><SectionLabel action={action}>{title}</SectionLabel>{children}</div>
);
const Grid = ({ children }: { children: React.ReactNode }) => <div className="grid grid-cols-2 gap-2">{children}</div>;
const Rows = ({ children }: { children: React.ReactNode }) => <div className="flex flex-col gap-1">{children}</div>;
const Art = ({ className, style, children }: { className?: string; style?: React.CSSProperties; children: React.ReactNode }) => (
  <span className={cn("grid size-10 shrink-0 place-items-center rounded-sm", className)} style={style}>{children}</span>
);
const RowText = ({ name, sub }: { name: string; sub: string }) => (
  <span className="flex min-w-0 flex-1 flex-col"><span className="truncate type-caption">{name}</span><span className="truncate type-micro text-[11px] text-ink-muted">{sub}</span></span>
);

function dragProps(payload: DragPayload) {
  // Prevent mousedown focus so pressing a card doesn't scroll the list to it.
  return { onPointerDown: (e: React.PointerEvent) => startLibraryDrag(e, payload), onMouseDown: (e: React.MouseEvent) => e.preventDefault() };
}
function addAtPlayhead(id: string) {
  const c = useEditor.getState().addClip(id);
  if (c) toast(`Added ${c.name} at ${tc(c.start)}`);
}

// ---------- Cards ----------
function AssetCard({ asset, showDur }: { asset: Asset; showDur?: boolean }) {
  const [thumb, setThumb] = React.useState<string>();
  React.useEffect(() => setThumb(assetThumb(asset.id, asset.type === "text")), [asset.id, asset.type, asset.thumb]);
  return (
    <div {...dragProps({ kind: "asset", id: asset.id, label: asset.name, thumb })} tabIndex={0} title="Drag to the timeline, or press + to add at the playhead"
      onKeyDown={(e) => e.key === "Enter" && addAtPlayhead(asset.id)}
      className="group relative flex cursor-grab select-none flex-col overflow-hidden rounded-md bg-surface-2 transition-shadow outline-none hover:shadow-[0_0_0_1px_var(--hairline),0_10px_30px_rgb(0_0_0/.35)] focus-visible:ring-2 focus-visible:ring-ring active:cursor-grabbing">
      <div className="relative aspect-video bg-black bg-cover bg-center" style={thumb ? { backgroundImage: `url(${thumb})` } : undefined}>
        {showDur && <span className="absolute right-1.5 bottom-1.5 rounded-xs bg-black/65 px-1.5 py-0.5 type-timecode text-[10px]">{shortTime(asset.dur)}</span>}
      </div>
      <div className="flex min-w-0 flex-col px-2 pt-1.5 pb-2">
        <span className="truncate type-micro font-medium">{asset.name}</span>
        <span className="truncate type-micro text-[11px] text-ink-muted">{asset.sub}</span>
      </div>
      <Button size="icon-xs" aria-label={`Add ${asset.name} at playhead`} onClick={() => addAtPlayhead(asset.id)}
        className="absolute top-1.5 right-1.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100">
        <Plus className="size-3.5" strokeWidth={2.4} />
      </Button>
    </div>
  );
}

function MusicRow({ asset, imported }: { asset: Asset; imported?: boolean }) {
  const [playing, setPlaying] = React.useState(false);
  React.useEffect(() => () => { if (playing) previewMusic(null, () => {}); }, [playing]);
  const toggle = () => {
    if (playing) { previewMusic(null, () => {}); setPlaying(false); }
    else { setPlaying(true); previewMusic(asset.id, () => setPlaying(false)); }
  };
  return (
    <div {...dragProps({ kind: "asset", id: asset.id, label: asset.name })} className="flex cursor-grab select-none items-center gap-2 rounded-md bg-surface-2 p-2 hover:bg-surface-3">
      <Art className={imported ? "bg-surface-3" : undefined} style={imported ? undefined : { background: `color-mix(in oklch, ${asset.hue} 18%, var(--surface-3))`, color: asset.hue }}>
        {imported ? <AudioLines className="size-4" /> : <Music2 className="size-4" />}
      </Art>
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate type-caption">{asset.name}</span>
        <span className="truncate type-micro text-[11px] text-ink-muted">{asset.sub}</span>
        <span className="truncate type-timecode text-[10px] text-ink-muted">{imported ? shortTime(asset.dur) : `${asset.bpm} BPM · ${shortTime(asset.dur)}`}</span>
      </span>
      {!imported && (
        <Button size="icon-sm" variant="ghost" aria-label={`${playing ? "Stop preview of" : "Preview"} ${asset.name}`} onClick={toggle} className="shrink-0">
          {playing ? <Pause className="size-3.5" /> : <Play className="size-3.5" />}
        </Button>
      )}
      <Button size="icon-sm" variant="ghost" aria-label={`Add ${asset.name} at playhead`} onClick={() => addAtPlayhead(asset.id)} className="shrink-0"><Plus className="size-3.5" /></Button>
    </div>
  );
}

function FontCard({ family }: { family: string }) {
  const f = FONTS.find((x) => x.family === family)!;
  const apply = () => {
    const s = useEditor.getState(), c = s.clips.find((k) => k.id === s.selected);
    if (c?.text) { s.commit(); s.updateText(c.id, { font: f.family, weight: f.weight }); toast(`Font set to ${f.family}`); return; }
    const n = s.addClip("tx-title", "T1");
    if (n) { useEditor.getState().updateText(n.id, { font: f.family, weight: f.weight, content: "Golden light" }); toast(`Added a title in ${f.family}`); }
  };
  return (
    <button type="button" onClick={apply} className="flex flex-col gap-1 rounded-md bg-surface-2 p-3 text-left hover:bg-surface-3 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none">
      <span className="truncate text-[26px] leading-[1.05]" style={{ fontFamily: `var(${f.cssVar})`, fontWeight: f.weight, letterSpacing: `${f.track}em` }}>Golden light</span>
      <span className="type-micro text-[11px] text-ink-muted">{f.family} · {f.role}</span>
    </button>
  );
}

function FxCard({ payload, thumb: makeThumb, onActivate }: { payload: DragPayload; thumb: () => string; onActivate: () => void }) {
  const [thumb, setThumb] = React.useState<string>();
  // Thumbnails render on the main thread; spread them over idle frames so opening the tab stays smooth.
  React.useEffect(() => { const id = requestAnimationFrame(() => setThumb(makeThumb())); return () => cancelAnimationFrame(id); }, [payload.id]); // eslint-disable-line react-hooks/exhaustive-deps
  const name = payload.label;
  return (
    <button type="button" {...dragProps({ ...payload, thumb })} onClick={onActivate}
      className="flex flex-col overflow-hidden rounded-md bg-surface-2 text-left transition-shadow hover:shadow-[0_0_0_1px_var(--hairline)] focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none">
      <span className="aspect-[16/10] bg-black bg-cover bg-center" style={thumb ? { backgroundImage: `url(${thumb})` } : undefined} />
      <span className="truncate px-2 pt-1.5 pb-2 type-micro font-medium">{name}</span>
    </button>
  );
}

function DraggableRow({ payload, onActivate, children }: { payload: DragPayload; onActivate: () => void; children: React.ReactNode }) {
  return (
    <button type="button" {...dragProps(payload)} onClick={onActivate}
      className="flex items-center gap-2.5 rounded-md bg-surface-2 p-2 text-left hover:bg-surface-3 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none">
      {children}
    </button>
  );
}

export function DropZone({ what }: { what: string }) {
  const [over, setOver] = React.useState(false);
  const input = React.useRef<HTMLInputElement>(null);
  return (
    <div
      onDragOver={(e) => { if (e.dataTransfer.types.includes("Files")) { e.preventDefault(); setOver(true); } }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => { e.preventDefault(); e.stopPropagation(); setOver(false); importFiles(e.dataTransfer.files); }}
      className={cn("flex flex-col items-center gap-2 rounded-md border border-dashed border-hairline p-4 text-center type-micro text-ink-muted transition-colors",
        over && "border-signal bg-signal/10 text-ink")}>
      <Upload className="size-4" />
      <span>Drop {what} here</span>
      <Button size="sm" onClick={() => input.current?.click()}>Choose files</Button>
      <input ref={input} type="file" accept="video/*,audio/*,image/*" multiple hidden onChange={(e) => { if (e.target.files?.length) importFiles(e.target.files); e.target.value = ""; }} />
    </div>
  );
}
