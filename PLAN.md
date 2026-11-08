# Kraken — a GPU-accelerated Rust terminal for macOS

> **Kraken**: one creature, many arms. Every pane, tab and agent is another tentacle, all in one window.

## 1. Vision

A native macOS terminal written in Rust that is:

- **Fast**: GPU rendering, under 10 ms from keypress to screen, smooth at 120 Hz on ProMotion displays.
- **Complete**: passes the standard terminal test suites (vttest, esctest), so vim, tmux, htop, ssh, and coding agents like Claude Code, Codex, and Aider all behave correctly.
- **Beautiful**: a very dark, vivid theme, a polished tab bar, splits you can drag around, and an informative status bar.
- **Aware**: knows your current directory, git branch, and when you're SSH'd into a server, and shows it.

## 2. Tech stack (recommended)

| Layer | Choice | Why |
|---|---|---|
| Language | Rust (stable) | Speed, memory safety, good ecosystem for terminals |
| Terminal emulation (VT parser + grid) | `alacritty_terminal` crate | Battle-tested escape-sequence handling. Writing our own would take many months to reach the same correctness. |
| PTY | `portable-pty` (or `rustix` openpty) | Spawns your shell (zsh) and connects it to the terminal |
| Window + events | `winit` | Native macOS windows, keyboard, mouse, IME |
| GPU rendering | `wgpu` (Metal backend on Mac) | Draws the text grid as instanced quads from a glyph atlas |
| Text shaping & fonts | `cosmic-text` / `swash` | Ligatures, emoji, Nerd Font icons, CJK, fallback fonts |
| UI chrome (tabs, status bar, palette) | Custom wgpu-drawn widgets | Same renderer as the grid, so it stays smooth and consistent |
| macOS integration | `objc2` + `objc2-app-kit` | Native menu bar, traffic lights, blur/vibrancy, notifications, Quick Look |
| Config | TOML at `~/.config/kraken/kraken.toml`, hot-reloaded with `notify` | Edit and save to see changes live |
| Packaging | `cargo-bundle` → `Kraken.app`, then codesign + DMG | Real Mac app in /Applications |

**Alternative considered:** a Swift/AppKit shell around a Rust core. It gives more native-feeling chrome but means two languages and an FFI layer. Pure Rust stays one codebase. We can revisit later.

## 3. Architecture

```
kraken/                         (Cargo workspace)
├── crates/
│   ├── kraken-config/          TOML schema, themes, [keys] table, live reload
│   ├── kraken-term/            PTY + alacritty_terminal grid, shell integration (zsh/bash/fish/nu/pwsh),
│   │                           images (kitty/sixel/iTerm2), grid → draw list
│   ├── kraken-render/          wgpu renderer: rects, images, glyphon text with per-line shaping cache
│   ├── kraken-mux/             split tree (layout) + session snapshots; no winit/wgpu, so a
│   │                           headless mux server can own sessions later
│   ├── kraken-ui/              drawn widgets: status bar, sidebar, search, sheet, splash, cards,
│   │                           agents dashboard, link detection, animation curves
│   └── kraken-app/             main(): event loop, windows, keymap + action registry, menu bar,
│       └── src/window/         mod (state + geometry) · mux · pane · chrome · input_router ·
│                               overlays · test_actions
└── scripts/                    bundle, release, icon
```

**Commands.** Every command is an `Action` with a stable id (`split-right`,
`focus:left`, `set-theme:ember`). The keyboard (`keys.rs` keymap, user
`[keys]` overrides), the menu bar, the command palette and `KRAKEN_ACTIONS`
all resolve to the same `Action` and run through `TermWindow::perform`.

**Background work.** `tasks::spawn` (shared worker pool, short jobs) and
`tasks::spawn_long` (named thread, long-lived loops); panics in either are
caught and logged.

**Threading model**
- One I/O thread per pane reads the PTY and feeds bytes into the parser (holding a lock on the grid).
- The main thread handles input and renders frames only when something changed (damage tracking).
- Rendering is capped to the display refresh rate, so a fast `cat` of a huge file never stalls typing.

**Split tree (pane layout)**
Each tab holds a binary tree: a node is either a `Pane` or a `Split { direction, ratio, a, b }`. Dragging a divider changes the `ratio`. Dragging a pane onto another pane's edge removes it from the tree and re-inserts it as a new split. This handles any layout (2×2 grids, nested splits) with simple recursion.

## 4. Features

