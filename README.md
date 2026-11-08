<p align="center">
  <img src="assets/icon.png" width="130" height="130" alt="Kraken App Icon" />
</p>

<h1 align="center">Kraken</h1>

<p align="center">
  <strong>A GPU-accelerated terminal for macOS, built in Rust for the coding agent era.</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/macOS-13.0%2B%20Ventura%2B-blue?logo=apple&logoColor=white" alt="macOS 13.0+" />
  <img src="https://img.shields.io/badge/Rust-2024%20Edition-orange?logo=rust&logoColor=white" alt="Rust 2024" />
  <img src="https://img.shields.io/badge/GPU-Metal%203-00d4bb" alt="Apple Metal 3" />
  <img src="https://img.shields.io/badge/Display-120Hz%20ProMotion-success" alt="120Hz ProMotion" />
  <img src="https://img.shields.io/badge/License-MIT-lightgrey" alt="License: MIT" />
</p>

<p align="center">
  <img src="assets/demo_preview.gif" width="560" alt="Kraken Terminal Demo Preview" style="border-radius: 14px; box-shadow: 0 16px 40px rgba(0,0,0,0.5);" />
</p>

---

## ⚡ Highlights

- **Sub-10ms Input Latency:** Powered by Apple Metal 3 and an instanced glyph atlas with per-line cosmic-text shaping cache. 120Hz ProMotion fluid rendering with zero keystroke lag.
- **Agent-First Interaction:** Built-in awareness for CLI coding agents (Claude Code, Codex, Gemini CLI, Aider). Non-modal **"Needs You" Cards** slide in with 1-click answers so you never have to switch tabs to grant permission.
- **Clean Multiplexer:** Native binary split trees. Drag-and-drop panes and tabs, resize dividers with mouse or hotkeys, and enjoy **clean resizing with zero prompt duplication**.
- **Full Inline Graphics Protocol:** Native support for Kitty graphics (`icat`), iTerm2 (`OSC 1337`), and Sixel image protocols right inside your terminal grid.
- **Developer Superpowers:** Integrated File Tree sidebar (`⌘B`) with Git status indicators, fuzzy file finder (`⌘P`), and `⌘-click` links that jump directly into Cursor, VS Code, Zed, or Sublime at the exact file line and column.
- **Bespoke WCAG AAA Themes:** 6 crafted high-contrast themes (`kraken-abyss`, `neon-reef`, `deep-sea`, `ember`, `midnight-ink`, `high-contrast`) with native macOS vibrancy and live hot-reloading.

---

## 📥 Download & Installation

Kraken supports macOS 13.0 (Ventura) and later on both **Apple Silicon (M1/M2/M3/M4)** and **Intel** Macs.

### Option 1: Homebrew Cask (Recommended)

Install via Homebrew:

```bash
brew tap kraken-term/tap
brew install --cask kraken
```

To update in the future:
```bash
brew upgrade --cask kraken
```

---

### Option 2: Direct Download (.DMG)

