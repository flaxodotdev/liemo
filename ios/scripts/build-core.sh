#!/bin/bash
# Build Liemo's Rust engine (native/mobile) for iPhone and the simulator, generate its Swift
# bindings, and package both into ios/LiemoCore.xcframework for the Xcode project.
#
#   ios/scripts/build-core.sh            release build
#   PROFILE=debug ios/scripts/build-core.sh
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
native=$(cd "$here/../native" && pwd)
profile=${PROFILE:-release}
flag=$([ "$profile" = release ] && echo --release || true)

cd "$native"
for target in aarch64-apple-ios aarch64-apple-ios-sim; do
  sdk=$([ "$target" = aarch64-apple-ios ] && echo iphoneos || echo iphonesimulator)
  echo "== $target"
  SDKROOT=$(xcrun --sdk "$sdk" --show-sdk-path) IPHONEOS_DEPLOYMENT_TARGET=17.0 \
    cargo build -p liemo-mobile $flag --lib --target "$target"
done

# Swift bindings, read from a Mac build of the same crate.
cargo build -p liemo-mobile $flag --lib
out="$here/Generated"
rm -rf "$out" && mkdir -p "$out"
cargo run -q -p liemo-mobile $flag --bin uniffi-bindgen -- generate \
  --library "target/$profile/libliemo_mobile.dylib" --language swift --out-dir "$out"

# Headers for the C side, with the module map Xcode expects.
headers=$(mktemp -d)
cp "$out/liemo_mobileFFI.h" "$headers/"
cp "$out/liemo_mobileFFI.modulemap" "$headers/module.modulemap"

rm -rf "$here/LiemoCore.xcframework"
xcodebuild -create-xcframework \
  -library "target/aarch64-apple-ios/$profile/libliemo_mobile.a" -headers "$headers" \
  -library "target/aarch64-apple-ios-sim/$profile/libliemo_mobile.a" -headers "$headers" \
  -output "$here/LiemoCore.xcframework"
rm -rf "$headers"
echo "$here/LiemoCore.xcframework"
