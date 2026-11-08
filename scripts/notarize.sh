#!/usr/bin/env bash
# Notarize and staple Kraken.app or a .dmg with notarytool.
#
#   ./scripts/notarize.sh                          # target/Kraken.app
#   ./scripts/notarize.sh target/Kraken-X.Y.Z.dmg
#
# Credentials (first match wins):
#   KRAKEN_NOTARY_PROFILE     a keychain profile created once with
#                             `xcrun notarytool store-credentials <profile> ...`
#   APPLE_API_KEY_ID + APPLE_API_ISSUER + APPLE_API_KEY_PATH
#                             App Store Connect API key (Key ID, Issuer UUID, path to AuthKey_*.p8)
# Notarization also needs a real Developer ID signature (KRAKEN_SIGN_IDENTITY).
# With either piece missing the script prints a notice and exits 0.
set -euo pipefail
source "$(dirname "$0")/lib.sh"

TARGET="${1:-$APP}"
[ -e "$TARGET" ] || die "$TARGET not found"

if ! have_identity; then
  note "KRAKEN_SIGN_IDENTITY not set — skipping notarization of $TARGET (ad-hoc builds can't be notarized)."
  exit 0
fi
if ! have_notary_creds; then
  note "No notary credentials (KRAKEN_NOTARY_PROFILE or APPLE_API_KEY_ID/APPLE_API_ISSUER/APPLE_API_KEY_PATH) — skipping notarization of $TARGET."
  exit 0
fi

if [ -n "${KRAKEN_NOTARY_PROFILE:-}" ]; then
  AUTH=(--keychain-profile "$KRAKEN_NOTARY_PROFILE")
  [ -n "${KRAKEN_KEYCHAIN:-}" ] && AUTH+=(--keychain "$KRAKEN_KEYCHAIN")
else
  [ -f "$APPLE_API_KEY_PATH" ] || die "APPLE_API_KEY_PATH=$APPLE_API_KEY_PATH does not exist"
  AUTH=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER")
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

case "$TARGET" in
  *.app)
    # notarytool takes zip/dmg/pkg; ditto keeps the bundle's symlinks and xattrs intact.
    UPLOAD="$WORK/$(basename "$TARGET" .app).zip"
    ditto -c -k --keepParent "$TARGET" "$UPLOAD"
    ;;
  *.dmg|*.pkg|*.zip) UPLOAD="$TARGET" ;;
  *) die "don't know how to notarize $TARGET" ;;
esac

step "Submitting $(basename "$UPLOAD") to Apple notary service (this can take minutes)"
RESULT="$WORK/result.json"
# notarytool exits 0 for a finished-but-Invalid submission, so check the status field.
set +e
xcrun notarytool submit "$UPLOAD" "${AUTH[@]}" --wait --timeout 30m \
  --output-format json > "$RESULT"
rc=$?
set -e
cat "$RESULT"; echo
STATUS=$(plutil -extract status raw -o - "$RESULT" 2>/dev/null || echo unknown)
ID=$(plutil -extract id raw -o - "$RESULT" 2>/dev/null || echo "")

if [ "$rc" -ne 0 ] || [ "$STATUS" != "Accepted" ]; then
  if [ -n "$ID" ]; then
    step "Notary log for $ID"
    xcrun notarytool log "$ID" "${AUTH[@]}" || true
  fi
  die "notarization failed (status: $STATUS, exit: $rc)"
fi

step "Stapling ticket to $TARGET"
xcrun stapler staple "$TARGET"
xcrun stapler validate "$TARGET"

if [[ "$TARGET" == *.app ]]; then
  spctl --assess --type execute -vv "$TARGET"
else
  spctl --assess --type open --context context:primary-signature -vv "$TARGET"
fi
echo "Notarized $TARGET"
