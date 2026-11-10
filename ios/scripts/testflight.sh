#!/bin/bash
# Build Liemo for iPhone/iPad and upload it to TestFlight.
#
#   ASC_KEY_ID=XXXXXXXXXX ASC_ISSUER_ID=xxxxxxxx-xxxx-... ios/scripts/testflight.sh
#
# Needs an App Store Connect API key (App Manager or Admin) saved as
# ~/.appstoreconnect/private_keys/AuthKey_<ASC_KEY_ID>.p8, and the app created in App Store
# Connect with bundle ID dev.flaxo.liemo. Signing certificates and profiles are made
# automatically; the build number goes up by itself on each upload.
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
: "${ASC_KEY_ID:?Set ASC_KEY_ID}" "${ASC_ISSUER_ID:?Set ASC_ISSUER_ID}"
key="$HOME/.appstoreconnect/private_keys/AuthKey_${ASC_KEY_ID}.p8"
[ -f "$key" ] || { echo "Missing $key"; exit 1; }
auth=(-allowProvisioningUpdates -authenticationKeyPath "$key" -authenticationKeyID "$ASC_KEY_ID" -authenticationKeyIssuerID "$ASC_ISSUER_ID")

cd "$here"
[ "${SKIP_CORE:-}" = 1 ] || scripts/build-core.sh
xcodegen generate
out="$here/build/release"
rm -rf "$out"
xcodebuild -project Liemo.xcodeproj -scheme Liemo -configuration Release \
  -destination 'generic/platform=iOS' -archivePath "$out/Liemo.xcarchive" \
  DEVELOPMENT_TEAM=B5UL5U45T7 CODE_SIGNING_ALLOWED=NO CURRENT_PROJECT_VERSION="$(date +%Y%m%d.%H%M)" archive
# The build number is the upload time, so it always goes up (even before Apple has processed the
# previous build).
# Signing happens on export, for App Store distribution (no registered devices needed).
xcodebuild -exportArchive -archivePath "$out/Liemo.xcarchive" \
  -exportOptionsPlist scripts/ExportOptions.plist -exportPath "$out" "${auth[@]}"
echo "Uploaded. It shows up in TestFlight once Apple finishes processing (usually 5–15 minutes)."
