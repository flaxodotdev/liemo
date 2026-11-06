#!/bin/bash
# Rebuild assets/app/AppIcon.icns from the mark drawn by `liemo app-icon`.
set -euo pipefail
cd "$(dirname "$0")/.."
bin=${1:-target/release/rayna}
tmp=$(mktemp -d)
"$bin" app-icon "$tmp/icon-1024.png"
set=$tmp/AppIcon.iconset
mkdir "$set"
for s in 16 32 128 256 512; do
  sips -z $s $s "$tmp/icon-1024.png" --out "$set/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z $d $d "$tmp/icon-1024.png" --out "$set/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$set" -o assets/app/AppIcon.icns
cp "$tmp/icon-1024.png" assets/app/AppIcon.png
rm -rf "$tmp"
echo "assets/app/AppIcon.icns"
