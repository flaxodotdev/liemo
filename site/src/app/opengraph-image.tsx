import { OG_SIZE, renderOg } from "@/lib/og";

// The share card: the hero, light, drawn at build time (src/lib/og.tsx).
export const alt = "Liemo: the video editor for the AI era";
export const size = OG_SIZE;
export const contentType = "image/png";
export const dynamic = "force-static";

export default function Image() {
  return renderOg("hero-light");
}
