import { readFile } from "node:fs/promises";
import path from "node:path";
import sharp from "sharp";
import { ImageResponse } from "next/og";
import { halftoneSvg, svgDataUri } from "@/lib/halftone";

// Open Graph images, drawn at build time in the site's own language: serif display type,
// hairline frame, and the hero's halftone footage. 1200×630, light or dark.

export const OG_SIZE = { width: 1200, height: 630 };

export const OG_VARIANTS = ["split-light", "split-dark", "hero-light", "hero-dark", "product-light", "product-dark"] as const;
export type OgVariant = (typeof OG_VARIANTS)[number];

const ROOT = process.cwd();
const asset = (...p: string[]) => path.join(ROOT, ...p);

const THEMES = {
  light: { paper: "#ffffff", ink: "#0a0a0a", muted: "#7a7a7a", line: "#e6e6e6" },
  dark: { paper: "#0a0a0a", ink: "#f2f2f2", muted: "#8c8c8c", line: "#232323" },
};
type Theme = (typeof THEMES)["light"];

async function fonts() {
  const f = (file: string) => readFile(asset("assets/fonts", file));
  return [
    { name: "Serif", data: await f("source-serif-400.ttf"), weight: 400 as const, style: "normal" as const },
    { name: "Inter", data: await f("inter-400.ttf"), weight: 400 as const, style: "normal" as const },
    { name: "Inter", data: await f("inter-500.ttf"), weight: 500 as const, style: "normal" as const },
    { name: "Mono", data: await f("jetbrains-mono-400.ttf"), weight: 400 as const, style: "normal" as const },
  ];
}

// "The video editor for the AI era", word by word so "AI" can be muted and the line still wraps.
function Headline({ t, size, center = false }: { t: Theme; size: number; center?: boolean }) {
  const words = "The video editor for the AI era".split(" ");
  return (
    <div
      style={{
        display: "flex",
        flexWrap: "wrap",
        justifyContent: center ? "center" : "flex-start",
        fontFamily: "Serif",
        fontSize: size,
        lineHeight: 1.02,
        letterSpacing: "-0.025em",
        color: t.ink,
      }}
    >
      {words.map((w, i) => (
        <span key={i} style={{ color: w === "AI" ? t.muted : t.ink, marginRight: i < words.length - 1 ? size * 0.24 : 0 }}>
          {w}
        </span>
      ))}
    </div>
  );
}

function Logo({ t, size = 30 }: { t: Theme; size?: number }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: size * 0.3 }}>
      <svg width={size * 0.68} height={size} viewBox="296 163 161 237" fill={t.ink}>
        <path d="M371 163 400 242 325 400 296 321Z" />
        <path d="M325 400 428 321 457 400Z" />
      </svg>
      <span style={{ fontFamily: "Serif", fontSize: size * 1.15, color: t.ink, letterSpacing: "-0.01em" }}>Liemo</span>
    </div>
  );
}

function Label({ t, children }: { t: Theme; children: string }) {
  return (
    <span style={{ fontFamily: "Mono", fontSize: 15, letterSpacing: "0.1em", textTransform: "uppercase", color: t.muted }}>
      {children}
    </span>
  );
}

// The frame: hairlines down both sides and across top and bottom bands, like the site.
function Frame({ t, children, top = 92, bottom = 76 }: { t: Theme; children: React.ReactNode; top?: number; bottom?: number }) {
  const { width: W, height: H } = OG_SIZE;
  const line = (s: React.CSSProperties) => <div style={{ position: "absolute", background: t.line, ...s }} />;
  return (
    <div style={{ display: "flex", position: "relative", width: W, height: H, background: t.paper, fontFamily: "Inter" }}>
      {children}
      {line({ left: 48, top: 0, width: 1, height: H })}
      {line({ right: 48, top: 0, width: 1, height: H })}
      {line({ left: 0, top, width: W, height: 1 })}
      {line({ left: 0, top: H - bottom, width: W, height: 1 })}
    </div>
  );
}

