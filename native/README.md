# Liemo (native)

The Liemo video editor as a native macOS app, written in Rust with [GPUI](https://www.gpui.rs).
It is a port of the Next.js app in the parent folder: same timeline, library, preview, inspector,
effects, transitions and export, same design tokens, and the same project and clip JSON (a clip
copied in one pastes into the other).

```bash
cargo run --release
```

Needs FFmpeg's libraries and `pkg-config` (`brew install ffmpeg pkgconf`) and Xcode.

## Build notes

- `.cargo/config.toml` points `SDKROOT` at Xcode's SDK. On this machine the Command Line Tools
  SDK (macOS 27) can't be read by its own linker, which breaks `ffmpeg-sys-next`'s build script.
- `vendor/gpui-pre-macos` is gpui-pre-macos 0.3.11 with three `Option::map_or_default` calls
  spelled out, so the crate builds on Rust 1.96 (that API is stable from 1.97). Drop the
  `[patch.crates-io]` entry once the toolchain is newer.
- GPUI is built with `runtime_shaders`, so the Metal toolchain isn't required.

## Layout

```
src/model/     time, clip/track types, asset catalogue (procedural scenes), editor store
src/render/    Canvas 2D work-alike over tiny-skia, text (rustybuzz), compositor, thumbnails
src/fx/        effect registry and engines, transitions, GPU shader runtime (remocn, Paper)
src/media/     FFmpeg: probing, frame-accurate video decoding, audio decoding, import
src/audio/     timeline mixer + music synth (shared by playback and export), cpal output
src/export.rs  MP4/WebM (x264 or VideoToolbox, VP9/VP8, AAC/Opus), GIF, PNG/JPEG, WAV/M4A
src/storage.rs projects as JSON in ~/Library/Application Support/Rayna
src/ui/        GPUI views: home, editor (top bar, library, preview, inspector, timeline, export)
```

Effects include 100 Instagram-style filters (`src/fx/looks.rs`): the feed filters follow
[CSSgram](https://github.com/una/CSSgram)'s recreations (MIT), plus the Stories city filters and
film and mood looks. Effect and transition thumbnails use photographs from Unsplash (Unsplash
licence), bundled in `assets/photos`. The sample footage is nature photography from Unsplash
(Unsplash licence) in `assets/footage`, played as slow camera moves.

Shader filters, shader transitions and the animated shader backgrounds come from
[remocn](https://github.com/Remocn/remocn-studio) (MIT). Its WebGL fragment shaders are in
`assets/remocn` and run on the GPU through `src/fx/gpu.rs`, which rewrites WebGL 2 / WebGL 1
GLSL for wgpu (Metal). `src/fx/remocn.rs` and `src/fx/remocn_tr.rs` carry remocn's defaults,
uniform setup and CSS transition timings. The backgrounds are built on
[Paper Shaders](https://shaders.paper.design) (Apache-2.0; `assets/paper` holds the shaders,
remocn's presets as `paper.json`, and Paper's LICENSE and NOTICE), driven by `src/fx/paper.rs`.
The icon-scatter transition draws [Lucide](https://lucide.dev) outlines (ISC). The bundled fonts
(Inter, Mona Sans, Instrument Serif, Bebas Neue, JetBrains Mono, Caveat) are under the SIL Open
Font License 1.1; their licences are in `assets/fonts/licenses`. FFmpeg (LGPL/GPL, as built by
Homebrew) ships inside release builds as separate libraries.

Differences from the browser version, by design:

- Imported media is used from where it is on disk, not copied into app storage.
- Exports are written to `~/Downloads` and the dialog offers "Show in Finder".

## Updates and releases

Liemo updates itself from GitHub Releases (`src/update.rs`). Tag `vX.Y.Z` to build, sign,
notarise and publish a release; see [RELEASING.md](RELEASING.md).

## AI editing (MCP)

`rayna mcp` is a [Model Context Protocol](https://modelcontextprotocol.io) server on stdio, so
any MCP client can edit videos. The projects page has a **Connect AI** button with ready-to-copy
setup; by hand:

```bash
claude mcp add rayna -- "$(pwd)/target/release/rayna" mcp      # Claude Code
```

```json
{ "mcpServers": { "rayna": { "command": "/path/to/rayna", "args": ["mcp"] } } }
```

When Liemo is open it listens on `mcp.sock` in its data folder and the server hands tool calls
to it: edits land in the open editor live (with undo and the usual notes), rendering and export
run off the UI thread. Without the app, tools edit the project files directly.

Tools: `list_projects`, `create_project` (any format, custom size, fps, background, sample
template), `get_project`, `open_project`, `set_project`, `list_library`, `list_effects`,
`list_transitions`, `list_formats`, `import_media`, `add_clip`, `add_text`, `update_clip`,
`remove_clip`, `split_clip`, `duplicate_clip`, `apply_effect`, `update_effect`, `remove_effect`,
`set_transition`, `set_track`, `undo`, `redo`, `render_frame` (returns a PNG) and `export`.
Code: `src/mcp/` (protocol and socket) and `src/mcp/tools.rs` (definitions and executor).

## Checks without the UI

```bash
cargo run --release -- --render-test <dir>          # sample frames, effect and transition thumbnails as PNG
cargo run --release -- --media-test <file> <dir>    # import, decode, and every export format
```

Launch hooks for screenshots and testing: `RAYNA_MUTE=1` (no audio output), `RAYNA_OPEN=sample`,
`RAYNA_TIME=<s>`, `RAYNA_SELECT=<n>`, `RAYNA_TAB=effects|transitions|text|music`, `RAYNA_EXPORT=1`.
