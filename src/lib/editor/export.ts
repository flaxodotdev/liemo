"use client";

// Export: video (MP4/WebM), animated GIF, a still frame, or audio only. Video is rendered frame
// by frame, audio is mixed offline, and both are encoded with WebCodecs (via mediabunny).
// Nothing plays aloud and every frame is exact.
import {
  AudioBufferSource, BufferTarget, CanvasSource, Mp4OutputFormat, Output, QUALITY_HIGH, QUALITY_LOW, QUALITY_MEDIUM, QUALITY_VERY_HIGH,
  WebMOutputFormat, getFirstEncodableAudioCodec, type AudioCodec, type Quality, type VideoCodec,
} from "mediabunny";
import { GIFEncoder, applyPalette, quantize } from "gifenc";
import { ASSETS } from "./assets";
import { H, W } from "./time";
import { projectEnd, trackOf, useEditor } from "./store";
import { assetBlob, fadeAt, renderFrame, seekVideosForExport } from "./render";
import { pause, playBeat } from "./playback";

// ---------- Options ----------
export type ExportKind = "video" | "gif" | "image" | "audio";
export type VideoContainer = "mp4" | "webm";
export type ImageFormat = "png" | "jpeg";
export type AudioFormat = "wav" | "m4a";
export type Resolution = "720" | "1080" | "1440" | "2160";
export type Aspect = "16:9" | "9:16" | "1:1" | "4:5";
export type Framing = "crop" | "blur" | "bars";
export type QualityLevel = "low" | "medium" | "high" | "best";

export const RESOLUTIONS: Record<Resolution, { label: string; short: number }> = {
  "720": { label: "720p", short: 720 }, "1080": { label: "1080p · Full HD", short: 1080 },
  "1440": { label: "1440p · 2K", short: 1440 }, "2160": { label: "2160p · 4K", short: 2160 },
};
export const ASPECTS: Record<Aspect, { label: string; w: number; h: number }> = {
  "16:9": { label: "16:9 · Original", w: 16, h: 9 }, "9:16": { label: "9:16 · Vertical", w: 9, h: 16 },
  "1:1": { label: "1:1 · Square", w: 1, h: 1 }, "4:5": { label: "4:5 · Portrait", w: 4, h: 5 },
};
const QUALITY: Record<QualityLevel, Quality> = { low: QUALITY_LOW, medium: QUALITY_MEDIUM, high: QUALITY_HIGH, best: QUALITY_VERY_HIGH };

/** Output pixel size: the short side follows the resolution; sides rounded to even numbers for encoders. */
export function outputSize(res: Resolution, aspect: Aspect) {
  const { w, h } = ASPECTS[aspect], short = RESOLUTIONS[res].short;
  const even = (n: number) => Math.round(n / 2) * 2;
  return w >= h ? { width: even((short * w) / h), height: short } : { width: short, height: even((short * h) / w) };
}

export interface ExportOptions {
  kind: ExportKind;
  range: { start: number; end: number };
  resolution: Resolution;
  aspect: Aspect;
  framing: Framing;
  container: VideoContainer;
  fps: number;
  quality: QualityLevel;
  audio: boolean;
  gifWidth: number;
  gifFps: number;
  imageFormat: ImageFormat;
  imageTime: number;
  audioFormat: AudioFormat;
}

export interface ExportResult { blob: Blob; ext: string; withAudio: boolean; seconds: number; width?: number; height?: number }
export class ExportCancelled extends Error { constructor() { super("Export cancelled"); } }

/** Fail instead of hanging when an encoder stops responding. */
function stall<T>(p: Promise<T>, what: string, ms = 15000): Promise<T> {
  return Promise.race([p, new Promise<T>((_, reject) => setTimeout(() => reject(new Error(`The browser's ${what} encoder stopped responding. Try another format or a lower resolution.`)), ms))]);
}

// ---------- Codec probing ----------
// WebKit reports H.264 Main/High as supported, then stalls after ~7 frames. So every candidate
// gets a real test encode at the actual output size, and codec strings carry a level that fits it.
type Candidate = { codec: VideoCodec; full: string };

