#!/bin/bash
# Build Liemo's Rust engine (native/mobile) for Android phones (arm64) and the emulator (x86_64),
# and generate its Kotlin bindings, for the Gradle project.
#
#   android/scripts/build-core.sh            release build
#   PROFILE=debug android/scripts/build-core.sh
#
# Needs the NDK (sdkmanager "ndk;27.2.12479018"), cargo-ndk (cargo install cargo-ndk) and the
# Rust targets (rustup target add aarch64-linux-android x86_64-linux-android).
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
native=$(cd "$here/../native" && pwd)
profile=${PROFILE:-release}
flag=$([ "$profile" = release ] && echo --release || true)
sdk=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Library/Android/sdk}}
export ANDROID_NDK_HOME=${ANDROID_NDK_HOME:-$(ls -d "$sdk"/ndk/* | sort -V | tail -1)}

cd "$native"
cargo ndk -t arm64-v8a -t x86_64 -P 29 -o "$here/app/src/main/jniLibs" build -p liemo-mobile $flag --lib

# Kotlin bindings, read from a Mac build of the same crate.
cargo build -p liemo-mobile $flag --lib
out="$here/app/src/main/java"
rm -rf "$out/uniffi"
cargo run -q -p liemo-mobile $flag --bin uniffi-bindgen -- generate \
  --library "target/$profile/libliemo_mobile.dylib" --language kotlin --no-format --out-dir "$out"
echo "$here/app/src/main/jniLibs"
