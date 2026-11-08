#!/usr/bin/env bash
# Build Kraken in release mode and wrap it into target/Kraken.app.
#
#   ./scripts/bundle.sh                 # host-arch release build → target/Kraken.app
#   ./scripts/bundle.sh --universal     # arm64 + x86_64 built separately, merged with lipo
#   ./scripts/bundle.sh --install       # …and copy to /Applications (combinable)
#
# Optional env (all baked into Info.plist):
#   KRAKEN_BUILD_NUMBER         CFBundleVersion (default: the Cargo version). Sparkle compares
#                               this one, so it must increase monotonically between releases.
#   KRAKEN_FEED_URL             SUFeedURL — the Sparkle appcast URL. Omitted when unset.
#   KRAKEN_SPARKLE_PUBLIC_KEY   SUPublicEDKey — base64 EdDSA public key. Omitted when unset.
#
# Re-running is safe: the bundle is rebuilt from scratch each time.
set -euo pipefail
source "$(dirname "$0")/lib.sh"

UNIVERSAL=0
INSTALL=0
for arg in "$@"; do
  case "$arg" in
    --universal) UNIVERSAL=1 ;;
    --install)   INSTALL=1 ;;
    -h|--help)   sed -n '2,15p' scripts/bundle.sh; exit 0 ;;
    *) die "unknown argument: $arg" ;;
  esac
done

if [ "$UNIVERSAL" = 1 ]; then
  TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
  if command -v rustup >/dev/null; then
    installed=$(rustup target list --installed)
    for t in "${TARGETS[@]}"; do
      grep -qx "$t" <<<"$installed" || die "missing Rust target $t — run: rustup target add $t"
    done
  fi
  SLICES=()
  for t in "${TARGETS[@]}"; do
    step "cargo build --release --target $t"
    cargo build --release --bin kraken --target "$t"
    SLICES+=("target/$t/release/kraken")
  done
  mkdir -p target/universal
  BIN=target/universal/kraken
  lipo -create "${SLICES[@]}" -output "$BIN"
  lipo -info "$BIN"
else
  cargo build --release
  BIN=target/release/kraken
fi

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/kraken"

# Icon (regenerated only when missing).
if [ ! -f assets/Kraken.icns ]; then
  ICONSET=$(mktemp -d)/Kraken.iconset; mkdir -p "$ICONSET"
  swift scripts/make_icon.swift assets/icon-1024.png
  for s in 16 32 128 256 512; do
    sips -z $s $s assets/icon-1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    sips -z $((s*2)) $((s*2)) assets/icon-1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
  done
  iconutil -c icns "$ICONSET" -o assets/Kraken.icns
fi
cp assets/Kraken.icns "$APP/Contents/Resources/Kraken.icns"

VERSION=$(kraken_version)
BUILD="${KRAKEN_BUILD_NUMBER:-$VERSION}"
YEAR=$(date +%Y)

# Sparkle keys are only written when configured, so an unconfigured build never
# points an updater at a placeholder URL.
SPARKLE_KEYS=""
if [ -n "${KRAKEN_FEED_URL:-}" ]; then
  SPARKLE_KEYS+="  <key>SUFeedURL</key><string>${KRAKEN_FEED_URL}</string>
  <key>SUEnableAutomaticChecks</key><true/>
"
fi
if [ -n "${KRAKEN_SPARKLE_PUBLIC_KEY:-}" ]; then
  SPARKLE_KEYS+="  <key>SUPublicEDKey</key><string>${KRAKEN_SPARKLE_PUBLIC_KEY}</string>
"
fi

# The NS*UsageDescription strings pair with the resource-access entitlements in
# packaging/Kraken.entitlements: programs run inside the terminal are attributed to
# Kraken by TCC, so macOS shows these strings when e.g. `osascript` or `ffmpeg` asks.
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Kraken</string>
  <key>CFBundleDisplayName</key><string>Kraken</string>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
  <key>CFBundleExecutable</key><string>kraken</string>
  <key>CFBundleIconFile</key><string>Kraken</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${BUILD}</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHumanReadableCopyright</key><string>Copyright © ${YEAR} Kraken contributors. MIT License.</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>NSAppleEventsUsageDescription</key><string>A program running in Kraken wants to control another app.</string>
  <key>NSMicrophoneUsageDescription</key><string>A program running in Kraken wants to use the microphone.</string>
  <key>NSCameraUsageDescription</key><string>A program running in Kraken wants to use the camera.</string>
${SPARKLE_KEYS}</dict>
</plist>
PLIST
plutil -lint -s "$APP/Contents/Info.plist"

# Ad-hoc signature so Gatekeeper lets it run locally. scripts/sign.sh replaces it
# with a hardened-runtime (Developer ID or ad-hoc) signature for distribution.
codesign --force --sign - "$APP" 2>/dev/null || true
echo "Built $APP ($VERSION, build $BUILD)"

if [ "$INSTALL" = 1 ]; then
  rm -rf /Applications/Kraken.app
  cp -R "$APP" /Applications/Kraken.app
  echo "Installed to /Applications/Kraken.app"
fi
