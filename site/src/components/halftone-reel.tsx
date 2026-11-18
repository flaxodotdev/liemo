"use client";

import { useEffect, useRef } from "react";

// The sample footage as a halftone, played the way Liemo plays it: slow camera moves,
// dissolving from one shot into the next, with the timeline's timecode underneath.

const SHOTS = [
  { src: "/reel/snow-peaks.jpg", name: "Snow peaks" },
  { src: "/reel/forest-waterfall.jpg", name: "Forest waterfall" },
  { src: "/reel/coastal-cliffs.jpg", name: "Coastal cliffs" },
  { src: "/reel/canyon-sunset.jpg", name: "Canyon sunset" },
];
const CLIP = 4.5; // seconds per shot
const FADE = 1; // dissolve length
const FPS = 30;
const CELL = 5; // CSS px per dot
const MAX_DOT = 0.84; // of a cell, so even solid areas keep the grid's texture
const SHARPEN = 1.4; // unsharp mask strength: brings out edges and texture
const TONES = [0.4, 0.62, 0.82, 1]; // dot opacity by tone, light to dense

const clamp = (x: number) => Math.min(1, Math.max(0, x));
const smoothstep = (a: number, b: number, x: number) => {
  const t = clamp((x - a) / (b - a));
  return t * t * (3 - 2 * t);
};

function timecode(s: number) {
  const f = Math.floor(s * FPS);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `00:${pad(Math.floor(f / FPS / 60))}:${pad(Math.floor(f / FPS) % 60)}:${pad(f % FPS)}`;
}

