# Liemo for Android

A native Kotlin + Jetpack Compose app on the same Rust engine as the desktop and iPhone apps
(`native/mobile`, through UniFFI's Kotlin bindings). Every edit goes through the engine's JSON
tools, the same ones the MCP server and the iPhone app use; Android supplies video frames, photo
and sound decoding (`AndroidMediaHost`), playback (`AudioTrack`) and export (`MediaCodec`).

## Build

Needs JDK 17, the Android SDK (platform 36, build-tools 36) with the NDK, and:

```sh
rustup target add aarch64-linux-android x86_64-linux-android
cargo install cargo-ndk
```

Then:

```sh
android/scripts/build-core.sh          # engine .so files + Kotlin bindings
cd android && ./gradlew installDebug   # onto a connected phone or emulator
```

`build-core.sh` puts the engine in `app/src/main/jniLibs` (not committed) and regenerates
`app/src/main/java/uniffi/` (committed, like the iOS bindings). Run it again after changing the
engine's API in `native/mobile/src/api.rs`.

The version comes from `native/mobile/Cargo.toml`, so it follows every Liemo release.

## Not on Android yet

Effects, filters and transitions pickers, captions, voiceover, Liemo AI, quick reels, the song
library and subject cut-outs: the engine does all of them already; the screens are still to come.
