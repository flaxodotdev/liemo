#!/bin/bash
# Build Liemo.app and its release zip.
#
#   scripts/bundle.sh            build, bundle, ad-hoc sign, zip      → dist/
#
# Optional environment (CI sets these from repository secrets):
#   SIGN_IDENTITY        "Developer ID Application: … (TEAMID)" — sign for distribution
#   APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD
#                        notarise with notarytool and staple the ticket
#   LIEMO_UPDATE_KEY     ed25519 update key (base64) — writes the .zip.sig the updater checks
#   LIEMO_VERSION        override the version (defaults to Cargo.toml's)
#   SKIP_BUILD=1         reuse target/release/rayna
set -euo pipefail
cd "$(dirname "$0")/.."

version=${LIEMO_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}
arch=$(uname -m)
[ "$arch" = "aarch64" ] && arch=arm64
bundle_id=dev.flaxo.liemo
dist=dist
app=$dist/Liemo.app
zip=$dist/Liemo-$version-macos-$arch.zip

[ -n "${SKIP_BUILD:-}" ] || cargo build --release
rm -rf "$app" "$zip" "$zip.sig" "$dist"/*.dmg
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Frameworks"
cp target/release/rayna "$app/Contents/MacOS/Liemo"
cp assets/app/AppIcon.icns "$app/Contents/Resources/AppIcon.icns"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Liemo</string>
  <key>CFBundleDisplayName</key><string>Liemo</string>
  <key>CFBundleIdentifier</key><string>$bundle_id</string>
  <key>CFBundleExecutable</key><string>Liemo</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>$(sw_vers -productVersion | cut -d. -f1).0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.video</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>Liemo records voice-overs and sounds from your microphone into your project.</string>
  <key>NSHumanReadableCopyright</key><string>Liemo is open source.</string>
</dict>
</plist>
PLIST

# ---- Bundle non-system libraries (FFmpeg from Homebrew and what it pulls in) ----
fw=$app/Contents/Frameworks
resolve() { # resolve a load command entry referenced by $2
  local dep=$1 from=$2
  case "$dep" in
    /System/*|/usr/lib/*) return 1 ;;
    @rpath/*|@loader_path/*|@executable_path/*)
      local name=${dep##*/}
      for dir in "$(dirname "$from")" /opt/homebrew/lib /usr/local/lib; do
        [ -f "$dir/$name" ] && { echo "$dir/$name"; return 0; }
      done
      # A library already copied in.
      [ -f "$fw/$name" ] && { echo "$fw/$name"; return 0; }
      echo "can't resolve $dep (from $from)" >&2; exit 1 ;;
    *) echo "$dep" ;;
  esac
}
deps_of() { otool -L "$1" | tail -n +2 | awk '{print $1}'; }

queue=("$app/Contents/MacOS/Liemo")
seen=" "
while [ ${#queue[@]} -gt 0 ]; do
  file=${queue[0]}; queue=("${queue[@]:1}")
  for dep in $(deps_of "$file"); do
    src=$(resolve "$dep" "$file") || continue
    name=${src##*/}
    [ "$(basename "$file")" = "$name" ] && continue # its own id
    if [[ "$seen" != *" $name "* ]]; then
      seen="$seen$name "
      if [ ! -f "$fw/$name" ]; then
        cp -L "$src" "$fw/$name"
        chmod u+w "$fw/$name"
        install_name_tool -id "@rpath/$name" "$fw/$name" 2>/dev/null
        queue+=("$fw/$name")
      fi
    fi
    install_name_tool -change "$dep" "@rpath/$name" "$file" 2>/dev/null
  done
done
install_name_tool -add_rpath "@executable_path/../Frameworks" "$app/Contents/MacOS/Liemo" 2>/dev/null || true
for lib in "$fw"/*.dylib; do
  install_name_tool -add_rpath "@loader_path" "$lib" 2>/dev/null || true
done
echo "bundled $(ls "$fw" | wc -l | tr -d ' ') libraries"

# ---- Sign ----
identity=${SIGN_IDENTITY:--}
opts=(--force --timestamp --options runtime)
[ "$identity" = "-" ] && opts=(--force)
for lib in "$fw"/*.dylib; do
  codesign "${opts[@]}" --sign "$identity" "$lib"
done
codesign "${opts[@]}" --entitlements scripts/entitlements.plist --sign "$identity" "$app"
codesign --verify --deep --strict "$app"
# A last check that every library resolves inside the bundle.
"$app/Contents/MacOS/Liemo" --version >/dev/null

# ---- Notarise ----
if [ -n "${APPLE_ID:-}" ] && [ "$identity" != "-" ]; then
  ditto -c -k --keepParent "$app" "$dist/notarize.zip"
  xcrun notarytool submit "$dist/notarize.zip" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait
  xcrun stapler staple "$app"
  rm "$dist/notarize.zip"
fi

# ---- Disk image for downloads (the updater uses the zip) ----
scripts/dmg.sh "$app" "$dist/Liemo-$version-macos-$arch.dmg"

# ---- Release zip and update signature ----
ditto -c -k --sequesterRsrc --keepParent "$app" "$zip"
if [ -n "${LIEMO_UPDATE_KEY:-}" ]; then
  "$app/Contents/MacOS/Liemo" sign-update - "$zip"
fi
ls -la "$dist"