export function HalftoneReel() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const nameRef = useRef<HTMLSpanElement>(null);
  const tcRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    const canvas = canvasRef.current!;
    const ctx = canvas.getContext("2d")!;
    const small = document.createElement("canvas");
    const sctx = small.getContext("2d", { willReadFrequently: true })!;
    const images = SHOTS.map((s) => Object.assign(new Image(), { src: s.src }));
    const reduce = matchMedia("(prefers-reduced-motion: reduce)").matches;

    let cols = 0, rows = 0, w = 0, h = 0;
    let lum = new Float32Array(0), blur = new Float32Array(0);
    function resize() {
      const dpr = devicePixelRatio || 1;
      const r = canvas.getBoundingClientRect();
      w = r.width;
      h = r.height;
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      cols = Math.max(1, Math.floor(w / CELL));
      rows = Math.max(1, Math.floor(h / CELL));
      small.width = cols;
      small.height = rows;
      lum = new Float32Array(cols * rows);
      blur = new Float32Array(cols * rows);
    }

    // Light: ink dots for the shadows. Dark: light dots for the highlights, so the footage
    // reads the right way round on black instead of as a negative.
    const style = getComputedStyle(document.documentElement);
    let ink = "#0a0a0a", dark = false;
    function theme() {
      ink = style.getPropertyValue("--ink").trim() || ink;
      dark = style.getPropertyValue("color-scheme").trim() === "dark";
    }

    // A shot at progress p (0..1) of its slow pan, scaled to cover the dot grid.
    function shot(i: number, p: number, alpha: number) {
      const img = images[((i % images.length) + images.length) % images.length];
      if (!img.complete || !img.naturalWidth) return;
      const s = Math.max(cols / img.naturalWidth, rows / img.naturalHeight) * 1.2;
      const dw = img.naturalWidth * s, dh = img.naturalHeight * s;
      sctx.globalAlpha = alpha;
      sctx.drawImage(img, -(dw - cols) * p, -(dh - rows) * (0.35 + 0.3 * p), dw, dh);
    }

    let lastT = 0;
    function draw(t: number) {
      lastT = t;
      const total = CLIP * SHOTS.length;
      const tt = ((t % total) + total) % total;
      const i = Math.floor(tt / CLIP);
      const local = tt - i * CLIP;
      const pan = (x: number) => (x + FADE) / (CLIP + FADE);

      sctx.globalAlpha = 1;
      sctx.fillStyle = dark ? "#000" : "#fff";
      sctx.fillRect(0, 0, cols, rows);
      shot(i, pan(local), 1);
      if (local > CLIP - FADE) shot(i + 1, pan(local - CLIP), smoothstep(CLIP - FADE, CLIP, local));

      // Brightness per cell, then a 3×3 box blur for the unsharp mask.
      const px = sctx.getImageData(0, 0, cols, rows).data;
      for (let k = 0; k < cols * rows; k++) {
        lum[k] = (0.299 * px[k * 4] + 0.587 * px[k * 4 + 1] + 0.114 * px[k * 4 + 2]) / 255;
      }
      for (let y = 0; y < rows; y++) {
        for (let x = 0; x < cols; x++) {
          let sum = 0, n = 0;
          for (let dy = -1; dy <= 1; dy++) {
            const yy = y + dy;
            if (yy < 0 || yy >= rows) continue;
            for (let dx = -1; dx <= 1; dx++) {
              const xx = x + dx;
              if (xx < 0 || xx >= cols) continue;
              sum += lum[yy * cols + xx];
              n++;
            }
          }
          blur[y * cols + x] = sum / n;
        }
      }

      // Dots grouped by tone, one path per tone, so a frame is four fills.
      const paths = TONES.map(() => new Path2D());
      const ox = (w - cols * CELL) / 2, oy = (h - rows * CELL) / 2;
      for (let y = 0; y < rows; y++) {
        for (let x = 0; x < cols; x++) {
          const k = y * cols + x;
          const l = clamp(lum[k] + SHARPEN * (lum[k] - blur[k]));
          // Fade to the page toward the edges, like ink running out.
          const nx = ((x + 0.5) / cols) * 2 - 1, ny = ((y + 0.5) / rows) * 2 - 1;
          const edge = 1 - smoothstep(0.45, 1, Math.hypot(nx * 0.9, ny * 1.08));
          const tone = dark ? smoothstep(0.12, 0.95, l) * 0.92 : smoothstep(0.08, 0.9, 1 - l);
          const d = tone * edge;
          if (d < 0.04) continue;
          const s = CELL * MAX_DOT * Math.sqrt(d);
          const bucket = Math.min(TONES.length - 1, Math.floor(d * TONES.length));
          paths[bucket].rect(ox + x * CELL + (CELL - s) / 2, oy + y * CELL + (CELL - s) / 2, s, s);
        }
      }
      ctx.clearRect(0, 0, w, h);
      ctx.fillStyle = ink;
      paths.forEach((p, b) => {
        ctx.globalAlpha = TONES[b];
        ctx.fill(p);
      });
      ctx.globalAlpha = 1;

      if (nameRef.current) nameRef.current.textContent = SHOTS[local > CLIP - FADE / 2 ? (i + 1) % SHOTS.length : i].name;
      if (tcRef.current) tcRef.current.textContent = timecode(tt);
    }

    let raf = 0, visible = true, last = 0, start = -1;
    function loop(now: number) {
      raf = requestAnimationFrame(loop);
      if (start < 0) start = now;
      if (!visible || now - last < 1000 / 24) return;
      last = now;
      draw((now - start) / 1000);
    }

    theme();
    resize();
    const redraw = () => draw(reduce ? 1.5 : lastT);
    const themeChanged = () => {
      theme();
      redraw();
    };
    const scheme = matchMedia("(prefers-color-scheme: dark)");
    scheme.addEventListener("change", themeChanged);
    const mo = new MutationObserver(themeChanged);
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    const ro = new ResizeObserver(() => {
      resize();
      redraw();
    });
    ro.observe(canvas);
    const io = new IntersectionObserver(([e]) => (visible = e.isIntersecting));
    io.observe(canvas);
    images.forEach((img) => img.addEventListener("load", redraw));
    if (!reduce) raf = requestAnimationFrame(loop);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      io.disconnect();
      mo.disconnect();
      scheme.removeEventListener("change", themeChanged);
    };
  }, []);

  return (
    <figure className="mx-auto w-full max-w-[900px]">
      <canvas
        ref={canvasRef}
        role="img"
        aria-label="Nature footage drawn as a halftone, panning slowly and dissolving from shot to shot."
        className="block aspect-[16/9] w-full"
      />
      <figcaption className="mt-2 flex items-center justify-center gap-3 font-mono text-[11px] uppercase tracking-[0.08em] text-muted">
        <span ref={nameRef}>{SHOTS[0].name}</span>
        <span className="h-px w-6 bg-line" />
        <span ref={tcRef} className="tabular-nums">00:00:00:00</span>
      </figcaption>
    </figure>
  );
}