### 4.1 Core terminal (must be rock-solid)
- Full xterm-compatible escape sequences, 24-bit true color, 256 colors
- Unicode: emoji, CJK wide characters, combining marks, Nerd Font icons, ligatures (toggle)
- Scrollback (configurable, e.g. 100k lines), smooth/inertial scrolling
- Mouse reporting (for vim, htop, tmux), bracketed paste, focus events
- **Kitty keyboard protocol**, so Shift+Enter, Ctrl+Tab etc. reach apps correctly (important for coding agents)
- **Synchronized output (mode 2026)**: no flicker when agents redraw large UIs
- OSC 8 hyperlinks, OSC 52 clipboard (copy over SSH), OSC 9/777 desktop notifications
- **Kitty graphics protocol** + sixel: images inline in the terminal
- IME input (Hindi, Chinese, Japanese etc.), Option-as-Meta toggle
- Copy on select (optional), rectangular selection (Option+drag), smart word/URL/path selection

### 4.2 Windows, tabs, panes
- Multiple windows; multiple tabs per window; unlimited splits per tab
- Split right `⌘D`, split down `⌘⇧D`, navigate `⌘⌥←↑→↓`, zoom pane `⌘⇧↩`
- **Drag and drop**: drag tabs to reorder, drag a tab out to make a new window, drag a pane onto another pane's edge to re-split, drag dividers to resize
- Broadcast input to all panes (optional, handy for multiple servers)
- Session restore: reopen windows/tabs/splits and cwd on relaunch
- Fullscreen, native macOS tabs optional, Quake-style drop-down window on a global hotkey

### 4.3 The status ("legend") bar
A slim, colorful bar at the bottom of each pane (or the window) with segments:
- 📁 **Current directory** (via shell integration OSC 7), click to open in Finder
- 🌿 **Git branch** + dirty/ahead/behind markers
- 🖥 **SSH indicator**: turns bright orange and shows `user@host` when the pane's foreground process is `ssh`, `mosh`, or `et`. Detection: check the PTY's foreground process group (`tcgetpgrp` + `proc_pidinfo`), so it works without anything installed on the server.
- ⏱ Last command duration + ✔/✘ exit status (OSC 133)
- 🔧 Running process name (e.g. `claude`, `nvim`, `npm run dev`)
- Optional: time, battery, CPU/RAM
- Segments configurable and reorderable in config

### 4.4 Search & command palette
- `⌘F` search bar: plain/regex/case-sensitive, highlight all matches, jump through them
- `⌘⇧P` command palette: fuzzy search every action, theme switcher, open recent directories
- Jump between prompts (`⌘↑/⌘↓`) using shell-integration marks
- Select a command's entire output with one click

### 4.5 Files & code
- `⌘`-click any file path or URL in the output: opens in your editor (VS Code, Zed, nvim) at the right line (`src/main.rs:42` works)
- **File sidebar** (`⌘B`): tree of the current pane's directory, fuzzy file finder, drag a file into the terminal to paste its path
- Quick Look preview (space on a file in the sidebar)
- Built-in lightweight file viewer with syntax highlighting (`syntect`) for quick reads; editing stays in real editors

### 4.6 Coding agent friendliness
- Kitty keyboard protocol + Shift+Enter newline
- Synchronized output, high-throughput rendering
- Desktop notification + dock bounce when a long command or agent finishes (or asks for input) in a background tab
- Tab title shows agent/process activity indicator

### 4.7 Customisation
- Themes (bundled + importable from iTerm2/Alacritty/Kitty theme files)
- Fonts, font size, line height, padding, cursor style (block/beam/underline, blink)
- Background opacity + macOS blur, optional background image
- Keybindings fully remappable
- Hot reload on save

## 5. Default theme — "Kraken Abyss"

Very dark, high-contrast, vivid but not neon-harsh.

| Role | Hex | | Role | Hex |
|---|---|---|---|---|
| Background | `#0A0C10` | | Foreground | `#E4E8F1` |
| Surface (tab bar, status bar) | `#11141B` | | Muted text | `#6E7590` |
| Elevated (palette, popups) | `#181C26` | | Border | `#232836` |
| Cursor | `#3DE0F5` | | Selection | `#283250` |
| Accent | `#8B7CFF` | | SSH alert | `#FF9F43` |

ANSI 16:

| | Normal | Bright |
|---|---|---|
| Black | `#1A1E28` | `#3A4152` |
| Red | `#FF5C7A` | `#FF8AA0` |
| Green | `#3DF5A6` | `#7BFFC6` |
| Yellow | `#FFD166` | `#FFE299` |
| Blue | `#5CA8FF` | `#8EC4FF` |
| Magenta | `#C77DFF` | `#DCA8FF` |
| Cyan | `#3DE0F5` | `#80EEFF` |
| White | `#C9CFDD` | `#FFFFFF` |

Every color is checked for ≥ 4.5:1 contrast on the background so text stays readable.

## 6. Performance targets
- Startup to first prompt: < 150 ms
- Input latency: < 10 ms
- 120 fps scrolling on ProMotion
- `cat` a 1 GB file without the UI freezing
- Idle CPU ≈ 0% (render only on change)
- Memory: < 60 MB for one window with one tab