function avcLevel(w: number, h: number, fps: number) {
  const frame = Math.ceil(w / 16) * Math.ceil(h / 16), rate = frame * fps;
  const levels: [string, number, number][] = [["1f", 3600, 108000], ["28", 8192, 245760], ["2a", 8704, 522240], ["32", 22080, 589824], ["33", 36864, 983040], ["34", 36864, 2073600]];
  return (levels.find(([, f, r]) => frame <= f && rate <= r) ?? levels[levels.length - 1])[0];
}
function candidates(container: VideoContainer, w: number, h: number, fps: number): Candidate[] {
  const lv = avcLevel(w, h, fps), short = Math.min(w, h);
  const vp9 = `vp09.00.${short <= 720 ? 31 : short <= 1080 ? 41 : 51}.08`;
  return container === "mp4"
    ? [{ codec: "avc", full: `avc1.6400${lv}` }, { codec: "avc", full: `avc1.4d00${lv}` }, { codec: "avc", full: `avc1.4200${lv}` },
       { codec: "hevc", full: "hvc1.1.6.L153.B0" }, { codec: "vp9", full: vp9 }]
    : [{ codec: "vp9", full: vp9 }, { codec: "vp8", full: "vp8" }];
}

async function smokeTest(full: string, w: number, h: number, fps: number): Promise<boolean> {
  const cfg: VideoEncoderConfig = { codec: full, width: w, height: h, bitrate: 4e6, framerate: fps };
  try { if (!(await VideoEncoder.isConfigSupported(cfg)).supported) return false; } catch { return false; }
  let chunks = 0, failed = false;
  const enc = new VideoEncoder({ output: () => { chunks++; }, error: () => { failed = true; } });
  try {
    enc.configure(cfg);
    const c = document.createElement("canvas"); c.width = w; c.height = h;
    const g = c.getContext("2d")!;
    for (let i = 0; i < 15; i++) {
      g.fillStyle = `hsl(${i * 24},60%,50%)`; g.fillRect(0, 0, w, h);
      const frame = new VideoFrame(c, { timestamp: Math.round((i * 1e6) / fps) });
      enc.encode(frame, { keyFrame: i === 0 }); frame.close();
    }
    await Promise.race([enc.flush(), new Promise((r) => setTimeout(r, 2500))]);
  } catch { failed = true; }
  try { enc.close(); } catch {}
  return !failed && chunks >= 12;
}

export interface VideoSupport { video: Candidate | null; audio: AudioCodec | null }
const probed = new Map<string, Promise<VideoSupport>>();
/** Which codecs really work for this container and size (tested once per session). */
export function probeVideo(container: VideoContainer, w: number, h: number, fps: number): Promise<VideoSupport> {
  const key = `${container}:${w}x${h}@${fps}`;
  let p = probed.get(key);
  if (!p) {
    p = (async () => {
      if (typeof VideoEncoder === "undefined") return { video: null, audio: null };
      let video: Candidate | null = null;
      for (const c of candidates(container, w, h, fps)) if (await smokeTest(c.full, w, h, fps)) { video = c; break; }
      const audio = await probeAudio(container === "mp4" ? ["aac", "opus"] : ["opus", "vorbis"]);
      return { video, audio };
    })();
    probed.set(key, p);
  }
  return p;
}
const audioProbed = new Map<string, Promise<AudioCodec | null>>();
export function probeAudio(codecs: AudioCodec[]) {
  const key = codecs.join();
  let p = audioProbed.get(key);
  if (!p) {
    p = typeof AudioEncoder === "undefined" ? Promise.resolve(null)
      : getFirstEncodableAudioCodec(codecs, { numberOfChannels: 2, sampleRate: 48000 }).catch(() => null);
    audioProbed.set(key, p);
  }
  return p;
}

