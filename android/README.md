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

## GitHub releases and updates

The `Release` GitHub Actions workflow builds a universal arm64 + x86_64 APK for each `v*` tag
and attaches it as `Liemo-<version>-android-universal.apk`. The website links to this latest
asset. Installed Android copies check the latest stable GitHub release at startup and offer to
download/install a newer APK through Android's package installer.

Android requires every update to use the same signing key as the installed app. Before tagging
an Android release, configure these repository Actions secrets from a persistent release
keystore; never regenerate or lose this keystore. The key created for Liemo is backed up at
`~/.config/liemo/android-release.keystore`; its password is stored in this Mac's Keychain under
`liemo-android-release-keystore-password`:

| Secret | Value |
|---|---|
| `ANDROID_KEYSTORE_BASE64` | Base64-encoded JKS/PKCS12 keystore file |
| `ANDROID_KEYSTORE_PASSWORD` | Keystore password |
| `ANDROID_KEY_ALIAS` | Signing key alias |
| `ANDROID_KEY_PASSWORD` | Signing key password |

The APK installs from GitHub only when the user allows Liemo to install unknown apps. The OS
installer verifies its signature and asks the user to confirm the update.

## Not on Android yet

Effects, filters and transitions pickers, captions, voiceover, Liemo AI, quick reels, the song
library and subject cut-outs: the engine does all of them already; the screens are still to come.
