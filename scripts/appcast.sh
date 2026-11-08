#!/usr/bin/env bash
# Add (or replace) this version's entry in packaging/appcast.xml — the Sparkle 2 feed.
#
#   ./scripts/appcast.sh [path/to/Kraken-X.Y.Z.dmg]
#
# Env:
#   KRAKEN_DOWNLOAD_URL      full URL of the DMG. Default:
#                            https://github.com/<repo>/releases/download/v<version>/<dmg name>
#   KRAKEN_GITHUB_REPO       owner/repo (falls back to $GITHUB_REPOSITORY, then `origin`)
#   KRAKEN_BUILD_NUMBER      sparkle:version (must match CFBundleVersion; default: version)
#   SPARKLE_ED_PRIVATE_KEY   base64 EdDSA private key (from Sparkle's generate_keys -x).
#   SPARKLE_SIGN_UPDATE      path to Sparkle's sign_update tool (default: $SPARKLE_BIN/sign_update,
#                            then sign_update on PATH).
# Without the key or the tool the entry is written without sparkle:edSignature (with a
# notice); Sparkle 2 rejects unsigned updates, so add both before shipping a feed.
set -euo pipefail
source "$(dirname "$0")/lib.sh"

VERSION=$(kraken_version)
BUILD="${KRAKEN_BUILD_NUMBER:-$VERSION}"
DMG="${1:-$(dmg_path)}"
[ -f "$DMG" ] || die "$DMG not found (run scripts/dmg.sh first)"

FEED=packaging/appcast.xml
TEMPLATE=packaging/appcast.template.xml
REPO=$(github_repo)
if [ -z "$REPO" ]; then
  note "GitHub repo unknown (set KRAKEN_GITHUB_REPO=owner/repo) — using OWNER/kraken in URLs."
  REPO=OWNER/kraken
fi
URL="${KRAKEN_DOWNLOAD_URL:-https://github.com/$REPO/releases/download/v$VERSION/$(basename "$DMG")}"
NOTES_URL="https://github.com/$REPO/releases/tag/v$VERSION"
LENGTH=$(stat -f%z "$DMG")
PUBDATE=$(LC_ALL=C date -u '+%a, %d %b %Y %H:%M:%S +0000')
MIN_OS=$(plutil -extract LSMinimumSystemVersion raw -o - "$APP/Contents/Info.plist" 2>/dev/null || echo 13.0)

SIGN_UPDATE="${SPARKLE_SIGN_UPDATE:-}"
if [ -z "$SIGN_UPDATE" ] && [ -n "${SPARKLE_BIN:-}" ] && [ -x "$SPARKLE_BIN/sign_update" ]; then
  SIGN_UPDATE="$SPARKLE_BIN/sign_update"
fi
[ -z "$SIGN_UPDATE" ] && SIGN_UPDATE=$(command -v sign_update || true)

SIG_ATTR=""
if [ -z "${SPARKLE_ED_PRIVATE_KEY:-}" ]; then
  note "SPARKLE_ED_PRIVATE_KEY not set — appcast entry has no sparkle:edSignature."
elif [ -z "$SIGN_UPDATE" ] || [ ! -x "$SIGN_UPDATE" ]; then
  note "Sparkle's sign_update not found (set SPARKLE_SIGN_UPDATE or SPARKLE_BIN) — appcast entry has no sparkle:edSignature."
else
  step "Signing $DMG with sign_update"
  # The key goes through stdin, never argv, so it doesn't show up in `ps`.
  OUT=$(printf '%s' "$SPARKLE_ED_PRIVATE_KEY" | "$SIGN_UPDATE" --ed-key-file - "$DMG")
  SIG=$(sed -nE 's/.*sparkle:edSignature="([^"]+)".*/\1/p' <<<"$OUT")
  [ -n "$SIG" ] || die "could not parse sign_update output"
  SIG_ATTR=" sparkle:edSignature=\"$SIG\""
fi

[ -f "$FEED" ] || sed "s#@HOMEPAGE@#https://github.com/$REPO#" "$TEMPLATE" > "$FEED"
grep -q '<!-- items -->' "$FEED" || die "$FEED has no '<!-- items -->' marker"

ITEM=$(mktemp)
trap 'rm -f "$ITEM"' EXIT
cat > "$ITEM" <<XML
    <!-- item:$VERSION -->
    <item>
      <title>Version $VERSION</title>
      <pubDate>$PUBDATE</pubDate>
      <sparkle:version>$BUILD</sparkle:version>
      <sparkle:shortVersionString>$VERSION</sparkle:shortVersionString>
      <sparkle:minimumSystemVersion>$MIN_OS</sparkle:minimumSystemVersion>
      <sparkle:releaseNotesLink>$NOTES_URL</sparkle:releaseNotesLink>
      <enclosure url="$URL" length="$LENGTH" type="application/octet-stream"$SIG_ATTR/>
    </item>
    <!-- /item:$VERSION -->
XML

# Drop any previous entry for this version, then insert the new one right after the marker.
TMP=$(mktemp)
awk -v v="$VERSION" -v item="$ITEM" '
  index($0, "<!-- item:" v " -->")  { skip = 1; next }
  index($0, "<!-- /item:" v " -->") { skip = 0; next }
  skip { next }
  { print }
  /<!-- items -->/ { while ((getline line < item) > 0) print line; close(item) }
' "$FEED" > "$TMP"
mv "$TMP" "$FEED"

if command -v xmllint >/dev/null; then xmllint --noout "$FEED"; fi
echo "Updated $FEED: $VERSION → $URL ($LENGTH bytes)"