// ---------- Framing: draw the 16:9 edit into any output shape ----------
function makeFramer(outW: number, outH: number, framing: Framing) {
  const out = document.createElement("canvas"); out.width = outW; out.height = outH;
  const og = out.getContext("2d", { willReadFrequently: true })!;
  const exact = Math.abs(outW / outH - W / H) < 0.01;
  // Size of the 16:9 render: cover the output when cropping, fit inside it otherwise.
  const k = exact ? outW / W : framing === "crop" ? Math.max(outW / W, outH / H) : Math.min(outW / W, outH / H);
  const src = exact ? out : document.createElement("canvas");
  if (!exact) { src.width = Math.round(W * k); src.height = Math.round(H * k); }
  const sg = exact ? og : src.getContext("2d", { willReadFrequently: true })!;
  // Blurred fill: shrink to a few pixels, then scale up in two steps so the result is a smooth wash.
  const tiny = document.createElement("canvas"); tiny.width = 12; tiny.height = Math.max(1, Math.round((12 * outH) / outW));
  const mid = document.createElement("canvas"); mid.width = 96; mid.height = Math.max(1, Math.round((96 * outH) / outW));

  return {
    canvas: out,
    draw(t: number) {
      renderFrame(sg, t);
      if (exact) return;
      og.setTransform(1, 0, 0, 1, 0, 0); og.globalAlpha = 1; og.fillStyle = "#000"; og.fillRect(0, 0, outW, outH);
      if (framing === "blur") {
        // A heavily blurred, darkened copy of the frame fills the space around the picture.
        const tg = tiny.getContext("2d")!, cover = Math.max(tiny.width / src.width, tiny.height / src.height);
        tg.imageSmoothingQuality = "high";
        tg.drawImage(src, (tiny.width - src.width * cover) / 2, (tiny.height - src.height * cover) / 2, src.width * cover, src.height * cover);
        const mg = mid.getContext("2d")!; mg.imageSmoothingQuality = "high"; mg.drawImage(tiny, 0, 0, mid.width, mid.height);
        og.imageSmoothingQuality = "high"; og.drawImage(mid, 0, 0, outW, outH);
        og.fillStyle = "rgba(0,0,0,.4)"; og.fillRect(0, 0, outW, outH);
      }
      og.drawImage(src, Math.round((outW - src.width) / 2), Math.round((outH - src.height) / 2));
    },
  };
}

// ---------- Export entry point ----------
export async function exportProject(opts: ExportOptions, signal: AbortSignal, onProgress: (fraction: number, label: string) => void): Promise<ExportResult> {
  pause();
  const end = projectEnd(useEditor.getState().clips);
  if (end <= 0) throw new Error("There's nothing on the timeline to export yet.");
  const start = Math.max(0, Math.min(opts.range.start, end)), stop = Math.max(start, Math.min(opts.range.end, end));
  if (opts.kind !== "image" && stop - start < 0.05) throw new Error("The export range is empty. Choose a longer range.");

  switch (opts.kind) {
    case "image": return exportImage(opts);
    case "audio": return exportAudio(opts, start, stop, signal, onProgress);
    case "gif": return exportGif(opts, start, stop, signal, onProgress);
    default: return exportVideo(opts, start, stop, signal, onProgress);
  }
}

async function exportVideo(opts: ExportOptions, start: number, stop: number, signal: AbortSignal, onProgress: (f: number, l: string) => void): Promise<ExportResult> {
  const { width, height } = outputSize(opts.resolution, opts.aspect), fps = opts.fps;
  onProgress(0, "Checking encoders");
  const support = await probeVideo(opts.container, width, height, fps);
  if (!support.video) throw new Error(`This browser can't encode ${opts.container.toUpperCase()} at ${width}×${height}. Try a lower resolution or the other format.`);

  onProgress(0.01, "Mixing audio");
  const audio = opts.audio && support.audio ? await mixAudio(start, stop, signal) : null;
  if (signal.aborted) throw new ExportCancelled();

  const framer = makeFramer(width, height, opts.framing);
  const output = new Output({
    format: opts.container === "mp4" ? new Mp4OutputFormat({ fastStart: "in-memory" }) : new WebMOutputFormat(),
    target: new BufferTarget(),
  });
  const video = new CanvasSource(framer.canvas, { codec: support.video.codec, fullCodecString: support.video.full, quality: QUALITY[opts.quality] });
  output.addVideoTrack(video, { frameRate: fps });
  const audioSource = audio && support.audio ? new AudioBufferSource({ codec: support.audio, quality: opts.quality === "low" ? QUALITY_LOW : QUALITY_HIGH }) : null;
  if (audioSource) output.addAudioTrack(audioSource);
  await output.start();

  try {
    // Video one exact frame at a time, with audio fed alongside in one-second slices:
    // the file writer interleaves tracks by time and waits for audio that lags behind.
    const frames = Math.max(1, Math.round((stop - start) * fps));
    const slices = audio && audioSource ? sliceAudio(audio, 1) : [];
    let si = 0;
    for (let f = 0; f < frames; f++) {
      if (signal.aborted) throw new ExportCancelled();
      const local = f / fps, t = start + local;
      while (audioSource && si < slices.length && si <= local + 1) await stall(audioSource.add(slices[si++]), "audio");
      await seekVideosForExport(t);
      framer.draw(t);
      await stall(video.add(local, 1 / fps), "video");
      if (f % 3 === 0 || f === frames - 1) onProgress(0.03 + 0.93 * ((f + 1) / frames), `Rendering frame ${f + 1} of ${frames}`);
    }
    while (audioSource && si < slices.length) await stall(audioSource.add(slices[si++]), "audio");
    onProgress(0.98, "Finishing the file");
    await stall(output.finalize(), "video", 60000);
  } catch (err) {
    await output.cancel().catch(() => {});
    throw err;
  }
  const buffer = (output.target as BufferTarget).buffer;
  if (!buffer) throw new Error("The encoder didn't produce a file.");
  onProgress(1, "Done");
  return { blob: new Blob([buffer], { type: opts.container === "mp4" ? "video/mp4" : "video/webm" }), ext: opts.container, withAudio: !!audioSource, seconds: stop - start, width, height };
}

