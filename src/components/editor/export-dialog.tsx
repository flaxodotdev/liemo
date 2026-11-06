"use client";

import * as React from "react";
import { AlertTriangle, AudioLines, Check, Download, Film, Image as ImageIcon, Loader2, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Slider } from "@/components/ui/slider";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from "@/components/ui/dialog";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { FieldRow } from "./primitives";
import { cn } from "@/lib/utils";
import { projectEnd, useEditor } from "@/lib/editor/store";
import { FPS, tc } from "@/lib/editor/time";
import {
  ASPECTS, ExportCancelled, RESOLUTIONS, downloadBlob, exportProject, outputSize, probeVideo,
  type Aspect, type AudioFormat, type ExportKind, type ExportOptions, type ExportResult, type Framing,
  type ImageFormat, type QualityLevel, type Resolution, type VideoContainer, type VideoSupport,
} from "@/lib/editor/export";

type Phase = { kind: "setup" } | { kind: "running"; fraction: number; label: string } | { kind: "done"; result: ExportResult; filename: string } | { kind: "error"; message: string };
type RangeMode = "all" | "clip" | "custom";
type Settings = Omit<ExportOptions, "range" | "imageTime"> & { rangeMode: RangeMode };

const DEFAULTS: Settings = {
  kind: "video", rangeMode: "all", resolution: "1080", aspect: "16:9", framing: "blur",
  container: "mp4", fps: 30, quality: "high", audio: true, gifWidth: 480, gifFps: 15, imageFormat: "png", audioFormat: "wav",
};
const STORE_KEY = "rayna-export-settings";
const safeName = (s: string) => s.trim().replace(/[\\/:*?"<>|]+/g, "-").slice(0, 80) || "Liemo export";
const mb = (bytes: number) => (bytes < 1048576 ? `${Math.max(1, Math.round(bytes / 1024))} KB` : `${(bytes / 1048576).toFixed(1)} MB`);

const KINDS: { id: ExportKind; label: string; icon: React.ElementType }[] = [
  { id: "video", label: "Video", icon: Film }, { id: "gif", label: "GIF", icon: Sparkles },
  { id: "image", label: "Still", icon: ImageIcon }, { id: "audio", label: "Audio", icon: AudioLines },
];

export function ExportDialog() {
  const [open, setOpen] = React.useState(false);
  const [s, setS] = React.useState<Settings>(DEFAULTS);
  const [name, setName] = React.useState("");
  const [custom, setCustom] = React.useState<[number, number]>([0, 0]);
  const [support, setSupport] = React.useState<VideoSupport | "checking" | null>(null);
  const [phase, setPhase] = React.useState<Phase>({ kind: "setup" });
  const abort = React.useRef<AbortController | null>(null);

  const projectName = useEditor((st) => st.name);
  const duration = useEditor((st) => projectEnd(st.clips));
  const clipCount = useEditor((st) => st.clips.length);
  const selected = useEditor((st) => st.clips.find((c) => c.id === st.selected));
  const time = useEditor((st) => st.time);
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setS((prev) => {
    const next = { ...prev, [k]: v };
    try { localStorage.setItem(STORE_KEY, JSON.stringify(next)); } catch {}
    return next;
  });

  // Restore last-used settings and reset the dialog each time it opens.
  React.useEffect(() => {
    if (!open) return;
    try { const saved = localStorage.getItem(STORE_KEY); if (saved) setS({ ...DEFAULTS, ...JSON.parse(saved) }); } catch {}
    setName(safeName(projectName));
    setCustom([0, duration]);
    setPhase({ kind: "setup" });
  }, [open]); // eslint-disable-line react-hooks/exhaustive-deps

  const range = React.useMemo(() => {
    if (s.rangeMode === "clip" && selected) return { start: selected.start, end: Math.min(duration, selected.start + selected.dur) };
    if (s.rangeMode === "custom") return { start: Math.min(custom[0], custom[1]), end: Math.max(custom[0], custom[1]) };
    return { start: 0, end: duration };
  }, [s.rangeMode, selected, custom, duration]);
  const length = Math.max(0, range.end - range.start);
  const size = s.kind === "gif"
    ? { width: s.gifWidth, height: Math.round((s.gifWidth * ASPECTS[s.aspect].h) / ASPECTS[s.aspect].w / 2) * 2 }
    : outputSize(s.resolution, s.aspect);

  // For video, check (once per combination) which codec really works at this size.
  React.useEffect(() => {
    if (!open || s.kind !== "video") return;
    let alive = true;
    setSupport("checking");
    const t = setTimeout(() => {
      probeVideo(s.container, size.width, size.height, s.fps).then((r) => { if (alive) setSupport(r); });
    }, 250);
    return () => { alive = false; clearTimeout(t); };
  }, [open, s.kind, s.container, size.width, size.height, s.fps]);

  const running = phase.kind === "running";
  const videoBlocked = s.kind === "video" && (support === "checking" || !support || !support.video);
  const canStart = clipCount > 0 && (s.kind === "image" || length >= 0.05) && !videoBlocked;

  const start = async () => {
    const controller = new AbortController();
    abort.current = controller;
    setPhase({ kind: "running", fraction: 0, label: "Preparing" });
    try {
      const opts: ExportOptions = { ...s, range, imageTime: Math.min(time, Math.max(0, duration - 1 / FPS)) };
      const result = await exportProject(opts, controller.signal, (fraction, label) => setPhase({ kind: "running", fraction, label }));
      const filename = `${safeName(name)}.${result.ext}`;
      downloadBlob(result.blob, filename);
      setPhase({ kind: "done", result, filename });
    } catch (err) {
      if (err instanceof ExportCancelled) setPhase({ kind: "setup" });
      else setPhase({ kind: "error", message: err instanceof Error ? err.message : "Something went wrong while exporting." });
    } finally {
      abort.current = null;
    }
  };

  const seg = <T extends string | number>(value: T, options: [T, string][], onChange: (v: T) => void, label: string, disabled?: (v: T) => boolean) => (
    <ToggleGroup type="single" size="sm" variant="outline" spacing={0} value={String(value)} aria-label={label} className="w-full"
      onValueChange={(v) => { if (!v) return; const hit = options.find(([o]) => String(o) === v); if (hit) onChange(hit[0]); }}>
      {options.map(([v, l]) => <ToggleGroupItem key={String(v)} value={String(v)} disabled={disabled?.(v)} className="flex-1 type-micro">{l}</ToggleGroupItem>)}
    </ToggleGroup>
  );
  const select = <T extends string | number>(id: string, value: T, options: [T, string][], onChange: (v: T) => void) => (
    <Select value={String(value)} onValueChange={(v) => { const hit = options.find(([o]) => String(o) === v); if (hit) onChange(hit[0]); }}>
      <SelectTrigger id={id} size="sm" className="w-full"><SelectValue /></SelectTrigger>
      <SelectContent>{options.map(([v, l]) => <SelectItem key={String(v)} value={String(v)}>{l}</SelectItem>)}</SelectContent>
    </Select>
  );

  const framingControls = (
    <>
      <FieldRow label="Shape" htmlFor="ex-aspect">{select<Aspect>("ex-aspect", s.aspect, Object.entries(ASPECTS).map(([k, v]) => [k as Aspect, v.label]), (v) => set("aspect", v))}</FieldRow>
      {s.aspect !== "16:9" && (
        <FieldRow label="Framing">{seg<Framing>(s.framing, [["crop", "Crop to fill"], ["blur", "Blurred fill"], ["bars", "Black bars"]], (v) => set("framing", v), "Framing")}</FieldRow>
      )}
    </>
  );

  const summary = s.kind === "audio"
    ? `${s.audioFormat === "wav" ? "WAV · 48 kHz · 16-bit" : "M4A · AAC"} · ${tc(length)}`
    : s.kind === "image"
      ? `${size.width} × ${size.height} · frame at ${tc(time)}`
      : `${size.width} × ${size.height} · ${s.kind === "gif" ? s.gifFps : s.fps} fps · ${tc(length)} · ${Math.max(1, Math.round(length * (s.kind === "gif" ? s.gifFps : s.fps)))} frames`;

  const warnings: string[] = [];
  if (s.kind === "gif" && length > 15) warnings.push("GIFs get large quickly. Keep them under about 15 seconds, or lower the width and frame rate.");
  if (s.kind === "video" && s.resolution === "2160") warnings.push("4K renders effects at full size, so heavy effects (ASCII, dots, edges) will export slowly.");
  if (s.kind !== "audio" && s.aspect !== "16:9" && s.framing === "crop") warnings.push("Cropping cuts off the sides of the frame. Check that titles still fit.");
  if (s.kind === "video" && support && support !== "checking" && support.video && s.audio && !support.audio) warnings.push(`This browser can't encode audio for ${s.container.toUpperCase()}, so the file will be silent.`);

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o && running) return; setOpen(o); }}>
      <DialogTrigger asChild>
        <Button size="lg" className="px-4" disabled={clipCount === 0}><Download />Export</Button>
      </DialogTrigger>
      <DialogContent className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-lg" showCloseButton={!running}
        onEscapeKeyDown={(e) => running && e.preventDefault()} onPointerDownOutside={(e) => running && e.preventDefault()}>
        <DialogHeader>
          <DialogTitle className="type-headline">Export</DialogTitle>
          <DialogDescription>{projectName || "Untitled project"} · <span className="type-timecode text-ink">{tc(duration)}</span> · {clipCount} clips</DialogDescription>
        </DialogHeader>

        {phase.kind === "setup" || phase.kind === "error" ? (
          <div className="flex flex-col gap-4">
            <div role="tablist" aria-label="What to export" className="grid grid-cols-4 gap-1.5">
              {KINDS.map(({ id, label, icon: Icon }) => (
                <button key={id} type="button" role="tab" aria-selected={s.kind === id} onClick={() => set("kind", id)}
                  className={cn("flex flex-col items-center gap-1.5 rounded-md border px-2 py-3 type-micro font-medium transition-colors focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
                    s.kind === id ? "border-transparent bg-surface-3 text-ink" : "border-hairline text-ink-muted hover:bg-surface-2 hover:text-ink")}>
                  <Icon className="size-4" />{label}
                </button>
              ))}
            </div>

            <div className="flex flex-col gap-3">
              <FieldRow label="File name" htmlFor="export-name">
                <Input id="export-name" value={name} onChange={(e) => setName(e.target.value)} className="h-8 bg-canvas type-caption" maxLength={80} />
              </FieldRow>

              {s.kind === "video" && (
                <>
                  <FieldRow label="Format">{seg<VideoContainer>(s.container, [["mp4", "MP4"], ["webm", "WebM"]], (v) => set("container", v), "Format")}</FieldRow>
                  <FieldRow label="Resolution" htmlFor="ex-res">{select<Resolution>("ex-res", s.resolution, Object.entries(RESOLUTIONS).map(([k, v]) => [k as Resolution, v.label]), (v) => set("resolution", v))}</FieldRow>
                  {framingControls}
                  <FieldRow label="Frame rate">{seg<number>(s.fps, [[24, "24"], [25, "25"], [30, "30"], [60, "60"]], (v) => set("fps", v), "Frame rate")}</FieldRow>
                  <FieldRow label="Quality">{seg<QualityLevel>(s.quality, [["low", "Low"], ["medium", "Medium"], ["high", "High"], ["best", "Best"]], (v) => set("quality", v), "Quality")}</FieldRow>
                  <FieldRow label="Sound">{seg<string>(s.audio ? "on" : "off", [["on", "Include"], ["off", "Silent"]], (v) => set("audio", v === "on"), "Sound")}</FieldRow>
                </>
              )}

              {s.kind === "gif" && (
                <>
                  <FieldRow label="Width" htmlFor="ex-gw">{select<number>("ex-gw", s.gifWidth, [[320, "320 px · small"], [480, "480 px"], [640, "640 px"], [800, "800 px · large"]], (v) => set("gifWidth", v))}</FieldRow>
                  {framingControls}
                  <FieldRow label="Frame rate">{seg<number>(s.gifFps, [[10, "10"], [15, "15"], [20, "20"], [24, "24"]], (v) => set("gifFps", v), "GIF frame rate")}</FieldRow>
                </>
              )}

              {s.kind === "image" && (
                <>
                  <FieldRow label="Format">{seg<ImageFormat>(s.imageFormat, [["png", "PNG"], ["jpeg", "JPEG"]], (v) => set("imageFormat", v), "Image format")}</FieldRow>
                  <FieldRow label="Resolution" htmlFor="ex-ires">{select<Resolution>("ex-ires", s.resolution, Object.entries(RESOLUTIONS).map(([k, v]) => [k as Resolution, v.label]), (v) => set("resolution", v))}</FieldRow>
                  {framingControls}
                  <p className="type-micro text-ink-muted">Saves the frame under the playhead, at <span className="type-timecode text-ink">{tc(time)}</span>. Move the playhead to pick another.</p>
                </>
              )}

              {s.kind === "audio" && (
                <>
                  <FieldRow label="Format">{seg<AudioFormat>(s.audioFormat, [["wav", "WAV · lossless"], ["m4a", "M4A · smaller"]], (v) => set("audioFormat", v), "Audio format")}</FieldRow>
                  {s.audioFormat === "m4a" && <FieldRow label="Quality">{seg<QualityLevel>(s.quality, [["low", "Low"], ["medium", "Medium"], ["high", "High"], ["best", "Best"]], (v) => set("quality", v), "Audio quality")}</FieldRow>}
                </>
              )}

              {s.kind !== "image" && (
                <>
                  <FieldRow label="Range">
                    {seg<RangeMode>(s.rangeMode, [["all", "Whole video"], ["clip", "Selected clip"], ["custom", "Custom"]], (v) => set("rangeMode", v), "Range", (v) => v === "clip" && !selected)}
                  </FieldRow>
                  {s.rangeMode === "custom" && (
                    <div className="flex flex-col gap-1.5 pl-[84px]">
                      <Slider aria-label="Export range" min={0} max={Math.max(duration, 1 / FPS)} step={1 / FPS} value={custom} minStepsBetweenThumbs={1}
                        onValueChange={(v) => setCustom([v[0], v[1]])} />
                      <div className="flex justify-between type-timecode text-[11px] text-ink-muted"><span>{tc(range.start)}</span><span>{tc(range.end)}</span></div>
                    </div>
                  )}
                  {s.rangeMode === "clip" && selected && (
                    <p className="pl-[84px] type-micro text-ink-muted">“{selected.name}”, {tc(range.start)} to {tc(range.end)}</p>
                  )}
                </>
              )}
            </div>

            <div className="flex flex-col gap-2 rounded-md bg-surface-1 p-3">
              <div className="flex items-center justify-between gap-3">
                <span className="type-timecode text-[12px] text-ink">{summary}</span>
                {s.kind === "video" && (
                  <span className="flex shrink-0 items-center gap-1.5 type-micro text-ink-muted">
                    {support === "checking" || !support ? <><Loader2 className="size-3 animate-spin" />Checking encoder</>
                      : support.video ? <><Check className="size-3 text-success" />{support.video.codec === "avc" ? "H.264" : support.video.codec.toUpperCase()}{s.audio && support.audio ? ` + ${support.audio.toUpperCase()}` : ""}</>
                      : <span className="text-coral">Not supported here</span>}
                  </span>
                )}
              </div>
              {warnings.map((w) => <p key={w} className="type-micro text-ink-muted">{w}</p>)}
            </div>

            {phase.kind === "error" && <Note>{phase.message}</Note>}
          </div>
        ) : phase.kind === "running" ? (
          <div className="flex flex-col gap-2" role="status" aria-live="polite">
            <div className="h-1.5 overflow-hidden rounded-full bg-surface-3">
              <div className="h-full rounded-full bg-ink transition-[width] duration-150" style={{ width: `${Math.round(phase.fraction * 100)}%` }} />
            </div>
            <div className="flex justify-between type-micro text-ink-muted">
              <span>{phase.label}</span><span className="type-timecode">{Math.round(phase.fraction * 100)}%</span>
            </div>
          </div>
        ) : (
          <div className="flex items-start gap-3 rounded-md bg-surface-1 p-3">
            <span className="grid size-8 shrink-0 place-items-center rounded-full bg-success/15 text-success"><Check className="size-4" /></span>
            <div className="flex min-w-0 flex-col gap-0.5">
              <span className="truncate type-caption">{phase.filename}</span>
              <span className="type-micro text-ink-muted">
                {mb(phase.result.blob.size)}
                {phase.result.width ? ` · ${phase.result.width} × ${phase.result.height}` : ""}
                {phase.result.seconds ? ` · ${tc(phase.result.seconds)}` : ""}
                {s.kind === "video" && !phase.result.withAudio ? " · no sound" : ""}. Saved to your downloads.
              </span>
            </div>
          </div>
        )}

        <DialogFooter>
          {phase.kind === "running" ? (
            <Button variant="secondary" onClick={() => abort.current?.abort()}>Cancel export</Button>
          ) : phase.kind === "done" ? (
            <>
              <Button variant="secondary" onClick={() => setPhase({ kind: "setup" })}>Export another</Button>
              <Button variant="secondary" onClick={() => downloadBlob(phase.result.blob, phase.filename)}><Download />Download again</Button>
              <Button onClick={() => setOpen(false)}>Done</Button>
            </>
          ) : (
            <>
              <Button variant="secondary" onClick={() => setOpen(false)}>Close</Button>
              <Button onClick={start} disabled={!canStart}>
                {phase.kind === "error" ? "Try again" : s.kind === "video" ? `Export ${s.container.toUpperCase()}` : s.kind === "gif" ? "Export GIF" : s.kind === "image" ? `Save ${s.imageFormat === "png" ? "PNG" : "JPEG"}` : `Export ${s.audioFormat.toUpperCase()}`}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function Note({ children }: { children: React.ReactNode }) {
  return <p className="flex items-start gap-2 rounded-md bg-coral/10 p-3 type-micro text-ink"><AlertTriangle className="mt-px size-3.5 shrink-0 text-coral" />{children}</p>;
}
