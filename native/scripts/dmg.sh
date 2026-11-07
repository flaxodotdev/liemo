#!/bin/bash
# Make the drag-to-install disk image for a built Liemo.app.
#
#   scripts/dmg.sh <Liemo.app> <out.dmg>
#
# With SIGN_IDENTITY set, the image is signed; with APPLE_ID, APPLE_TEAM_ID and
# APPLE_APP_PASSWORD too, it's notarised and the ticket stapled, so it opens without warnings.
set -euo pipefail
app=${1:?usage: scripts/dmg.sh <Liemo.app> <out.dmg>}
out=${2:?usage: scripts/dmg.sh <Liemo.app> <out.dmg>}
cd "$(dirname "$0")/.."

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
ditto "$app" "$stage/Liemo.app"
ln -s /Applications "$stage/Applications"
rm -f "$out"
hdiutil create -quiet -volname "Liemo" -srcfolder "$stage" -fs HFS+ -format UDZO -imagekey zlib-level=9 "$out"

if [ -n "${SIGN_IDENTITY:-}" ] && [ "${SIGN_IDENTITY}" != "-" ]; then
  codesign --force --timestamp --sign "$SIGN_IDENTITY" "$out"
  if [ -n "${APPLE_ID:-}" ]; then
    xcrun notarytool submit "$out" --apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD" --wait
    xcrun stapler staple "$out"
  fi
fi
echo "$out"
