"use client";

// Editor building blocks. Every panel composes these so spacing, type and states stay identical.
import * as React from "react";
import { Button } from "@/components/ui/button";
import { Slider } from "@/components/ui/slider";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { cn } from "@/lib/utils";
import { tc } from "@/lib/editor/time";

export function Panel({ className, ...props }: React.ComponentProps<"section">) {
  return <section className={cn("flex min-h-0 min-w-0 flex-col overflow-hidden rounded-lg bg-surface-1", className)} {...props} />;
}

export function PanelHeader({ className, ...props }: React.ComponentProps<"div">) {
  return <div className={cn("flex min-h-12 items-center gap-2 border-b border-hairline-soft px-4 py-2", className)} {...props} />;
}

export function SectionLabel({ className, children, action }: { className?: string; children: React.ReactNode; action?: React.ReactNode }) {
  return (
    <div className={cn("flex items-center justify-between gap-2", className)}>
      <h3 className="type-label text-ink-muted">{children}</h3>
      {action && <span className="type-micro text-ink-muted">{action}</span>}
    </div>
  );
}

export function Group({ title, action, children, className }: { title?: string; action?: React.ReactNode; children: React.ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col gap-3 border-b border-hairline-soft py-4 last:border-b-0", className)}>
      {title && <SectionLabel action={action}>{title}</SectionLabel>}
      {children}
    </div>
  );
}

type IconButtonProps = React.ComponentProps<typeof Button> & { label: string; shortcut?: string };
export function IconButton({ label, shortcut, size = "icon", variant = "secondary", ...props }: IconButtonProps) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button size={size} variant={variant} aria-label={label} {...props} />
      </TooltipTrigger>
      <TooltipContent>
        {label}
        {shortcut && <kbd className="ml-2 type-timecode text-[11px] opacity-70">{shortcut}</kbd>}
      </TooltipContent>
    </Tooltip>
  );
}

/** Label · control · value row used across the inspector. */
export function FieldRow({ label, children, value, htmlFor }: { label: string; children: React.ReactNode; value?: React.ReactNode; htmlFor?: string }) {
  return (
    <div className={cn("grid items-center gap-3", value !== undefined ? "grid-cols-[72px_minmax(0,1fr)_48px]" : "grid-cols-[72px_minmax(0,1fr)]")}>
      <label htmlFor={htmlFor} className="type-micro text-ink-muted">{label}</label>
      <div className="min-w-0">{children}</div>
      {value !== undefined && <output className="type-timecode text-right text-[11px] text-ink">{value}</output>}
    </div>
  );
}

export function SliderField({ label, value, min, max, step, format, onBegin, onChange, onCommit, idPrefix = "f" }: {
  label: string; value: number; min: number; max: number; step: number;
  format?: (v: number) => string; onBegin: () => void; onChange: (v: number) => void; onCommit: () => void; idPrefix?: string;
}) {
  const id = `${idPrefix}-${label.toLowerCase().replace(/\W+/g, "-")}`;
  const started = React.useRef(false);
  return (
    <FieldRow label={label} htmlFor={id} value={format ? format(value) : value}>
      <Slider
        id={id} aria-label={label} min={min} max={max} step={step} value={[value]}
        onValueChange={([v]) => { if (!started.current) { started.current = true; onBegin(); } onChange(v); }}
        onValueCommit={() => { started.current = false; onCommit(); }}
      />
    </FieldRow>
  );
}

export function Timecode({ seconds, className }: { seconds: number; className?: string }) {
  return <span className={cn("type-timecode", className)}>{tc(seconds)}</span>;
}

export function Kbd({ children }: { children: React.ReactNode }) {
  return <kbd className="type-timecode justify-self-start rounded-xs bg-surface-3 px-1.5 py-0.5 text-[11px] text-ink shadow-[inset_0_-1px_0_rgb(0_0_0/.4)]">{children}</kbd>;
}
