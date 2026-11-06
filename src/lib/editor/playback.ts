"use client";

import { toast } from "sonner";
import { ASSETS, CHORDS, type Asset } from "./assets";
import { FPS, snapFrame } from "./time";
import { projectEnd, trackOf, useEditor } from "./store";
import { fadeAt, render, syncMedia } from "./render";

// ---------- Synth ----------
let actx: AudioContext | null = null;
let bus: GainNode | null = null;
let schedFrom = 0, playT0 = 0, ctxT0 = 0;

function ensureAudio() {
  if (!actx) {
    const AC = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!AC) return null;
    actx = new AC();
  }
  if (actx.state === "suspended") actx.resume();
  return actx;
}
const mtof = (m: number) => 440 * Math.pow(2, (m - 69) / 12);

function tone(ac: BaseAudioContext, when: number, freq: number, len: number, type: OscillatorType, gain: number, dest: AudioNode) {
  const o = ac.createOscillator(), g = ac.createGain();
  o.type = type; o.frequency.value = freq;
  g.gain.setValueAtTime(0, when);
  g.gain.linearRampToValueAtTime(gain, when + Math.min(0.02, len / 4));
  g.gain.exponentialRampToValueAtTime(0.0001, when + len);
  o.connect(g).connect(dest); o.start(when); o.stop(when + len + 0.05);
}

const noiseBuffers = new WeakMap<BaseAudioContext, AudioBuffer>();
function noiseFor(ac: BaseAudioContext) {
  let b = noiseBuffers.get(ac);
  if (!b) {
    b = ac.createBuffer(1, Math.round(ac.sampleRate * 0.2), ac.sampleRate);
    const d = b.getChannelData(0); for (let i = 0; i < d.length; i++) d[i] = Math.random() * 2 - 1;
    noiseBuffers.set(ac, b);
  }
  return b;
}

/** One beat of a music asset: pad on the bar, bass, arp, kick and hats. Works on live and offline contexts. */
export function playBeat(ac: BaseAudioContext, dest: AudioNode, a: Asset, b: number, when: number, vol: number) {
  const out = ac.createGain(); out.gain.value = vol; out.connect(dest);
  const beat = 60 / a.bpm!, roots = a.roots!, root = roots[Math.floor(b / 4) % roots.length], ch = CHORDS[a.quality!];
  if (b % 4 === 0) ch.forEach((n) => tone(ac, when, mtof(root + 12 + n), beat * 4, "triangle", 0.05, out));
  tone(ac, when, mtof(root - 12), beat * 0.9, "sine", 0.22, out);
  tone(ac, when + beat / 2, mtof(root + 24 + ch[(b * 2 + 1) % ch.length]), beat * 0.4, "square", 0.018, out);
  if (b % 2 === 0) {
    const o = ac.createOscillator(), g = ac.createGain();
    o.frequency.setValueAtTime(140, when); o.frequency.exponentialRampToValueAtTime(40, when + 0.12);
    g.gain.setValueAtTime(0.5, when); g.gain.exponentialRampToValueAtTime(0.001, when + 0.18);
    o.connect(g).connect(out); o.start(when); o.stop(when + 0.2);
  }
  [0, 0.5].forEach((off) => {
    const s = ac.createBufferSource(), hp = ac.createBiquadFilter(), g = ac.createGain(), at = when + off * beat;
    s.buffer = noiseFor(ac); hp.type = "highpass"; hp.frequency.value = 7000;
    g.gain.setValueAtTime(0.06, at); g.gain.exponentialRampToValueAtTime(0.001, at + 0.05);
    s.connect(hp).connect(g).connect(out); s.start(at); s.stop(at + 0.06);
  });
}