async function exportGif(opts: ExportOptions, start: number, stop: number, signal: AbortSignal, onProgress: (f: number, l: string) => void): Promise<ExportResult> {
  const { w, h } = ASPECTS[opts.aspect];
  const width = Math.round(opts.gifWidth / 2) * 2, height = Math.round((opts.gifWidth * h) / w / 2) * 2;
  const framer = makeFramer(width, height, opts.framing);
  const g = framer.canvas.getContext("2d", { willReadFrequently: true })!;
  const gif = GIFEncoder();
  const frames = Math.max(1, Math.round((stop - start) * opts.gifFps)), delay = Math.round(1000 / opts.gifFps);
  for (let f = 0; f < frames; f++) {
    if (signal.aborted) throw new ExportCancelled();
    const t = start + f / opts.gifFps;
    await seekVideosForExport(t);
    framer.draw(t);
    const { data } = g.getImageData(0, 0, width, height);
    const palette = quantize(data, 256);
    gif.writeFrame(applyPalette(data, palette), width, height, { palette, delay });
    if (f % 2 === 0 || f === frames - 1) onProgress(((f + 1) / frames) * 0.97, `Rendering frame ${f + 1} of ${frames}`);
    if (f % 4 === 3) await new Promise((r) => setTimeout(r, 0)); // keep the page responsive
  }
  gif.finish();
  onProgress(1, "Done");
  return { blob: new Blob([gif.bytes() as BlobPart], { type: "image/gif" }), ext: "gif", withAudio: false, seconds: stop - start, width, height };
}

async function exportImage(opts: ExportOptions): Promise<ExportResult> {
  const { width, height } = outputSize(opts.resolution, opts.aspect);
  const framer = makeFramer(width, height, opts.framing);
  await seekVideosForExport(opts.imageTime);
  framer.draw(opts.imageTime);
  const type = opts.imageFormat === "png" ? "image/png" : "image/jpeg";
  const blob = await new Promise<Blob | null>((r) => framer.canvas.toBlob(r, type, 0.92));
  if (!blob) throw new Error("The browser couldn't create the image.");
  return { blob, ext: opts.imageFormat === "png" ? "png" : "jpg", withAudio: false, seconds: 0, width, height };
}

async function exportAudio(opts: ExportOptions, start: number, stop: number, signal: AbortSignal, onProgress: (f: number, l: string) => void): Promise<ExportResult> {
  onProgress(0.05, "Mixing audio");
  const buf = await mixAudio(start, stop, signal);
  if (signal.aborted) throw new ExportCancelled();
  if (!buf) throw new Error("There's no audio in this range to export. Add music, or unmute a track.");
  if (opts.audioFormat === "wav") {
    onProgress(0.9, "Writing WAV");
    return { blob: encodeWav(buf), ext: "wav", withAudio: true, seconds: stop - start };
  }
  const codec = await probeAudio(["aac"]);
  if (!codec) throw new Error("This browser can't encode M4A audio. Choose WAV instead.");
  onProgress(0.5, "Encoding audio");
  const output = new Output({ format: new Mp4OutputFormat({ fastStart: "in-memory" }), target: new BufferTarget() });
  const src = new AudioBufferSource({ codec, quality: QUALITY[opts.quality] });
  output.addAudioTrack(src);
  await output.start();
  await stall(src.add(buf), "audio", 60000);
  await stall(output.finalize(), "audio", 60000);
  const buffer = (output.target as BufferTarget).buffer!;
  onProgress(1, "Done");
  return { blob: new Blob([buffer], { type: "audio/mp4" }), ext: "m4a", withAudio: true, seconds: stop - start };
}

