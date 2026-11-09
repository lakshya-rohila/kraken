#!/usr/bin/env bash
# Kraken one-line installer for macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/lakshya-rohila/kraken/main/install.sh | bash
#
# What it does:
#   1. Installs the newest prebuilt Kraken.app from GitHub Releases, if one exists.
#   2. Otherwise builds it from source: installs Rust (rustup) if missing, clones the
#      repo into a temporary folder, runs scripts/bundle.sh --install, and cleans up.
#
# Options (environment variables):
#   KRAKEN_FROM_SOURCE=1   always build from source, even if a release exists
#   KRAKEN_REF=<branch>    branch or tag to build from (default: main)
#   KRAKEN_PREFIX=<dir>    where Kraken.app goes (default: /Applications)
set -euo pipefail

REPO="lakshya-rohila/kraken"
REF="${KRAKEN_REF:-main}"
PREFIX="${KRAKEN_PREFIX:-/Applications}"
APP="$PREFIX/Kraken.app"

bold=$'\033[1m'; dim=$'\033[2m'; red=$'\033[31m'; cyan=$'\033[36m'; amber=$'\033[38;5;208m'; reset=$'\033[0m'
[ -t 1 ] || { bold=; dim=; red=; cyan=; amber=; reset=; }
say()  { printf '%s\n' "${cyan}▌${reset} $*"; }
die()  { printf '%s\n' "${red}✖${reset} $*" >&2; exit 1; }

printf '\n%s\n' "${amber}${bold}  ◉ KRAKEN${reset}${dim}  — the GPU terminal for the coding-agent era${reset}"
printf '\n'

[ "$(uname -s)" = Darwin ] || die "Kraken is a macOS app (macOS 13 Ventura or newer)."
major=$(sw_vers -productVersion | cut -d. -f1)
[ "$major" -ge 13 ] || die "Kraken needs macOS 13 or newer (you have $(sw_vers -productVersion))."

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# Copies an .app into $PREFIX, using sudo only if the folder isn't writable.
place_app() {
  local src="$1" sudo=""
  [ -w "$PREFIX" ] || sudo="sudo"
  if pgrep -xq kraken; then say "Kraken is running — it keeps running the old build until you restart it."; fi
  $sudo rm -rf "$APP"
  $sudo ditto "$src" "$APP"
  $sudo xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true
}

install_release() {
  local api asset
  api=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null) || return 1
  asset=$(printf '%s' "$api" | grep -o '"browser_download_url": *"[^"]*\.\(zip\|dmg\)"' | head -1 | sed 's/.*"\(http[^"]*\)"/\1/')
  [ -n "$asset" ] || return 1
  say "Downloading ${bold}$(basename "$asset")${reset}"
  curl -fL --progress-bar "$asset" -o "$TMP/pkg" || return 1
  case "$asset" in
    *.zip)
      ditto -x -k "$TMP/pkg" "$TMP/x" ;;
    *.dmg)
      local mnt="$TMP/mnt"; mkdir -p "$mnt"
      hdiutil attach -nobrowse -quiet -mountpoint "$mnt" "$TMP/pkg"
      mkdir -p "$TMP/x"; ditto "$mnt/Kraken.app" "$TMP/x/Kraken.app"
      hdiutil detach -quiet "$mnt" ;;
  esac
  local found; found=$(find "$TMP/x" -maxdepth 3 -name Kraken.app -type d | head -1)
  [ -n "$found" ] || return 1
  place_app "$found"
}

install_source() {
  xcode-select -p >/dev/null 2>&1 || {
    say "Installing Xcode Command Line Tools (a macOS dialog will open)…"
    xcode-select --install || true
    die "Finish the Command Line Tools install, then run this command again."
  }
  if ! command -v cargo >/dev/null 2>&1; then
    [ -x "$HOME/.cargo/bin/cargo" ] || {
      say "Installing Rust (rustup)…"
      curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null
    }
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
  command -v rustup >/dev/null 2>&1 && rustup update stable --no-self-update >/dev/null 2>&1 || true

  say "Cloning ${bold}$REPO${reset} ($REF)"
  git clone --depth 1 --branch "$REF" --quiet "https://github.com/$REPO.git" "$TMP/kraken"
  say "Building a release bundle — the first build takes a few minutes ☕"
  (cd "$TMP/kraken" && ./scripts/bundle.sh)
  place_app "$TMP/kraken/target/Kraken.app"
}

if [ "${KRAKEN_FROM_SOURCE:-0}" != 1 ] && install_release; then
  :
else
  [ "${KRAKEN_FROM_SOURCE:-0}" = 1 ] || say "No prebuilt release yet — building from source."
  install_source
fi

printf '\n%s\n' "${amber}${bold}  ◉ Kraken is installed${reset} → $APP"
printf '%s\n\n' "${dim}  Open it from Spotlight (⌘ Space → Kraken) or run:${reset} open -a Kraken"
