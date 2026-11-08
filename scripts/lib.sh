# Shared helpers for the release scripts. Source it, don't run it:
#   source "$(dirname "$0")/lib.sh"
# Sourcing cd's into the repo root so every path below is repo-relative.

cd "$(dirname "${BASH_SOURCE[0]}")/.."
# shellcheck disable=SC1091
source "$HOME/.cargo/env" 2>/dev/null || true

APP_NAME=Kraken
BUNDLE_ID=dev.kraken.terminal
APP=target/Kraken.app
ENTITLEMENTS=packaging/Kraken.entitlements

# Workspace version (first `version = "..."` in the root Cargo.toml, i.e. [workspace.package]).
kraken_version() {
  grep -m1 '^version' Cargo.toml | cut -d'"' -f2
}

dmg_path() {
  echo "target/${APP_NAME}-$(kraken_version).dmg"
}

# owner/repo on GitHub: $KRAKEN_GITHUB_REPO, then $GITHUB_REPOSITORY (set in Actions),
# then the `origin` remote. Empty if none of those are available.
github_repo() {
  if [ -n "${KRAKEN_GITHUB_REPO:-}" ]; then echo "$KRAKEN_GITHUB_REPO"; return; fi
  if [ -n "${GITHUB_REPOSITORY:-}" ]; then echo "$GITHUB_REPOSITORY"; return; fi
  git remote get-url origin 2>/dev/null \
    | sed -nE 's#^(https://github\.com/|git@github\.com:)([^/]+/[^/]+)$#\2#p' \
    | sed 's/\.git$//' || true
}

note() { printf '\033[1;33m==> NOTICE:\033[0m %s\n' "$*" >&2; }
step() { printf '\033[1;34m==>\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m==> ERROR:\033[0m %s\n' "$*" >&2; exit 1; }

# True when a real Developer ID identity is configured.
have_identity() { [ -n "${KRAKEN_SIGN_IDENTITY:-}" ]; }

# True when notarytool credentials are configured (keychain profile or API key).
have_notary_creds() {
  [ -n "${KRAKEN_NOTARY_PROFILE:-}" ] && return 0
  [ -n "${APPLE_API_KEY_ID:-}" ] && [ -n "${APPLE_API_ISSUER:-}" ] && [ -n "${APPLE_API_KEY_PATH:-}" ]
}
