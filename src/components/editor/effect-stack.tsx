"use client";

import * as React from "react";
import { ChevronDown, ChevronUp, Eye, EyeOff, Plus, X } from "lucide-react";
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectSeparator, SelectTrigger, SelectValue } from "@/components/ui/select";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { FieldRow, Group, IconButton, SliderField } from "./primitives";
import { cn } from "@/lib/utils";
import { EFFECTS, EFFECT_GROUPS, effectDef, type ClipEffect, type ParamDef } from "@/lib/editor/fx/registry";
import { TRANSITIONS, TRANSITION_GROUPS, transitionDef } from "@/lib/editor/fx/transitions";
import { effectsOnObject, useEditor, type Clip } from "@/lib/editor/store";

const SWATCHES = ["#ffffff", "#22c55e", "#0099ff", "#6a4cf5", "#d44df0", "#ff5577", "#ff7a3d", "#ffd9a0", "#1b0f2e"];

/** The clip's effects, top to bottom in the order they are applied. */
export function EffectStack({ clip }: { clip: Clip }) {
  const { applyEffect, commit, updateClip } = useEditor.getState();
  const onObject = effectsOnObject(clip);
  return (
    <Group title="Effects" action={clip.effects.length ? `${clip.effects.length} applied, top first` : undefined}>
      <FieldRow label="Applies to">
        <ToggleGroup type="single" size="sm" variant="outline" spacing={0} value={onObject ? "clip" : "frame"} aria-label="Effects apply to"
          onValueChange={(v) => { if (!v) return; commit(); updateClip(clip.id, { fxScope: v as "clip" | "frame" }); }} className="w-full">
          <ToggleGroupItem value="clip" className="flex-1 type-micro" title="Effects stay on this clip's own shape">{clip.text ? "Just the text" : "Just this clip"}</ToggleGroupItem>
          <ToggleGroupItem value="frame" className="flex-1 type-micro" title="Effects fill the whole 1280×720 frame">Whole frame</ToggleGroupItem>
        </ToggleGroup>
      </FieldRow>
      {clip.effects.length === 0 && (
        <p className="type-micro text-ink-muted">No effects yet. Add one below, or drag one from the Effects tab onto the clip.</p>
      )}
      {clip.effects.map((fx, i) => (
        <EffectCard key={fx.uid} clip={clip} fx={fx} first={i === 0} last={i === clip.effects.length - 1} />
      ))}
      <Select value="" onValueChange={(id) => applyEffect(id, clip.id)}>
        <SelectTrigger aria-label="Add effect" className="w-full border-dashed bg-transparent text-ink-muted">
          <span className="flex items-center gap-1.5"><Plus className="size-3.5" /><SelectValue placeholder="Add effect" /></span>
        </SelectTrigger>
        <SelectContent>
          {EFFECT_GROUPS.map(([group, label], i) => (
            <React.Fragment key={group}>
              {i > 0 && <SelectSeparator />}
              <SelectGroup>
                <SelectLabel>{label}</SelectLabel>
                {EFFECTS.filter((e) => e.group === group).map((e) => <SelectItem key={e.id} value={e.id}>{e.name}</SelectItem>)}
              </SelectGroup>
            </React.Fragment>
          ))}
        </SelectContent>
      </Select>
    </Group>
  );
}

function EffectCard({ clip, fx, first, last }: { clip: Clip; fx: ClipEffect; first: boolean; last: boolean }) {
  const { beginEdit, endEdit, commit, updateEffect, removeEffect, moveEffect } = useEditor.getState();
  const def = effectDef(fx.id);
  if (!def) return null;
  const idPrefix = `fx-${fx.uid}`;
  const setParam = (key: string, value: ClipEffect["params"][string]) => updateEffect(clip.id, fx.uid, { params: { [key]: value } });

  return (
    <div className={cn("flex flex-col gap-3 rounded-md bg-surface-2 p-3", !fx.enabled && "opacity-60")}>
      <div className="flex items-center gap-1">
        <span className="flex-1 truncate type-caption">{def.name}</span>
        <IconButton label={fx.enabled ? "Turn effect off" : "Turn effect on"} size="icon-xs" variant="ghost" aria-pressed={!fx.enabled}
          onClick={() => { commit(); updateEffect(clip.id, fx.uid, { enabled: !fx.enabled }); }}>{fx.enabled ? <Eye /> : <EyeOff />}</IconButton>
        <IconButton label="Move up" size="icon-xs" variant="ghost" disabled={first} onClick={() => moveEffect(clip.id, fx.uid, -1)}><ChevronUp /></IconButton>
        <IconButton label="Move down" size="icon-xs" variant="ghost" disabled={last} onClick={() => moveEffect(clip.id, fx.uid, 1)}><ChevronDown /></IconButton>
        <IconButton label="Remove effect" size="icon-xs" variant="ghost" onClick={() => removeEffect(clip.id, fx.uid)}><X /></IconButton>
      </div>

      <SliderField idPrefix={idPrefix} label={def.engine === "css" ? "Strength" : "Mix"} value={fx.amount} min={0} max={1} step={0.01}
        format={(v) => `${Math.round(v * 100)}%`} onBegin={beginEdit} onChange={(v) => updateEffect(clip.id, fx.uid, { amount: v })} onCommit={endEdit} />

      {def.params.map((p) => (
        <ParamControl key={p.key} def={p} value={fx.params[p.key]} idPrefix={idPrefix} hidden={p.key === "tint" && fx.params.color !== "tint"}
          onBegin={beginEdit} onCommit={endEdit} onSet={(v, discrete) => { if (discrete) commit(); setParam(p.key, v); }} />
      ))}
    </div>
  );
}