function scheduleMusic() {
  const ac = actx, out = bus;
  if (!ac || !out) return;
  const s = useEditor.getState(), ahead = s.time + 0.4;
  s.clips.forEach((c) => {
    if (c.type !== "music" || trackOf(s, c.track).muted) return;
    const a = ASSETS.get(c.asset)!, beat = 60 / a.bpm!;
    for (let b = Math.max(0, Math.ceil((schedFrom - c.start + c.in) / beat)); ; b++) {
      const tt = c.start - c.in + b * beat;
      if (tt >= ahead || tt >= c.start + c.dur) break;
      if (tt < schedFrom || tt < c.start) continue;
      const when = ctxT0 + (tt - playT0);
      if (when < ac.currentTime - 0.01) continue;
      playBeat(ac, out, a, b, Math.max(when, ac.currentTime), c.volume * fadeAt(c, tt));
    }
  });
  schedFrom = ahead;
}
function startBus() {
  if (!actx) return;
  bus = actx.createGain(); bus.gain.value = 0.8; bus.connect(actx.destination);
  playT0 = useEditor.getState().time; ctxT0 = actx.currentTime + 0.05; schedFrom = playT0;
}
function stopBus() {
  if (bus && actx) { const b = bus; b.gain.setTargetAtTime(0, actx.currentTime, 0.03); setTimeout(() => b.disconnect(), 200); }
  bus = null;
}

// ---------- Library preview ----------
let previewBus: GainNode | null = null, previewTimer: ReturnType<typeof setTimeout> | undefined;
export function previewMusic(id: string | null, onEnd: () => void) {
  previewBus?.disconnect(); previewBus = null; clearTimeout(previewTimer);
  if (!id) return;
  const ac = ensureAudio();
  if (!ac) { toast("Audio playback isn't available in this browser"); onEnd(); return; }
  const a = ASSETS.get(id)!, beat = 60 / a.bpm!, t0 = ac.currentTime + 0.05;
  previewBus = ac.createGain(); previewBus.gain.value = 0.8; previewBus.connect(ac.destination);
  for (let b = 0; b < 16; b++) playBeat(ac, previewBus, a, b, t0 + b * beat, 0.8);
  previewTimer = setTimeout(() => { previewBus?.disconnect(); previewBus = null; onEnd(); }, beat * 16000 + 300);
}

// ---------- Transport ----------
export function play() {
  ensureLoop();
  const s = useEditor.getState();
  if (s.time >= projectEnd(s.clips) - 1 / FPS) useEditor.setState({ time: 0 });
  useEditor.setState({ playing: true });
  if (ensureAudio()) startBus();
}
export function pause() {
  useEditor.setState({ playing: false });
  stopBus(); syncMedia();
}
export const togglePlay = () => (useEditor.getState().playing ? pause() : play());
export function seek(t: number) {
  const s = useEditor.getState();
  useEditor.setState({ time: Math.max(0, Math.min(t, Math.max(projectEnd(s.clips), 1))) });
  if (s.playing && actx) { stopBus(); startBus(); }
  syncMedia(); render();
}
export function step(frames: number) {
  if (useEditor.getState().playing) pause();
  seek(snapFrame(useEditor.getState().time + frames / FPS));
}
export function restartAudio() { if (useEditor.getState().playing && actx) { stopBus(); startBus(); } }

let raf = 0, last = 0, running = false, failures = 0;
function frame(now: number) {
  // Schedule the next frame first, so one bad frame can never stop playback for good.
  raf = requestAnimationFrame(frame);
  const dt = Math.min(0.25, (now - last) / 1000);
  last = now;
  const s = useEditor.getState();
  if (!s.playing) return;
  try {
    let t = s.time + dt;
    const end = projectEnd(s.clips);
    if (t >= end) {
      if (s.loop) { useEditor.setState({ time: 0 }); restartAudio(); t = 0; }
      else { t = Math.max(0, end - 1 / FPS); useEditor.setState({ time: t }); pause(); }
    }
    if (useEditor.getState().playing) useEditor.setState({ time: t });
    scheduleMusic(); syncMedia(); render();
    failures = 0;
  } catch (err) {
    console.error("Liemo: playback frame failed", err);
    if (++failures > 10) { pause(); toast("Playback stopped because a frame couldn't be drawn. Check the console for details."); failures = 0; }
  }
}
function ensureLoop() {
  if (running) return;
  running = true;
  last = performance.now();
  raf = requestAnimationFrame(frame);
}
export function startLoop() {
  ensureLoop();
  // Redraw whenever the edit changes while paused.
  const unsub = useEditor.subscribe((s, p) => {
    if (s.playing) return;
    if (s.clips !== p.clips || s.tracks !== p.tracks || s.time !== p.time) { syncMedia(); render(); }
  });
  return () => { cancelAnimationFrame(raf); running = false; unsub(); pause(); };
}