// ---------- Audio ----------
/** Mix music, imported audio and the sound of imported videos for [start, stop) into one stereo buffer. */
async function mixAudio(start: number, stop: number, signal: AbortSignal): Promise<AudioBuffer | null> {
  const s = useEditor.getState();
  const audible = s.clips.filter((c) => (c.type === "music" || c.type === "audio" || c.type === "video") && !trackOf(s, c.track).muted && c.volume > 0
    && c.start < stop && c.start + c.dur > start);
  if (!audible.length) return null;
  const sr = 48000;
  const ctx = new OfflineAudioContext(2, Math.max(1, Math.ceil((stop - start) * sr)), sr);
  const master = ctx.createGain(); master.gain.value = 0.8; master.connect(ctx.destination);
  const at = (t: number) => t - start; // timeline time → output time

  for (const c of audible) {
    if (signal.aborted) return null;
    if (c.type === "music") {
      const a = ASSETS.get(c.asset); if (!a?.bpm) continue;
      const beat = 60 / a.bpm;
      for (let b = Math.max(0, Math.ceil(c.in / beat)); ; b++) {
        const tt = c.start - c.in + b * beat;
        if (tt >= c.start + c.dur || tt >= stop) break;
        if (tt < c.start || tt < start) continue;
        playBeat(ctx, master, a, b, at(tt), c.volume * fadeAt(c, tt));
      }
      continue;
    }
    const blob = await assetBlob(c.asset); if (!blob) continue;
    let buf: AudioBuffer;
    try { buf = await ctx.decodeAudioData(await blob.arrayBuffer()); } catch { continue; } // e.g. a video with no sound
    const src = ctx.createBufferSource(); src.buffer = buf;
    const g = ctx.createGain();
    const from = Math.max(c.start, start), until = Math.min(c.start + c.dur, stop);
    // Volume envelope with fades, evaluated in timeline time.
    g.gain.setValueAtTime(c.volume * fadeAt(c, from), at(from));
    if (c.fadeIn > 0 && c.start + c.fadeIn > from) g.gain.linearRampToValueAtTime(c.volume, at(Math.min(until, c.start + c.fadeIn)));
    if (c.fadeOut > 0 && c.start + c.dur - c.fadeOut < until) {
      const fo = Math.max(from, c.start + c.dur - c.fadeOut);
      g.gain.setValueAtTime(c.volume * fadeAt(c, fo), at(fo));
      g.gain.linearRampToValueAtTime(c.volume * fadeAt(c, until), at(until));
    }
    src.connect(g).connect(master);
    const offset = c.in + (from - c.start);
    if (offset < buf.duration) src.start(at(from), offset, Math.min(until - from, buf.duration - offset));
  }
  return ctx.startRendering();
}

function sliceAudio(buf: AudioBuffer, seconds: number): AudioBuffer[] {
  const per = Math.round(buf.sampleRate * seconds), out: AudioBuffer[] = [];
  for (let s = 0; s < buf.length; s += per) {
    const length = Math.min(per, buf.length - s);
    const slice = new AudioBuffer({ length, sampleRate: buf.sampleRate, numberOfChannels: buf.numberOfChannels });
    for (let ch = 0; ch < buf.numberOfChannels; ch++) slice.copyToChannel(buf.getChannelData(ch).subarray(s, s + length), ch);
    out.push(slice);
  }
  return out;
}

/** 16-bit PCM WAV. */
function encodeWav(buf: AudioBuffer): Blob {
  const ch = buf.numberOfChannels, len = buf.length, bytes = 44 + len * ch * 2;
  const v = new DataView(new ArrayBuffer(bytes));
  const str = (o: number, s: string) => { for (let i = 0; i < s.length; i++) v.setUint8(o + i, s.charCodeAt(i)); };
  str(0, "RIFF"); v.setUint32(4, bytes - 8, true); str(8, "WAVE"); str(12, "fmt ");
  v.setUint32(16, 16, true); v.setUint16(20, 1, true); v.setUint16(22, ch, true); v.setUint32(24, buf.sampleRate, true);
  v.setUint32(28, buf.sampleRate * ch * 2, true); v.setUint16(32, ch * 2, true); v.setUint16(34, 16, true);
  str(36, "data"); v.setUint32(40, len * ch * 2, true);
  const data = Array.from({ length: ch }, (_, i) => buf.getChannelData(i));
  let o = 44;
  for (let i = 0; i < len; i++) for (let c = 0; c < ch; c++) {
    const x = Math.max(-1, Math.min(1, data[c][i]));
    v.setInt16(o, x < 0 ? x * 0x8000 : x * 0x7fff, true); o += 2;
  }
  return new Blob([v.buffer], { type: "audio/wav" });
}

/** Save a blob through the browser's normal download. */
export function downloadBlob(blob: Blob, filename: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url; a.download = filename; a.rel = "noopener";
  document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
