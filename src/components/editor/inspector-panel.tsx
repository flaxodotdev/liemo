"use client";

import * as React from "react";
import { Copy, Scissors, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Textarea } from "@/components/ui/textarea";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { FieldRow, Group, Kbd, Panel, PanelHeader, SliderField } from "./primitives";
import { EffectStack, TransitionControl } from "./effect-stack";
import { cn } from "@/lib/utils";
import { ASSETS, FONTS, TEXT_ANIMS, TEXT_COLORS, type TextProps } from "@/lib/editor/assets";
import { projectEnd, useEditor, type Clip } from "@/lib/editor/store";
import { tc } from "@/lib/editor/time";

const KIND: Record<Clip["type"], string> = { scene: "Video", video: "Video", image: "Image", photo: "Image", overlay: "Overlay", text: "Text", music: "Music", audio: "Audio" };
const pct = (v: number) => `${Math.round(v * 100)}%`;

export function InspectorPanel() {
  const clip = useEditor((s) => s.clips.find((c) => c.id === s.selected));
  return (
    <Panel aria-label="Inspector" className="inspector">
      <PanelHeader><span className="type-label text-ink-muted">Inspector</span></PanelHeader>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col px-4 pb-4">{clip ? <ClipInspector clip={clip} /> : <ProjectInspector />}</div>
      </ScrollArea>
    </Panel>
  );
}

function ProjectInspector() {
  const name = useEditor((s) => s.name);
  const count = useEditor((s) => s.clips.length);
  const end = useEditor((s) => projectEnd(s.clips));
  return (
    <>
      <Group>
        <h2 className="type-headline break-words">{name || "Untitled edit"}</h2>
        <KV rows={[["Clips", String(count)], ["Length", tc(end)], ["Frame rate", "30 fps"], ["Canvas", "1280 × 720"]]} />
      </Group>
      <Group title="Get started">
        <p className="type-micro text-ink-muted">Drag footage, music, overlays and text from the library onto a track. Click a clip to edit its timing, look and type here.</p>
      </Group>
      <Group title="Shortcuts">
        <div className="grid grid-cols-[auto_1fr] items-center gap-x-3 gap-y-1.5 type-micro text-ink-muted">
          {([["Space", "Play / pause"], ["S", "Split at playhead"], ["⌫", "Delete clip"], ["← →", "Step one frame (⇧ for 10)"], ["⌘Z", "Undo, ⇧⌘Z to redo"], ["⌘C ⌘V", "Copy and paste at playhead"], ["⌘X", "Cut clip"], ["⌥ arrows", "Nudge on screen (⇧ for 10px)"], ["⌘D", "Duplicate clip"], ["+ −", "Zoom timeline"], ["N", "Toggle snapping"]] as const)
            .map(([k, d]) => <React.Fragment key={k}><Kbd>{k}</Kbd><span>{d}</span></React.Fragment>)}
        </div>
      </Group>
    </>
  );
}

function KV({ rows }: { rows: [string, string][] }) {
  return (
    <dl className="grid grid-cols-[1fr_auto] gap-x-3 gap-y-1.5 type-caption">
      {rows.map(([k, v]) => <React.Fragment key={k}><dt className="font-normal text-ink-muted">{k}</dt><dd className="type-timecode text-right text-[12px]">{v}</dd></React.Fragment>)}
    </dl>
  );
}

function ClipInspector({ clip: c }: { clip: Clip }) {
  const { beginEdit, endEdit, updateClip, split, duplicate, remove } = useEditor.getState();
  const a = ASSETS.get(c.asset);
  const visual = c.type !== "music" && c.type !== "audio";
  const maxFade = Math.min(3, +(c.dur / 2).toFixed(2));
  const slider = (label: string, key: keyof Clip, min: number, max: number, step: number, fmt?: (v: number) => string) => (
    <SliderField label={label} value={c[key] as number} min={min} max={max} step={step} format={fmt}
      onBegin={beginEdit} onChange={(v) => updateClip(c.id, { [key]: v })} onCommit={endEdit} />
  );

  return (
    <>
      <Group>
        <Badge variant="secondary" className="self-start rounded-sm bg-surface-3 type-label text-ink-muted">{KIND[c.type]} · {c.track}</Badge>
        <h2 className="type-headline break-words">{c.name}</h2>
        <KV rows={[["Start", tc(c.start)], ["End", tc(c.start + c.dur)], ["Duration", tc(c.dur)], ...(c.in ? [["Source in", tc(c.in)] as [string, string]] : [])]} />
      </Group>

      {c.text && <TextGroups clip={c} text={c.text} />}

      {visual ? (
        <>
          <Group title="Transform">
            {slider("Position X", "x", -640, 640, 1, (v) => String(Math.round(v)))}
            {slider("Position Y", "y", -360, 360, 1, (v) => String(Math.round(v)))}
            {slider("Scale", "scale", 0.1, 3, 0.01, pct)}
            {slider("Rotation", "rot", -180, 180, 1, (v) => `${v}°`)}
            {slider("Opacity", "opacity", 0, 1, 0.01, pct)}
          </Group>
          <TransitionControl clip={c} />
          <EffectStack clip={c} />
        </>
      ) : (
        <Group title="Audio">
          {slider("Volume", "volume", 0, 1, 0.01, pct)}
          {c.type === "music" && a && <p className="type-micro text-ink-muted">{a.sub} · {a.bpm} BPM. The track keeps playing for as long as you stretch the clip.</p>}
        </Group>
      )}

      <Group title="Fades">
        {slider("Fade in", "fadeIn", 0, maxFade, 0.05, (v) => `${v.toFixed(2)}s`)}
        {slider("Fade out", "fadeOut", 0, maxFade, 0.05, (v) => `${v.toFixed(2)}s`)}
      </Group>

      <Group>
        <div className="flex flex-wrap gap-1.5">
          <Button size="sm" variant="secondary" onClick={split}><Scissors />Split</Button>
          <Button size="sm" variant="secondary" onClick={duplicate}><Copy />Duplicate</Button>
          <Button size="sm" variant="destructive" onClick={remove}><Trash2 />Delete</Button>
        </div>
      </Group>
    </>
  );
}