1. Download the latest `Kraken-<version>.dmg` from [**GitHub Releases**](https://github.com/kraken-term/kraken/releases).
2. Open the `.dmg` file.
3. Drag **Kraken.app** into your `/Applications` folder.
4. Launch Kraken from Spotlight (`⌘ Space` → `Kraken`) or Launchpad.

---

### Option 3: Build from Source

Requirements:
- macOS 13.0+
- Xcode Command Line Tools (`xcode-select --install`)
- Rust 1.85+ (Rust 2024 edition)

```bash
# 1. Clone the repository
git clone https://github.com/kraken-term/kraken.git
cd kraken

# 2. Build release bundle and install directly into /Applications
./scripts/bundle.sh --install

# Or build a Universal (Apple Silicon + Intel) release:
./scripts/bundle.sh --universal --install

# Or run locally in development mode:
cargo run
```

---

## ⌨️ Shortcuts Reference

### Tabs, Splits & Windows

| Key Combination | Action |
|---|---|
| <kbd>⌘</kbd> <kbd>T</kbd> | New tab (inherits current working directory) |
| <kbd>⌘</kbd> <kbd>⇧</kbd> <kbd>R</kbd> *(or double-click)* | Rename active tab (<kbd>↩</kbd> save, <kbd>Esc</kbd> cancel, empty = auto) |
| <kbd>⌘</kbd> <kbd>⇧</kbd> <kbd>A</kbd> | **Agents Dashboard** (live overview of all background agents) |
| <kbd>⌘</kbd> <kbd>⇧</kbd> <kbd>\</kbd> | **Tab Overview** (live miniature thumbnails of every tab layout) |
| <kbd>⌘</kbd> <kbd>D</kbd> / <kbd>⌘</kbd> <kbd>⇧</kbd> <kbd>D</kbd> | Split pane right / Split pane down |
| <kbd>⌘</kbd> <kbd>⌥</kbd> <kbd>←</kbd> <kbd>↑</kbd> <kbd>↓</kbd> <kbd>→</kbd> | Move focus between split panes |
| <kbd>⌘</kbd> <kbd>⌥</kbd> <kbd>⇧</kbd> <kbd>←</kbd> <kbd>↑</kbd> <kbd>↓</kbd> <kbd>→</kbd> | Swap active pane with neighbor |
| <kbd>⌘</kbd> <kbd>⌃</kbd> <kbd>←</kbd> <kbd>↑</kbd> <kbd>↓</kbd> <kbd>→</kbd> | Move nearest divider (resize panes) |
| <kbd>⌘</kbd> <kbd>⇧</kbd> <kbd>↩</kbd> | **Zoom pane toggle** (maximize focused pane) |
| <kbd>⌘</kbd> <kbd>1</kbd> … <kbd>⌘</kbd> <kbd>8</kbd>, <kbd>⌘</kbd> <kbd>9</kbd> | Go to tab 1–8 / Go to last tab |
| <kbd>⌘</kbd> <kbd>⇧</kbd> <kbd>[</kbd> / <kbd>⌘</kbd> <kbd>⇧</kbd> <kbd>]</kbd> | Previous / Next tab (also <kbd>⌃</kbd><kbd>Tab</kbd> / <kbd>⌃</kbd><kbd>⇧</kbd><kbd>Tab</kbd>) |
| <kbd>⌘</kbd> <kbd>W</kbd> | Close pane (closes tab if last pane; closes window if last tab) |
| <kbd>⌘</kbd> <kbd>N</kbd> / <kbd>⌘</kbd> <kbd>Q</kbd> | New window / Quit application |

> **Mouse Gestures:** Click any pane to focus; drag dividers to resize; drag tabs sideways to reorder; hold <kbd>⌘</kbd><kbd>⌥</kbd> and drag a pane onto another's edge to re-split, or drop onto the tab bar to extract into a new tab.

---

### Editing, Navigation & Tools

| Key Combination | Action |
|---|---|
| <kbd>⌘</kbd> <kbd>C</kbd> / <kbd>⌘</kbd> <kbd>V</kbd> | Copy selection / Smart bracketed paste |
| <kbd>⌘</kbd> <kbd>F</kbd> | Find in buffer (<kbd>↩</kbd> older, <kbd>⇧↩</kbd> newer, <kbd>⌘R</kbd> regex toggle, <kbd>Esc</kbd> close) |
| <kbd>⌘</kbd> <kbd>⇧</kbd> <kbd>P</kbd> | **Command Palette** (fuzzy search all actions with live theme preview) |
| <kbd>⌘</kbd> <kbd>P</kbd> | **Find File** (fuzzy search project files; skips `.git` and `target`) |
| <kbd>⌘</kbd> <kbd>B</kbd> | **File Sidebar** (project tree with Git colors and Nerd Font icons) |
| <kbd>⌘</kbd> + Click | Open URL in browser or jump to `file:line:col` in your editor |
| <kbd>⌘</kbd> <kbd>,</kbd> | Open settings file (`~/.config/kraken/kraken.toml`) |
| <kbd>⌘</kbd> <kbd>K</kbd> | Clear screen and scrollback |
| <kbd>⌘</kbd> <kbd>=</kbd> / <kbd>⌘</kbd> <kbd>-</kbd> / <kbd>⌘</kbd> <kbd>0</kbd> | Increase / Decrease / Reset font size |
| <kbd>⌘</kbd> <kbd>↑</kbd> / <kbd>⌘</kbd> <kbd>↓</kbd> | Jump to previous / next shell prompt (OSC 133 markers) |
| <kbd>⌘</kbd> <kbd>⇧</kbd> <kbd>↑</kbd> / <kbd>⌘</kbd> <kbd>⇧</kbd> <kbd>↓</kbd> | Scroll to top / bottom of history |
| <kbd>⇧</kbd> <kbd>↩</kbd> | Newline without submitting (Kitty keyboard protocol for AI agents) |
| <kbd>⌘</kbd> <kbd>/</kbd> | **Shortcut Sheet** (visual color-coded keycap guide) |
| <kbd>⌘</kbd> <kbd>⇧</kbd> <kbd>K</kbd> | **Summon the Kraken** (procedural welcome animation) |

---

## 🤖 Coding Agent Features

Kraken is designed specifically for developers using interactive CLI coding agents:

- **"Needs You" Interactive Cards:** When an agent in a background tab asks a question or requests permission (e.g. *Do you want to make this edit to main.rs?*), a non-modal frosted card slides in from the right. Numbered choices (**1 Yes / 2 Always / 3 No**) and `(y/n)` prompts become clickable buttons—answer the agent instantly without switching away from your work.
- **Agents Dashboard (<kbd>⌘</kbd><kbd>⇧</kbd><kbd>A</kbd>):** Real-time monitoring of all running agents across tabs with status pips:
  - <span style="color: #4DFFB0;">●</span> **Working:** Agent is actively generating or running commands.
  - <span style="color: #FFA552;">◆</span> **Waiting / Needs You:** Agent went quiet or rang the bell awaiting your input.
  - Press <kbd>↩</kbd> on any agent in the dashboard to jump straight to its pane.
- **Zero Prompt-Wrap Glitches:** Unlike traditional terminals, Kraken's prompt tracking prevents duplicated prompt lines when resizing splits or toggling the sidebar.

---

## 🎨 Configuration (`kraken.toml`)

Kraken is configured via a single TOML file at `~/.config/kraken/kraken.toml`. It is created automatically on first run with every setting documented. **Changes apply instantly upon saving** with live file-watching.

```toml
# Color Theme: kraken-abyss, deep-sea, neon-reef, ember, midnight-ink, high-contrast
theme = "kraken-abyss"

# Typography
[font]
family = ["JetBrainsMonoNL Nerd Font", "Menlo"]
size = 14.0
ligatures = true

# Window & Vibrancy
padding = 12
opacity = 0.92             # 0.3 - 1.0; values below 1.0 enable see-through glass
blur = true                 # Native macOS background blur
restore_session = true      # Resume splits, tabs, and folders on launch

# Preferred Editor for ⌘-Click and File Sidebar
# Options: "auto" (Cursor -> VS Code -> Zed -> Sublime), "terminal" ($EDITOR), "system"
editor = "auto"

# Cursor Appearance
[cursor]
style = "block"            # "block", "beam", "underline"
blink = "app"              # "app", "on", "off"

# Custom Keybindings
[keys]
"cmd+shift+t" = "new-window"
"cmd+alt+t"   = "set-theme:neon-reef"
```

---

## 🏗 Modular Architecture

Kraken is engineered as an ultra-modular Rust workspace:

| Crate | Directory | Purpose |
|---|---|---|
| **`kraken-config`** | [`crates/kraken-config`](crates/kraken-config) | Configuration parser, schema, WCAG AAA theme palettes, and keymap definitions. |
| **`kraken-term`** | [`crates/kraken-term`](crates/kraken-term) | PTY spawning, Alacritty VT parser, Kitty/iTerm2/Sixel media, and shell integration. |
| **`kraken-render`** | [`crates/kraken-render`](crates/kraken-render) | wgpu Metal 3 rendering engine, instanced quad pipelines, and cached glyph shaping. |
| **`kraken-mux`** | [`crates/kraken-mux`](crates/kraken-mux) | Headless split-tree multiplexer, layout math, and session serialization. |
| **`kraken-ui`** | [`crates/kraken-ui`](crates/kraken-ui) | Pure drawn widgets: status bar, sidebar, search, agent cards, and shortcut sheets. |
| **`kraken-app`** | [`crates/kraken-app`](crates/kraken-app) | Main application: winit event loop, AppKit menu bar, input routing, and window state. |

---

## 📄 License

Kraken is open-source software licensed under the [**MIT License**](LICENSE).