function TopBand({ t }: { t: Theme }) {
  return (
    <div
      style={{
        position: "absolute",
        left: 80,
        right: 80,
        top: 0,
        height: 92,
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
      }}
    >
      <Logo t={t} />
      <Label t={t}>Open source · MIT</Label>
    </div>
  );
}

function BottomBand({ t }: { t: Theme }) {
  return (
    <div
      style={{
        position: "absolute",
        left: 80,
        right: 80,
        bottom: 0,
        height: 76,
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
      }}
    >
      <Label t={t}>Mac · Windows · Linux · iPhone</Label>
      <Label t={t}>MCP server built in</Label>
    </div>
  );
}

async function split(t: Theme, dark: boolean) {
  const W = 600, H = 462;
  const dots = await halftoneSvg({
    src: asset("public/reel/coastal-cliffs.jpg"),
    width: W,
    height: H,
    cell: 5,
    dark,
    ink: t.ink,
    fade: { cx: 0.52, cy: 0.5, rx: 0.6, ry: 0.66 },
  });
  return (
    <Frame t={t}>
      <TopBand t={t} />
      <div style={{ position: "absolute", left: 80, top: 92, width: 540, height: 462, display: "flex", flexDirection: "column", justifyContent: "center" }}>
        <Headline t={t} size={80} />
        <div style={{ marginTop: 28, maxWidth: 430, fontSize: 24, lineHeight: 1.45, color: t.muted, display: "flex" }}>
          A native editor you drive yourself, or hand to your AI agent.
        </div>
      </div>
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img src={svgDataUri(dots)} width={W} height={H} style={{ position: "absolute", right: 48, top: 92 }} alt="" />
      <BottomBand t={t} />
    </Frame>
  );
}

async function hero(t: Theme, dark: boolean) {
  const W = 1104, H = 340;
  const dots = await halftoneSvg({
    src: asset("public/reel/coastal-cliffs.jpg"),
    width: W,
    height: H,
    cell: 5,
    dark,
    ink: t.ink,
    fade: { cx: 0.5, cy: 0.6, rx: 0.48, ry: 0.7 },
  });
  return (
    <Frame t={t} top={92} bottom={0}>
      <TopBand t={t} />
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img src={svgDataUri(dots)} width={W} height={H} style={{ position: "absolute", left: 48, bottom: 0 }} alt="" />
      <div style={{ position: "absolute", left: 230, right: 230, top: 122, display: "flex", flexDirection: "column", alignItems: "center" }}>
        <Headline t={t} size={76} center />
      </div>
    </Frame>
  );
}

async function product(t: Theme) {
  // The top of the real editor screenshot, rising from the bottom edge.
  const shot = await sharp(asset("public/editor.webp")).extract({ left: 0, top: 0, width: 2880, height: 1180 }).resize(1040).png().toBuffer();
  return (
    <Frame t={t} top={92} bottom={0}>
      <TopBand t={t} />
      <div style={{ position: "absolute", left: 120, right: 120, top: 128, display: "flex", flexDirection: "column", alignItems: "center" }}>
        <Headline t={t} size={64} center />
        <div style={{ marginTop: 14, fontSize: 22, color: t.muted, display: "flex" }}>
          A native editor you drive yourself, or hand to your AI agent.
        </div>
      </div>
      <div
        style={{
          position: "absolute",
          left: 80,
          top: 268,
          width: 1040,
          height: 426,
          display: "flex",
          borderRadius: 14,
          overflow: "hidden",
          border: `1px solid ${t.line}`,
          boxShadow: "0 30px 80px -30px rgba(0,0,0,0.45)",
        }}
      >
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img src={`data:image/png;base64,${shot.toString("base64")}`} width={1040} height={426} style={{ objectFit: "cover", objectPosition: "top" }} alt="" />
      </div>
    </Frame>
  );
}

export async function renderOg(variant: OgVariant) {
  const dark = variant.endsWith("dark");
  const t = THEMES[dark ? "dark" : "light"];
  const kind = variant.split("-")[0];
  const node = kind === "split" ? await split(t, dark) : kind === "hero" ? await hero(t, dark) : await product(t);
  return new ImageResponse(node, { ...OG_SIZE, fonts: await fonts() });
}
