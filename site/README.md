# Liemo website

The landing page, a separate Next.js app from the browser editor in the parent folder. It uses the
editor's design tokens and exports static HTML.

```bash
npm install
npm run dev      # http://localhost:3001
npm run build    # writes out/, ready for any static host
```

The download links come from the latest GitHub release at build time (`src/lib/release.ts`), so
rebuild after each release. `public/editor.webp` is a capture of the native app
(`RAYNA_MUTE=1 RAYNA_OPEN=sample RAYNA_TIME=3 RAYNA_SELECT=1`). `/privacy` mirrors `PRIVACY.md`.
