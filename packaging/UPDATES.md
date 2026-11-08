# Auto-updates: what exists and what's left

## Done (feed side)

- `scripts/appcast.sh` adds an `<item>` per release to `packaging/appcast.xml`
  (version, build number, DMG URL, byte length, minimum macOS, release-notes link) and,
  when `SPARKLE_ED_PRIVATE_KEY` and Sparkle's `sign_update` are available, the
  `sparkle:edSignature`.
- The release workflow attaches `appcast.xml` to every GitHub Release, so
  `https://github.com/<owner>/<repo>/releases/latest/download/appcast.xml` always serves
  the newest feed once a release is published (drafts don't count as "latest").
  Commit the updated `packaging/appcast.xml` back after each release if you want the
  full history in the feed; Sparkle only needs the newest item.
- `scripts/bundle.sh` writes `SUFeedURL` (from `KRAKEN_FEED_URL`) and `SUPublicEDKey`
  (from `KRAKEN_SPARKLE_PUBLIC_KEY`) into Info.plist when set. CI sets `CFBundleVersion`
  to the workflow run number so it rises with every release.

Nothing reads those keys yet: the app has no updater. Pick one of the two options below.

## Option A: Sparkle 2 (standard, full in-place updates)

1. One-time keys: download a Sparkle release, run `./bin/generate_keys` (stores the
   private key in your login keychain and prints the public key), then
   `./bin/generate_keys -x sparkle_private.txt` to export it. Put the private key in the
   `SPARKLE_ED_PRIVATE_KEY` secret, the public key in the `SPARKLE_ED_PUBLIC_KEY` repo
   variable, and the feed URL in `KRAKEN_FEED_URL`. Delete the exported file.
2. Embed `Sparkle.framework` in `Kraken.app/Contents/Frameworks/` (bundle.sh: copy it
   in; the binary needs `-rpath @executable_path/../Frameworks`, e.g. a
   `cargo:rustc-link-arg` in `crates/kraken-app/build.rs`). `scripts/sign.sh` already
   signs anything under `Contents/Frameworks` inside-out before the app. Sparkle's XPC
   services and `Autoupdate`/`Updater.app` helpers must each be signed with the Developer
   ID; check `codesign --verify --deep --strict` after embedding.
3. App glue (Rust, via `objc2`): at launch, create an `SPUStandardUpdaterController`
   (`initWithStartingUpdater:YES updaterDelegate:nil userDriverDelegate:nil`) and keep it
   alive for the app's lifetime; add a "Check for Updates…" menu item that calls
   `checkForUpdates:` on it. That is ~40 lines with `objc2`'s `msg_send!` and
   `objc2-foundation`, linked against the framework (`#[link(name = "Sparkle", kind = "framework")]`).
4. No extra entitlements are needed for a non-sandboxed app.

Cost: a ~3 MB framework, an ObjC dependency, more things to sign. Benefit: signed
delta-free in-place updates, standard UI, relaunch handling.

## Option B: minimal "new version available" check (no framework)

1. On launch (at most once a day, opt-out via config), `GET
   https://api.github.com/repos/<owner>/<repo>/releases/latest` (or fetch the appcast).
2. Compare `tag_name` (strip the `v`) with `CARGO_PKG_VERSION` using semver.
3. If newer, show a non-modal notice (status bar / sheet) with a button that opens the
   release page or DMG URL via `open`. Homebrew users get updates from `brew upgrade`.

Cost: no in-place install; users drag the new app over the old one. Benefit: one small
HTTP call (e.g. `ureq`), no ObjC, nothing extra to sign. The appcast and its EdDSA
signature still work if you move to Sparkle later.
