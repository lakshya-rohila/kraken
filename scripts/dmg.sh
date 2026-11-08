#!/usr/bin/env bash
# Package target/Kraken.app into target/Kraken-<version>.dmg (UDZO), with an
# /Applications symlink for drag-to-install. Signs and notarizes the DMG when
# KRAKEN_SIGN_IDENTITY / notary credentials are configured (see sign.sh, notarize.sh).
#
#   ./scripts/dmg.sh [path/to/Kraken.app]
#
# Sign + notarize the .app *before* this step so the copy inside the DMG is stapled.
set -euo pipefail
source "$(dirname "$0")/lib.sh"

SRC="${1:-$APP}"
[ -d "$SRC" ] || die "$SRC not found (run scripts/bundle.sh first)"

VERSION=$(kraken_version)
DMG=$(dmg_path)
VOLNAME="Kraken ${VERSION}"

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
ditto "$SRC" "$STAGE/Kraken.app"
ln -s /Applications "$STAGE/Applications"

# A volume with the same name still mounted (e.g. from a previous run) makes hdiutil fail.
if [ -d "/Volumes/$VOLNAME" ]; then
  note "Detaching stale /Volumes/$VOLNAME"
  hdiutil detach "/Volumes/$VOLNAME" -quiet || hdiutil detach "/Volumes/$VOLNAME" -force -quiet || true
fi

step "Creating $DMG"
rm -f "$DMG"
hdiutil create -volname "$VOLNAME" -srcfolder "$STAGE" -fs HFS+ \
  -format UDZO -imagekey zlib-level=9 -ov "$DMG" >/dev/null
hdiutil verify -quiet "$DMG"

scripts/sign.sh "$DMG"
scripts/notarize.sh "$DMG"

shasum -a 256 "$DMG"
echo "Built $DMG ($(stat -f%z "$DMG") bytes)"
