# Liemo

An open-source video editor for macOS, with an AI-editing MCP server built in.

- **The app**, native and GPU-accelerated, lives in [`native/`](native/): Rust + GPUI, FFmpeg, Metal.
  Download it from [Releases](https://github.com/flaxodotdev/liemo/releases/latest); it keeps itself
  up to date. Building, credits and the MCP server: [native/README.md](native/README.md);
  releases: [native/RELEASING.md](native/RELEASING.md).
- **The browser version**, the original prototype, is this Next.js app (below).

Liemo is MIT licensed (see [LICENSE](LICENSE)). Third-party code, shaders, fonts and photos keep
their own licences, listed in [native/README.md](native/README.md).

## Browser version

A browser video editor: frame-accurate timeline (`HH:MM:SS:FF` at 30 fps), media pool, music, images, overlays, text styles, fonts and effects.

Next.js 15 (App Router) · TypeScript · Tailwind v4 · shadcn/ui (Radix) · zustand.

```bash
npm install
npm run dev
```

## Design system

The visual language comes from `docs/DESIGN.md` (Framer tokens). It lives in code in two places:

- `src/app/globals.css` holds every token: surfaces (`canvas`, `surface-1..3`), text (`ink`, `ink-muted`), the signal blue, gradient anchors and clip colours. It maps them onto shadcn's variables (`--background`, `--primary`, `--ring`, …) and defines the type scale as utilities: `type-display`, `type-headline`, `type-body`, `type-body-sm`, `type-caption`, `type-micro`, `type-label`, `type-timecode`.
- `src/components/editor/primitives.tsx` has the editor building blocks (`Panel`, `PanelHeader`, `Group`, `SectionLabel`, `IconButton`, `FieldRow`, `SliderField`, `Timecode`, `Kbd`). Panels compose these instead of styling one-off markup.

Rules: dark only. Hierarchy comes from surface lift, not extra greys. Text is either `ink` or `ink-muted`. Blue (`signal`) is for focus, selection and snapping, never a fill. Buttons are pills.

## Layout

```
src/lib/editor/
  assets.ts     sample footage, music, stills, overlays, fonts, text styles, effects
  store.ts      zustand store: clips, tracks, selection, undo/redo, edit actions
  render.ts     canvas compositor, media sync, thumbnails, file import
  playback.ts   transport, rAF loop, Web Audio music synth
  time.ts       FPS, timecode helpers
src/components/editor/
  editor.tsx    layout, keyboard shortcuts, boot
  top-bar.tsx  library-panel.tsx  preview-panel.tsx  inspector-panel.tsx  timeline-panel.tsx
src/components/ui/   shadcn components
```

Imported media stays in the browser tab (object URLs). There is no export yet.
