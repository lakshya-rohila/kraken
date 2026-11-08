# Kraken

A GPU-accelerated terminal for macOS, written in Rust. See [PLAN.md](PLAN.md) for the roadmap.

## Build & run

```bash
cargo run                      # debug build
./scripts/bundle.sh            # release build → target/Kraken.app
./scripts/bundle.sh --install  # …and copy to /Applications
```

## Shortcuts

### Tabs, splits, windows
| Keys | Action |
|---|---|
| ⌘T | New tab (opens in the current folder) |
| ⌘⇧R or double-click a tab | Rename the tab (↩ save, Esc cancel, empty = automatic name) |
| ⌘⇧A | Agents dashboard |
| ⌘⇧\\ | Tab overview: every tab as a live thumbnail (←↑↓→ choose, ↩ open, 1–9 jump) |
| ⌘D / ⌘⇧D | Split right / split down |
| ⌘⌥ ←↑↓→ | Move focus between panes |
| ⌘⌥⇧ ←↑↓→ | Swap the pane with its neighbour |
| ⌘⌃ ←↑↓→ | Move the nearest divider |
| ⌘⇧↩ | Zoom the focused pane (toggle) |
| ⌘1…⌘8, ⌘9 | Go to tab N / last tab |
| ⌘⇧[ ⌘⇧] or ⌃Tab ⌃⇧Tab | Previous / next tab |
| ⌘W | Close pane (then tab, then window) |
| ⌘N / ⌘Q | New window / quit |

Mouse: click a pane to focus it, drag a divider to resize, click a tab to switch,
drag a tab sideways to reorder, `×` closes the active tab, `+` opens a new one.
A green ● on a tab means new output in the background; orange ● means a bell.

### Editing & view
| Keys | Action |
|---|---|
| ⌘C / ⌘V | Copy selection / paste (bracketed paste aware) |
| ⌘A | Select all |
| ⌘K | Clear screen and scrollback |
| ⌘= / ⌘- / ⌘0 | Bigger / smaller / reset font |
| ⌘← / ⌘→ / ⌘⌫ | Line start / line end / delete to line start |
| ⌘↑ / ⌘↓ | Previous / next prompt |
| ⌘⇧↑ / ⌘⇧↓ | Top / bottom of history |
| ⌘F | Find (↩ older, ⇧↩ newer, ⌘R regex, Esc close) |
| ⌘⇧P | Command palette |
| ⌘, | Open settings |
| ⌘P | Find a file |
| ⌘B | File sidebar |
| ⌘-click | Open URL / file:line |
| ⌥← / ⌥→ / ⌥⌫ | Word left / word right / delete word |
| ⇧↩ | Newline without submitting (coding agents) |
| ⇧PgUp / ⇧PgDn | Scroll scrollback |
| ⌘/ | Shortcut sheet |
| ⌘⇧K | Summon the Kraken (welcome animation) |
| Double / triple click | Select word / line (⌥-drag = block select) |

## Menu bar, tab colours, agents

