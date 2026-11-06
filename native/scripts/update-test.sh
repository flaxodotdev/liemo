#!/bin/bash
# End-to-end updater test, entirely local: installs this version of Liemo.app in a temporary
# folder, builds the next patch version as a signed release, serves it from a local release
# feed, and lets the installed copy find it, download it, verify it, swap itself and relaunch.
#
#   scripts/update-test.sh <update-key-file>
#
# The test copy runs muted, in the background, with its own data folder.
set -euo pipefail
cd "$(dirname "$0")/.."
key=${1:?usage: scripts/update-test.sh <update-key-file>}

tmp=$(mktemp -d /tmp/liemo-update-test.XXXXXX)
cargo_toml=$(cat Cargo.toml)
restore() { printf '%s\n' "$cargo_toml" > Cargo.toml; }
trap restore EXIT

current=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
next=$(echo "$current" | awk -F. '{print $1"."$2"."$3+1}')
arch=$(uname -m); [ "$arch" = "aarch64" ] && arch=arm64

echo "== installing $current"
scripts/bundle.sh >/dev/null
mkdir -p "$tmp/install" "$tmp/release" "$tmp/data"
ditto dist/Liemo.app "$tmp/install/Liemo.app"

echo "== building $next as a signed release"
sed -i '' "s/^version = \"$current\"/version = \"$next\"/" Cargo.toml
LIEMO_UPDATE_KEY=$(cat "$key") scripts/bundle.sh >/dev/null
zip=Liemo-$next-macos-$arch.zip
cp "dist/$zip" "dist/$zip.sig" "$tmp/release/"
restore

cat > "$tmp/feed.json" <<JSON
{ "tag_name": "v$next", "draft": false, "prerelease": false,
  "html_url": "https://github.com/flaxodotdev/liemo/releases/tag/v$next",
  "body": "Test release.",
  "assets": [
    { "name": "$zip", "browser_download_url": "file://$tmp/release/$zip" },
    { "name": "$zip.sig", "browser_download_url": "file://$tmp/release/$zip.sig" } ] }
JSON

echo "== running $current and letting it update"
exe="$tmp/install/Liemo.app/Contents/MacOS/Liemo"
RAYNA_DATA="$tmp/data" RAYNA_BACKGROUND=1 RAYNA_MUTE=1 LIEMO_UPDATE_FEED="file://$tmp/feed.json" LIEMO_UPDATE_AUTOINSTALL=1 \
  "$exe" >"$tmp/run.log" 2>&1 &
first=$!

ok=
for _ in $(seq 1 120); do
  sleep 1
  v=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$tmp/install/Liemo.app/Contents/Info.plist" 2>/dev/null || true)
  relaunched=$(pgrep -f "$exe" | grep -v "^$first\$" || true)
  if [ "$v" = "$next" ] && ! kill -0 $first 2>/dev/null && [ -n "$relaunched" ]; then
    ok=1; break
  fi
done
pkill -f "$exe" || true
if [ -n "$ok" ]; then
  echo "PASS: $current updated itself to $next and relaunched ($("$exe" --version))"
  [ -d "$tmp/install/.liemo-update-$next" ] && echo "note: staging folder left behind"
  rm -rf "$tmp"
else
  echo "FAIL: installed version '$v'; log in $tmp/run.log"
  exit 1
fi