## 7. Milestones

| Phase | Goal | Rough size |
|---|---|---|
| **0. Setup** | Install Rust, create workspace, CI, `cargo-bundle` produces an empty Kraken.app | 1 day |
| **1. It's a terminal** ✅ | One window, one shell, GPU text grid, keyboard, colors, scrollback, copy/paste, resize, Kraken Abyss theme | 1–2 weeks |
| **2. Tabs & splits** ✅ | Tab bar, split tree, keyboard navigation, dividers, multiple windows | 1–2 weeks |
| **3. Status bar & awareness** ✅ | Shell integration, cwd, git, SSH detection, exit codes, notifications | 1 week |
| **4. Delight: shortcuts, animation, ASCII art** ✅ | `⌘/` shortcut sheet listing every key and what it does (rich colors); animations (tabs, splits, focus, cursor, smooth scroll); ASCII-art Kraken welcome screen. The user will describe the exact animation style they want. | 1–2 weeks |
| **5. Search & palette** ✅ | `⌘F` search, `⌘⇧P` command palette, `⌘,` settings, live config reload, prompt jumping | 1 week |
| **6. Drag & drop + polish** ✅ | Drag panes to re-split, blur, session restore | 1–2 weeks |
| **7. Files & code** ✅ | ⌘-click paths, file sidebar, Quick Look, viewer | 1–2 weeks |
| **8. Advanced protocols** ✅ | Kitty keyboard + graphics, sixel, ligatures, IME polish | 2 weeks |
| **8½. Polish backlog** | ✅ Prompt-aware resize · ✅ menu bar · ✅ tab names & colours · ✅ agents dashboard · still open: built-in file viewer with syntax highlighting; bash/fish integration | — |
| **9. Ship it** | Config docs, theme import, codesign, DMG, auto-update | 1 week |
| **10. Foundations** | See checklist below: architecture, tests, CI, error handling, rendering correctness, performance, shipping, accessibility | 3–6 weeks |

### Phase 10 checklist
- [x] Central action registry (stable ids; keys, menu, palette, `KRAKEN_ACTIONS` share one path) + remappable `[keys]`
- [x] `window.rs` split into `window/` submodules; all background threads go through `tasks`
- [x] `kraken-mux` and `kraken-ui` crates extracted
- [x] Config file watching (`notify`, directory-level so atomic saves work)
- [x] Panic hook saves the session + crash report; `unwrap`/`unsafe` audit (`undocumented_unsafe_blocks` enforced)
- [x] Undercurl, dotted/dashed/double underlines, SGR 58 underline colour; cursor blink + `[cursor]` config (SGR 21 still = cancel-bold in vte)
- [x] Golden draw-list tests (vttest-themed VT cases), smoke fuzzing on stable + cargo-fuzz targets weekly
- [ ] Real vttest/esctest run through a headless PTY harness that answers DA/CPR
- [x] CI: fmt, clippy `-D warnings`, tests, cargo-deny, cargo-audit, release build
- [x] Measurement: `KRAKEN_PERF`, throughput bench; content-addressed shaping cache; menu bar off the startup path; scrollback default 10k
- [ ] Per-cell glyph atlas for monospace text (shaping is ~0.1 ms/line; a full-screen change ≈ 7 ms at 120 Hz)
- [ ] Sub-line smooth trackpad scrolling (needs the row above the viewport)
- [x] Shell integration for bash (verified live, bash 3.2), fish, nushell, PowerShell (unit-tested; not run here)
- [x] Release pipeline: universal build, codesign, notarize, DMG, appcast, Homebrew cask (signing untested without Apple credentials)
- [ ] Auto-update inside the app (Sparkle via objc2, or GitHub-releases check; see packaging/UPDATES.md)
- [x] Accessibility: AccessKit/VoiceOver tree, live announcements, high-contrast theme (VoiceOver not yet checked by ear)

You can use it as your daily terminal from the end of Phase 2.

## 8. Risks & how we handle them
- **Escape-sequence correctness** is the hardest part of any terminal → reuse `alacritty_terminal` instead of writing our own parser.
- **Font rendering quality on Mac** (subpixel positioning, Retina scaling) → `swash` rasteriser, test against Terminal.app side by side.
- **IME and keyboard edge cases** on macOS → dedicated test checklist (dead keys, Option combos, Hindi/Japanese input).
- **Scope creep** → phases above. Each phase ends with a usable build.

## 9. Not in v1
- Windows/Linux builds (architecture keeps it possible later)
- Built-in AI assistant
- Plugin system / scripting (Lua or WASM later)
- Full code editor inside the terminal

## 10. Before Phase 0
- Rust is **not installed** on this Mac yet. Install with:
  `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Xcode is installed (needed for Metal and codesigning).