- **Menu bar.** Kraken, Shell, Edit, View, Window and Help menus list every command with its shortcut — the classic Mac way to discover things. Themes, opacity, animations and tab colours are submenus.
- **Tab colours.** Shell ▸ Tab Color (or "Tab Color" in ⌘⇧P): red, orange, yellow, green, blue, purple, cyan. Tabs connected over SSH turn orange on their own. Colours are saved with your session.
- **Agents dashboard (⌘⇧A).** Every coding agent running in this window — Claude Code, Codex, Gemini CLI, OpenCode, Aider, Goose… — with its state: ● working (it's printing) or ◆ waiting / needs you (it went quiet). Waiting agents sort to the top; ↩ jumps straight to that pane. The status bar always shows the count, and when an agent in a background tab finishes and waits for you, macOS notifies you and its tab gets an orange dot.
- **"Needs you" cards.** When an agent in another tab goes quiet, rings the bell or asks permission, a card slides in from the right with its actual question (e.g. *Do you want to make this edit to main.rs?*). Numbered choices (Claude Code's **1 Yes / 2 Always / 3 No**) and `(y/n)` prompts become buttons — answer without leaving your tab. Click the card to jump to the agent; × dismisses it; it slides away by itself once the agent is working again or you go look. If you're in another app, macOS notifies you with the question too.
- **Clean resizing.** Splitting a pane or opening the sidebar no longer leaves a duplicated prompt line: Kraken knows exactly which lines are the prompt and keeps them from re-wrapping.

## Images, keyboard, fonts

- **Inline images.** All three common protocols work: Kitty graphics (`kitty +kitten icat photo.png`, `timg`, `yazi` previews…), iTerm2's `imgcat` (OSC 1337), and sixel (`img2sixel`, `lsix`, matplotlib's sixel backend…). Images scroll with the text, stay in history, and shrink to fit the pane.
- **Kitty keyboard protocol.** Programs that ask for it (Claude Code, neovim, helix, fish…) get unambiguous keys: Shift+Enter, Ctrl+I vs Tab, Esc vs Alt, and key releases.
- **Ligatures.** `->` `=>` `!=` `>=` `===` become single symbols with JetBrains Mono (on by default; `ligatures = false` under `[font]` turns them off).
- **Input methods.** Chinese, Japanese, Korean, Hindi… input shows what you're composing at the cursor, and the candidate window opens right beside it.

## Files & code

- **⌘-click links.** Hold ⌘ and links underline: URLs open in your browser; file paths open in your editor at the exact spot — `src/main.rs:42:7` from a compiler error lands on line 42, column 7. Also works for `grep -n` output, stack traces, `git diff` paths (`a/src/x.rs`) and OSC 8 hyperlinks. Folders open in the sidebar (or Finder).
- **⌘B – File sidebar.** The focused pane's project as a tree (folders first, Nerd Font icons). Files are coloured by git status: yellow = modified, green = new, red = deleted; folders show if anything inside changed. It follows the pane as you `cd`. Keys: ↑↓ move, →/← open/close folder, ↩ open in editor, ⌥↩ edit in nvim in a split, ⇧↩ paste the path into the terminal, Space = Quick Look preview, ⌫ = parent folder, Esc = back to the terminal. Click the header to go up a folder.
- **⌘P – Find a file.** Fuzzy search every file in the project (skips `node_modules`, `target`, `.git`…). ↩ opens it, ⌥↩ edits in nvim, ⇧↩ pastes its path.
- **Drag files from Finder** into a pane to paste their paths (spaces quoted for you).
- **Editor:** `editor = "auto"` uses the first of Cursor, VS Code, Zed, Sublime that's installed (Cursor on this Mac). `editor = "terminal"` edits in `$EDITOR` (nvim) in a new split that closes when you quit the editor.

## Arrange, see through, come back

- **Move panes by dragging.** Hover a pane (when there's more than one) and grab the small handle at its top, or hold **⌘⌥** and drag anywhere in it. Drop near another pane's edge to split there, in its middle to swap, on a tab to move it into that tab, or on **+** to give it its own tab. Keyboard: **⌘⌥⇧ + arrows** swaps with the neighbouring pane.
- **See-through windows.** `opacity = 0.85` and `blur = true` in settings (or "Window Opacity" / "Background Blur" in the palette). The title, tab and status bars stay a little more solid so they're easy to read.
- **Session restore.** Kraken reopens your windows, tabs, splits (with their sizes) and each pane's folder. It saves every few seconds and when you quit with ⌘Q. Closing the last window yourself starts fresh next time. Turn off with `restore_session = false`.

## Find, commands, settings

- **⌘F – Find.** Search the pane's screen and whole history. Matches light up amber; the current one is orange. ↩ goes to older matches, ⇧↩ newer, ⌘R switches between plain text and regex, Esc closes. Lowercase searches ignore case; add a capital to match case.
- **⌘⇧P – Command palette.** Type a few letters of any command (`sr` → Split Right). Browsing themes previews them live; ↩ keeps it.
- **⌘, – Settings.** Opens `~/.config/kraken/kraken.toml` (created with every option explained). Save it and Kraken applies changes immediately; a typo shows a red message with the line number and keeps your previous settings.
- **⌘↑ / ⌘↓ – Jump between prompts** (with shell integration). ⌘⇧↑ / ⌘⇧↓ go to the very top / bottom.
- **Themes:** `kraken-abyss`, `deep-sea`, `neon-reef`, `ember`, `midnight-ink`, `high-contrast` (every text colour ≥ 7:1, WCAG AAA). Override single colours under `[colors]`.

## Look & feel

- **Welcome:** the first window opens with an animated ASCII-art Kraken: it draws itself line by line, its tentacles sway, a shimmer runs across the name, then it fades to your prompt. Any key or click skips it. **⌘⇧K** summons it again.
- **⌘/** opens the shortcut sheet: every key and what it does, as colour-coded keycaps. Any key or click closes it.
- **Animations:** new tabs pop open, the active-tab underline glides between tabs, new split panes grow out from the divider, and new tabs fade in.
- Idle windows use no CPU: Kraken only animates while something is moving.

## Copy, paste, images

- **⌘V pastes images.** A screenshot (⌃⇧⌘4), or an image copied from a browser,
  Preview or Figma, is saved as a PNG and its path is pasted, so agents attach it:
  Claude Code shows `[Image #1]`, Codex and Gemini read the file. Pasted images live
  in `~/Library/Caches/Kraken/pastes` and are cleared after a week.
- **⌘V pastes copied files as paths.** Copy files in Finder, ⌘V in Kraken: full,
  quoted paths (Finder only puts the *name* on the clipboard as text).
- **Ctrl+V** is left to the program: Claude Code reads clipboard images itself on Ctrl+V.
- **Drag and drop** files or images onto a pane: pasted as paths, as a real paste.
- **Copying.** ⌘C copies the selection and says how much ("Copied 42 characters");
  `copy_on_select = true` copies as you select. Programs can copy to your clipboard
  with OSC 52, also over ssh.
- Pastes are bracketed when the program asks, so multi-line text never runs early.

## Focus, overview, clickable status bar

- **Focus.** In a split, the pane you're typing in has a soft accent glow and the
  others sit back; switching crossfades. `dim_inactive = 0.38` (0 = off, up to 0.85)
  and `focus_glow = true` in settings.
- **Tab overview (⌘⇧\\).** Every tab as a live miniature of its real split layout,
  with each agent's state. Click a card or use the arrows and ↩; 1–9 jump; Esc closes.
- **Clickable status bar.** Click the folder to open it in Finder, the git branch to
  see every changed file (click one to open it), or the agent count to open the
  agents dashboard.
- **Light on resources.** A covered or minimized window draws nothing until you can
  see it again; the status bar redraws only when something on it changes; git is
  asked only while the window is in front.

## Status bar

The bar at the bottom describes the focused pane:

- **Left:** what's running (an orange **SSH user@host** pill when you're on a server, an accent pill for a running program like `claude` or `vim`), the current folder, and the git branch with changed-file count (●), ahead (↑) and behind (↓).
- **Right:** a live timer while a command runs, then ✓ or ✗ exit code with how long it took, and the clock.
- Panes connected over ssh/mosh get an orange outline and a `⇄ host` title.
- When a command that ran 10 s or more finishes while you're looking elsewhere, macOS shows a notification. Programs (like coding agents) can also send notifications with OSC 9 / OSC 777.

Exit codes and timing come from Kraken's shell integration, which loads automatically for zsh, bash, fish, nushell and PowerShell (`pwsh`). Your own startup files (`.zshrc`, `.bashrc`/`.bash_profile`, `config.fish`, `config.nu`, `$PROFILE`) are untouched and load as usual. Turn it off with `shell_integration = false`.

## Config

Press **⌘,** to open `~/.config/kraken/kraken.toml`. It's created on first use with every option explained, and changes apply the moment you save. For example:

```toml
theme = "neon-reef"        # kraken-abyss, deep-sea, neon-reef, ember, midnight-ink, high-contrast
animation_speed = 1.0      # 2 = faster, 0.5 = slower, 0 = off
welcome = true
padding = 12
opacity = 1.0              # 0.3–1.0; below 1 the window is see-through
blur = true                # blur what shows through
restore_session = true
editor = "auto"            # cursor, code, zed, subl, terminal, system

[font]
family = ["JetBrainsMonoNL Nerd Font", "Menlo"]
size = 14
ligatures = true

[colors]                   # override any single colour of the theme
cursor = "#3DE0F5"
```

## Accessibility

- **VoiceOver.** Kraken exposes the focused pane as a terminal element (its visible
  text), the status bar as one spoken sentence ("running cargo, in ~/Kraken, last
  command failed with exit code 1…"), and announces toasts like "Copied". Nothing is
  gathered while VoiceOver is off.
- **High contrast.** `theme = "high-contrast"`.
- **Cursor.** `[cursor] style = "block" | "beam" | "underline"`, `blink = "app" | "on" | "off"`.
  Blinking stops after 30 s idle.

## Performance

`KRAKEN_PERF=1` prints startup phases and, once a second, fps, draw-list time,
renderer prep time, lines shaped per frame, Kraken's key→present latency and memory
footprint. `cargo bench -p kraken-term --bench throughput` measures the emulation
core (MB/s and µs per full-screen draw list) without a GPU.

## Custom shortcuts

Every command has a stable id, and the `[keys]` table maps key chords to them.
The menu bar and the command palette show your bindings automatically.

```toml
[keys]
"cmd+shift+t" = "new-window"   # add or change a binding
"cmd+k"       = "none"         # unbind: the key goes to the shell instead
"cmd+alt+t"   = "set-theme:ember"
```

Ids include `new-tab`, `split-right`, `split-down`, `focus:left`, `move-pane:up`,
`resize:right`, `go-to-tab:3`, `zoom-pane`, `palette`, `agents`, `find-file`,
`set-tab-color:red`, `set-opacity:0.9` — the full list is in the generated
`kraken.toml`. The same ids work in `KRAKEN_ACTIONS`.

## Layout

| Crate | Role |
|---|---|
| `kraken-config` | Config file, themes, `[keys]` overrides |
| `kraken-term` | Shell/PTY session (alacritty_terminal), shell integration, images, grid → draw list |
| `kraken-render` | wgpu renderer: rect pipeline + glyphon text, per-line shaping cache |
| `kraken-mux` | Split-tree layout and session snapshots (no window or GPU code) |
| `kraken-ui` | Drawn widgets: status bar, sidebar, search, cards, agents, splash, sheet |
| `kraken-app` | winit windows, keymap + action registry, menu bar, input encoding, clipboard |

## Releasing

The release pipeline is `bundle → sign → notarize → dmg → appcast → cask`. Every step
that needs Apple credentials prints a notice and skips when they are absent, so the
same commands work on a fresh clone (you get an ad-hoc signed, un-notarized DMG).

### One-time setup

GitHub repository secrets (Settings → Secrets and variables → Actions), all optional:

| Secret | What |
|---|---|
| `MACOS_CERT_P12_BASE64` | `base64 -i DeveloperID.p12`: the "Developer ID Application" certificate and private key |
| `MACOS_CERT_PASSWORD` | Password of that `.p12` |
| `MACOS_SIGN_IDENTITY` | e.g. `Developer ID Application: Jane Doe (TEAMID1234)`; auto-detected when unset |
| `APPLE_API_KEY_ID`, `APPLE_API_ISSUER` | App Store Connect API key (Users and Access → Integrations, role "Developer") |
| `APPLE_API_KEY_P8_BASE64` | `base64 -i AuthKey_<KEYID>.p8` |
| `SPARKLE_ED_PRIVATE_KEY` | Sparkle EdDSA private key for signing appcast entries (see [packaging/UPDATES.md](packaging/UPDATES.md)) |

Repository variables (not secret): `KRAKEN_FEED_URL` (`SUFeedURL`), `SPARKLE_ED_PUBLIC_KEY`
(`SUPublicEDKey`), and optionally `SPARKLE_VERSION` / `SPARKLE_TARBALL_SHA256`.

### Local commands

```bash
./scripts/bundle.sh --universal              # arm64 + x86_64 → target/Kraken.app
./scripts/sign.sh                            # hardened runtime; ad-hoc unless KRAKEN_SIGN_IDENTITY is set
./scripts/notarize.sh                        # notarytool + stapler; skipped unless configured
./scripts/dmg.sh                             # target/Kraken-<version>.dmg (signed + notarized when configured)
./scripts/appcast.sh                         # adds this version to packaging/appcast.xml
./scripts/cask.sh                            # target/kraken.rb with version + sha256 filled in
```

Environment for local signing: `KRAKEN_SIGN_IDENTITY="Developer ID Application: …"`, and
either `KRAKEN_NOTARY_PROFILE=<profile>` (made once with
`xcrun notarytool store-credentials <profile>`) or `APPLE_API_KEY_ID`, `APPLE_API_ISSUER`
and `APPLE_API_KEY_PATH`. Set `KRAKEN_GITHUB_REPO=owner/repo` if `origin` isn't GitHub.
Hardened-runtime entitlements, and why each one is there, are in
[packaging/Kraken.entitlements](packaging/Kraken.entitlements).

### What's automated

`.github/workflows/release.yml` runs on `v*` tag pushes and on manual dispatch. It
checks that the tag matches the `Cargo.toml` version, imports the certificate into a
temporary keychain, runs the steps above, and uploads the DMG, `appcast.xml` and
`kraken.rb` as build artifacts. On a tag push it also creates a **draft** GitHub Release
with those files attached. Publishing the release, committing the updated
`packaging/appcast.xml`, and copying `kraken.rb` into a Homebrew tap stay manual.

To cut a release: bump `version` in the root `Cargo.toml`, commit, then
`git tag v<version> && git push origin v<version>`.

## Debug hooks

- `KRAKEN_CAPTURE=/path/frame.bmp` saves a rendered frame ~2.5 s after launch.
- `KRAKEN_INPUT='ls -la\n'` types text into the shell at startup.
- `KRAKEN_ACTIONS='split-right|input:pwd\n|wait:500|new-tab|go-to-tab:1|zoom-pane'` scripts a layout at startup (any action id, plus test helpers like `input:`, `wait:`, `find:`).
# kraken
