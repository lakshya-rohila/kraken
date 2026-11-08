#!/usr/bin/env bash
# Render the Homebrew cask from packaging/kraken.rb for a built DMG → target/kraken.rb.
#
#   ./scripts/cask.sh [path/to/Kraken-X.Y.Z.dmg]
#
# Env: KRAKEN_GITHUB_REPO=owner/repo (falls back to $GITHUB_REPOSITORY, then `origin`).
set -euo pipefail
source "$(dirname "$0")/lib.sh"

VERSION=$(kraken_version)
DMG="${1:-$(dmg_path)}"
[ -f "$DMG" ] || die "$DMG not found (run scripts/dmg.sh first)"
case "$(basename "$DMG")" in
  "Kraken-$VERSION.dmg") ;;
  *) note "$(basename "$DMG") doesn't match Kraken-$VERSION.dmg, which the cask url expects." ;;
esac

REPO=$(github_repo)
if [ -z "$REPO" ]; then
  note "GitHub repo unknown (set KRAKEN_GITHUB_REPO=owner/repo) — using OWNER/kraken."
  REPO=OWNER/kraken
fi
SHA=$(shasum -a 256 "$DMG" | awk '{print $1}')

OUT=target/kraken.rb
sed -e '1,/^cask /{/^#/d;}' -e "s#@VERSION@#$VERSION#g" -e "s#@SHA256@#$SHA#g" -e "s#@REPO@#$REPO#g" \
  packaging/kraken.rb > "$OUT"
if grep -q '@[A-Z0-9]*@' "$OUT"; then die "unfilled placeholder left in $OUT"; fi
if command -v ruby >/dev/null; then ruby -c "$OUT" >/dev/null; fi
echo "Wrote $OUT (version $VERSION, sha256 $SHA)"
