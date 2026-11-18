import sharp from "sharp";

// The hero's halftone (components/halftone-reel.tsx) for still images: the same tone curve,
// sharpening and tone-grouped dots, written out as an SVG at build time.

const MAX_DOT = 0.84;
const SHARPEN = 1.4;
const TONES = [0.4, 0.62, 0.82, 1];

const clamp = (x: number) => Math.min(1, Math.max(0, x));
const smoothstep = (a: number, b: number, x: number) => {
  const t = clamp((x - a) / (b - a));
  return t * t * (3 - 2 * t);
};

export type HalftoneOptions = {
  src: string;
  width: number;
  height: number;
  cell?: number;
  dark?: boolean;
  ink?: string;
  // Where the dots fade out: an ellipse centred at (cx, cy) in 0..1, reaching rx, ry.
  fade?: { cx: number; cy: number; rx: number; ry: number };
  // Which part of the photo to keep when cropping to the frame.
  position?: string;
};

export async function halftoneSvg({
  src,
  width,
  height,
  cell = 6,
  dark = false,
  ink = dark ? "#f2f2f2" : "#0a0a0a",
  fade = { cx: 0.5, cy: 0.5, rx: 1, ry: 1 },
  position = "centre",
}: HalftoneOptions): Promise<string> {
  const cols = Math.floor(width / cell), rows = Math.floor(height / cell);
  const { data } = await sharp(src)
    .resize(cols, rows, { fit: "cover", position })
    .removeAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });

  const n = cols * rows;
  const lum = new Float32Array(n), blur = new Float32Array(n);
  for (let k = 0; k < n; k++) lum[k] = (0.299 * data[k * 3] + 0.587 * data[k * 3 + 1] + 0.114 * data[k * 3 + 2]) / 255;
  for (let y = 0; y < rows; y++) {
    for (let x = 0; x < cols; x++) {
      let sum = 0, c = 0;
      for (let dy = -1; dy <= 1; dy++) {
        for (let dx = -1; dx <= 1; dx++) {
          const yy = y + dy, xx = x + dx;
          if (yy < 0 || yy >= rows || xx < 0 || xx >= cols) continue;
          sum += lum[yy * cols + xx];
          c++;
        }
      }
      blur[y * cols + x] = sum / c;
    }
  }

  const paths = TONES.map(() => [] as string[]);
  const ox = (width - cols * cell) / 2, oy = (height - rows * cell) / 2;
  for (let y = 0; y < rows; y++) {
    for (let x = 0; x < cols; x++) {
      const k = y * cols + x;
      const l = clamp(lum[k] + SHARPEN * (lum[k] - blur[k]));
      const nx = ((x + 0.5) / cols - fade.cx) / fade.rx, ny = ((y + 0.5) / rows - fade.cy) / fade.ry;
      const edge = 1 - smoothstep(0.45, 1, Math.hypot(nx, ny));
      const tone = dark ? smoothstep(0.12, 0.95, l) * 0.92 : smoothstep(0.08, 0.9, 1 - l);
      const d = tone * edge;
      if (d < 0.04) continue;
      const s = +(cell * MAX_DOT * Math.sqrt(d)).toFixed(2);
      const px = +(ox + x * cell + (cell - s) / 2).toFixed(2), py = +(oy + y * cell + (cell - s) / 2).toFixed(2);
      paths[Math.min(TONES.length - 1, Math.floor(d * TONES.length))].push(`M${px} ${py}h${s}v${s}h-${s}z`);
    }
  }

  const body = paths
    .map((p, i) => (p.length ? `<path fill="${ink}" fill-opacity="${TONES[i]}" d="${p.join("")}"/>` : ""))
    .join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">${body}</svg>`;
}

export const svgDataUri = (svg: string) => `data:image/svg+xml;base64,${Buffer.from(svg).toString("base64")}`;
