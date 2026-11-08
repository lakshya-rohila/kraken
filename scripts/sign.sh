#!/usr/bin/env bash
# Code-sign Kraken.app (hardened runtime + packaging/Kraken.entitlements) or a .dmg.
#
#   ./scripts/sign.sh                     # signs target/Kraken.app
#   ./scripts/sign.sh path/to/Kraken.app
#   ./scripts/sign.sh target/Kraken-X.Y.Z.dmg
#
# Env:
#   KRAKEN_SIGN_IDENTITY   e.g. "Developer ID Application: Jane Doe (TEAMID1234)".
#                          Unset → the app is ad-hoc signed (runs locally, cannot be
#                          notarized) and DMG signing is skipped.
#   KRAKEN_KEYCHAIN        optional keychain to search for the identity (CI uses a temp one).
set -euo pipefail
source "$(dirname "$0")/lib.sh"

TARGET="${1:-$APP}"
[ -e "$TARGET" ] || die "$TARGET not found (run scripts/bundle.sh first)"

KEYCHAIN_ARGS=()
[ -n "${KRAKEN_KEYCHAIN:-}" ] && KEYCHAIN_ARGS=(--keychain "$KRAKEN_KEYCHAIN")

case "$TARGET" in
  *.dmg)
    if ! have_identity; then
      note "KRAKEN_SIGN_IDENTITY not set — leaving $TARGET unsigned."
      exit 0
    fi
    step "Signing $TARGET as \"$KRAKEN_SIGN_IDENTITY\""
    codesign --force --timestamp ${KEYCHAIN_ARGS[@]+"${KEYCHAIN_ARGS[@]}"} \
      --sign "$KRAKEN_SIGN_IDENTITY" "$TARGET"
    codesign --verify --strict --verbose=2 "$TARGET"
    exit 0
    ;;
  *.app) ;;
  *) die "don't know how to sign $TARGET (expected .app or .dmg)" ;;
esac

[ -f "$ENTITLEMENTS" ] || die "missing $ENTITLEMENTS"
plutil -lint -s "$ENTITLEMENTS"

if have_identity; then
  IDENTITY="$KRAKEN_SIGN_IDENTITY"
  TIMESTAMP=--timestamp
  step "Signing $TARGET as \"$IDENTITY\" (hardened runtime)"
else
  IDENTITY=-
  TIMESTAMP=--timestamp=none   # ad-hoc signatures can't carry a secure timestamp
  note "KRAKEN_SIGN_IDENTITY not set — ad-hoc signing $TARGET."
  note "The app runs on this Mac but cannot be notarized; Gatekeeper will block it elsewhere."
fi

# Sign nested code inside-out first (none today; this keeps it correct if frameworks
# such as Sparkle.framework or helper tools are added later). --deep is deliberately
# not used for signing: Apple discourages it and it applies one entitlements file to
# everything.
if [ -d "$TARGET/Contents/Frameworks" ]; then
  while IFS= read -r -d '' nested; do
    step "  nested: $nested"
    codesign --force --options runtime $TIMESTAMP ${KEYCHAIN_ARGS[@]+"${KEYCHAIN_ARGS[@]}"} \
      --sign "$IDENTITY" "$nested"
  done < <(find "$TARGET/Contents/Frameworks" -depth \
             \( -name '*.framework' -o -name '*.dylib' -o -name '*.xpc' -o -name '*.app' \) -print0)
fi

codesign --force --options runtime $TIMESTAMP ${KEYCHAIN_ARGS[@]+"${KEYCHAIN_ARGS[@]}"} \
  --entitlements "$ENTITLEMENTS" --identifier "$BUNDLE_ID" \
  --sign "$IDENTITY" "$TARGET"

step "Verifying signature"
codesign --verify --deep --strict --verbose=2 "$TARGET"
codesign --display --verbose=2 "$TARGET" 2>&1 | grep -E '^(Identifier|Format|Authority|TeamIdentifier|Timestamp|Runtime Version|Signature)=' || true

step "Gatekeeper assessment (spctl)"
if spctl --assess --type execute -vv "$TARGET"; then
  :
elif have_identity; then
  # Before notarization a Developer ID app is still rejected ("Unnotarized Developer ID");
  # notarize.sh re-runs spctl after stapling, which is the check that must pass.
  note "spctl rejected $TARGET — expected until it is notarized (scripts/notarize.sh)."
else
  note "spctl rejected $TARGET — expected for an ad-hoc signature."
fi
echo "Signed $TARGET"