function TextGroups({ clip, text: T }: { clip: Clip; text: TextProps }) {
    const { beginEdit, endEdit, updateText, commit } = useEditor.getState();
    const set = (patch: Partial<TextProps>) => { commit(); updateText(clip.id, patch); };
    const seg = <V extends string>(value: string, options: [V, string][], onChange: (v: V) => void, label: string) => (
      <ToggleGroup type="single" size="sm" variant="outline" spacing={0} value={value} aria-label={label}
        onValueChange={(v) => v && onChange(v as V)} className="w-full">
        {options.map(([v, l]) => <ToggleGroupItem key={v} value={v} className="flex-1 type-micro">{l}</ToggleGroupItem>)}
      </ToggleGroup>
    );
    return (
      <>
        <Group title="Text">
          <Textarea id="f-text" aria-label="Text" rows={2} value={T.content}
            onFocus={beginEdit} onBlur={endEdit} onChange={(e) => updateText(clip.id, { content: e.target.value })}
            className="min-h-16 bg-canvas type-caption" />
          <FieldRow label="Font" htmlFor="f-font">
            <Select value={T.font} onValueChange={(v) => set({ font: v, weight: FONTS.find((f) => f.family === v)!.weight })}>
              <SelectTrigger id="f-font" className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>{FONTS.map((f) => <SelectItem key={f.family} value={f.family} style={{ fontFamily: `var(${f.cssVar})` }}>{f.family}</SelectItem>)}</SelectContent>
            </Select>
          </FieldRow>
          <FieldRow label="Weight" htmlFor="f-weight">
            <Select value={String(T.weight)} onValueChange={(v) => set({ weight: +v })}>
              <SelectTrigger id="f-weight" className="w-full"><SelectValue /></SelectTrigger>
              <SelectContent>{([[400, "Regular"], [500, "Medium"], [600, "Semibold"], [700, "Bold"], [800, "Heavy"]] as const).map(([w, l]) => <SelectItem key={w} value={String(w)}>{l}</SelectItem>)}</SelectContent>
            </Select>
          </FieldRow>
          <SliderField label="Size" value={T.size} min={16} max={240} step={1} format={(v) => `${v}px`} onBegin={beginEdit} onChange={(v) => updateText(clip.id, { size: v })} onCommit={endEdit} />
          <SliderField label="Height" value={T.y} min={-320} max={320} step={1} format={(v) => String(Math.round(v))} onBegin={beginEdit} onChange={(v) => updateText(clip.id, { y: v })} onCommit={endEdit} />
          <div className="flex flex-wrap gap-1.5" role="radiogroup" aria-label="Text colour">
            {TEXT_COLORS.map((col) => (
              <button key={col} type="button" role="radio" aria-checked={col === T.color} aria-label={`Colour ${col}`} onClick={() => set({ color: col })}
                className={cn("size-[22px] rounded-full shadow-[inset_0_0_0_1px_rgb(255_255_255/.15)] outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  col === T.color && "shadow-[0_0_0_2px_var(--canvas),0_0_0_3.5px_var(--signal-blue)]")} style={{ background: col }} />
            ))}
          </div>
        </Group>
        <Group title="Layout & motion">
          <FieldRow label="Align">{seg(T.align, [["left", "Left"], ["center", "Center"], ["right", "Right"]], (v) => set({ align: v }), "Align")}</FieldRow>
          <FieldRow label="Backing">{seg(String(T.box), [["false", "None"], ["true", "Box"]], (v) => set({ box: v === "true" }), "Backing")}</FieldRow>
          <FieldRow label="Animate">{seg(T.anim, TEXT_ANIMS, (v) => set({ anim: v }), "Animate")}</FieldRow>
        </Group>
      </>
    );
}