function ParamControl({ def: p, value, idPrefix, hidden, onBegin, onCommit, onSet }: {
  def: ParamDef; value: ClipEffect["params"][string]; idPrefix: string; hidden?: boolean;
  onBegin: () => void; onCommit: () => void; onSet: (v: number | string | boolean, discrete: boolean) => void;
}) {
  if (hidden) return null;
  const id = `${idPrefix}-${p.key}`;
  switch (p.type) {
    case "range":
      return (
        <SliderField idPrefix={idPrefix} label={p.label} value={typeof value === "number" ? value : p.min} min={p.min} max={p.max} step={p.step}
          format={(v) => `${+v.toFixed(2)}${p.unit ?? ""}`} onBegin={onBegin} onChange={(v) => onSet(v, false)} onCommit={onCommit} />
      );
    case "select":
      return (
        <FieldRow label={p.label} htmlFor={id}>
          <Select value={String(value)} onValueChange={(v) => onSet(v, true)}>
            <SelectTrigger id={id} size="sm" className="w-full"><SelectValue /></SelectTrigger>
            <SelectContent>{p.options.map(([v, l]) => <SelectItem key={v} value={v}>{l}</SelectItem>)}</SelectContent>
          </Select>
        </FieldRow>
      );
    case "toggle":
      return (
        <FieldRow label={p.label}>
          <ToggleGroup type="single" size="sm" variant="outline" spacing={0} value={value ? "on" : "off"} aria-label={p.label}
            onValueChange={(v) => v && onSet(v === "on", true)} className="w-full">
            <ToggleGroupItem value="off" className="flex-1 type-micro">Off</ToggleGroupItem>
            <ToggleGroupItem value="on" className="flex-1 type-micro">On</ToggleGroupItem>
          </ToggleGroup>
        </FieldRow>
      );
    case "color":
      return (
        <FieldRow label={p.label}>
          <div className="flex flex-wrap items-center gap-1.5" role="radiogroup" aria-label={p.label}>
            {SWATCHES.map((col) => (
              <button key={col} type="button" role="radio" aria-checked={col === value} aria-label={`${p.label} ${col}`} onClick={() => onSet(col, true)}
                className={cn("size-5 rounded-full shadow-[inset_0_0_0_1px_rgb(255_255_255/.15)] outline-none focus-visible:ring-2 focus-visible:ring-ring",
                  col === value && "shadow-[0_0_0_2px_var(--surface-2),0_0_0_3.5px_var(--signal-blue)]")} style={{ background: col }} />
            ))}
            <label className="relative size-5 cursor-pointer overflow-hidden rounded-full bg-[conic-gradient(red,yellow,lime,cyan,blue,magenta,red)]" title="Custom colour">
              <input id={id} type="color" value={String(value)} onChange={(e) => onSet(e.target.value, false)} onBlur={onCommit} onFocus={onBegin}
                className="absolute inset-0 size-full cursor-pointer opacity-0" aria-label={`Custom ${p.label.toLowerCase()}`} />
            </label>
          </div>
        </FieldRow>
      );
  }
}

/** The transition into this clip: type, length, and whether it blends from a previous clip. */
export function TransitionControl({ clip }: { clip: Clip }) {
  const { commit, updateClip, beginEdit, endEdit, applyTransition } = useEditor.getState();
  const prevTouches = useEditor((s) => s.clips.some((k) => k.track === clip.track && k.id !== clip.id && Math.abs(k.start + k.dur - clip.start) < 1.5 / 30));
  const tr = clip.transition, def = tr ? transitionDef(tr.id) : undefined;
  return (
    <Group title="Transition in" action={def ? (prevTouches ? "Blends from the clip before" : "Plays over the start") : undefined}>
      <FieldRow label="Type" htmlFor="f-transition">
        <Select value={tr?.id ?? "none"} onValueChange={(v) => {
          if (v === "none") { commit(); updateClip(clip.id, { transition: undefined }); } else applyTransition(v, clip.id);
        }}>
          <SelectTrigger id="f-transition" size="sm" className="w-full"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="none">None (hard cut)</SelectItem>
            {TRANSITION_GROUPS.map(([group, label]) => (
              <SelectGroup key={group}>
                <SelectSeparator />
                <SelectLabel>{label}</SelectLabel>
                {TRANSITIONS.filter((t) => t.group === group).map((t) => <SelectItem key={t.id} value={t.id}>{t.name}</SelectItem>)}
              </SelectGroup>
            ))}
          </SelectContent>
        </Select>
      </FieldRow>
      {tr && (
        <SliderField label="Duration" value={tr.dur} min={0.1} max={Math.max(0.2, Math.min(3, clip.dur))} step={0.05}
          format={(v) => `${v.toFixed(2)}s`} onBegin={beginEdit} onCommit={endEdit}
          onChange={(v) => updateClip(clip.id, { transition: { ...tr, dur: v } })} />
      )}
    </Group>
  );
}
