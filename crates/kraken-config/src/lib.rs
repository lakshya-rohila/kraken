//! Kraken configuration: fonts, window, terminal behaviour and colour themes.
//!
//! Loaded from `~/.config/kraken/kraken.toml`. Every field has a default, so the
//! file is optional and may contain only the keys you want to change.

use serde::Deserialize;
use std::path::PathBuf;

/// An sRGB colour stored as `[r, g, b]`; written in TOML as `"#RRGGBB"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const fn hex(v: u32) -> Self {
        Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    /// Linear blend: `t = 0` gives `self`, `t = 1` gives `other`.
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb(m(self.0, other.0), m(self.1, other.1), m(self.2, other.2))
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        let h = s.trim_start_matches('#');
        u32::from_str_radix(h, 16)
            .ok()
            .filter(|_| h.len() == 6)
            .map(Rgb::hex)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid colour {s:?}, expected #RRGGBB")))
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub background: Rgb,
    pub foreground: Rgb,
    /// Tab bar / title bar / status bar.
    pub surface: Rgb,
    /// Popups, palette, search bar.
    pub elevated: Rgb,
    pub border: Rgb,
    pub muted: Rgb,
    pub cursor: Rgb,
    pub cursor_text: Rgb,
    pub selection: Rgb,
    pub accent: Rgb,
    pub ssh: Rgb,
    /// ANSI 0-7.
    pub normal: [Rgb; 8],
    /// ANSI 8-15.
    pub bright: [Rgb; 8],
}

impl Theme {
    /// Built-in themes: (id used in the config file, display name).
    pub const BUILTIN: &'static [(&'static str, &'static str)] = &[
        ("kraken-abyss", "Kraken Abyss"),
        ("deep-sea", "Deep Sea"),
        ("neon-reef", "Neon Reef"),
        ("ember", "Ember"),
        ("midnight-ink", "Midnight Ink"),
        ("high-contrast", "High Contrast"),
    ];

    pub fn builtin(id: &str) -> Option<Theme> {
        let h = Rgb::hex;
        let mk = |name: &str, base: [u32; 10], normal: [u32; 8], bright: [u32; 8]| Theme {
            name: name.into(),
            background: h(base[0]),
            foreground: h(base[1]),
            surface: h(base[2]),
            elevated: h(base[3]),
            border: h(base[4]),
            muted: h(base[5]),
            cursor: h(base[6]),
            cursor_text: h(base[0]),
            selection: h(base[7]),
            accent: h(base[8]),
            ssh: h(base[9]),
            normal: normal.map(h),
            bright: bright.map(h),
        };
        Some(match id {
            "kraken-abyss" => Theme::kraken_abyss(),
            //                 bg        fg        surface   elevated  border    muted     cursor    selection accent    ssh
            "deep-sea" => mk(
                "Deep Sea",
                [0x06111A, 0xD6E7F2, 0x0B1A25, 0x10222F, 0x173041, 0x5D7A8C, 0x2EE6C5, 0x163B4F, 0x3FA9F5, 0xFFA552],
                [0x0E1E2A, 0xFF6B7A, 0x2EE6A8, 0xF2D06B, 0x4DA3FF, 0x9B8CFF, 0x2EE6E0, 0xC3D5E0],
                [0x2C4456, 0xFF95A0, 0x7BF5C8, 0xF7E29B, 0x86C2FF, 0xBDB2FF, 0x7FF3EF, 0xFFFFFF],
            ),
            "neon-reef" => mk(
                "Neon Reef",
                [0x0B0612, 0xF1E9FF, 0x120B1C, 0x1A1127, 0x2A1D3D, 0x7D6A99, 0xFF4FD8, 0x3A2050, 0xFF4FD8, 0xFFB020],
                [0x1A1125, 0xFF3D7F, 0x4DFFB0, 0xFFE24D, 0x4D9DFF, 0xFF4FD8, 0x4DF0FF, 0xD9CCEA],
                [0x3B2C4F, 0xFF7AA6, 0x8CFFD0, 0xFFEE8C, 0x8CC0FF, 0xFF8CE6, 0x8CF6FF, 0xFFFFFF],
            ),
            "ember" => mk(
                "Ember",
                [0x110B09, 0xF2E6DF, 0x18100D, 0x201612, 0x33231C, 0x8A7266, 0xFF8A3D, 0x3D261B, 0xFF6B3D, 0xFFC23D],
                [0x1F1612, 0xFF5C5C, 0x9BE564, 0xFFC857, 0x6CA8FF, 0xE58AFF, 0x5CE1E6, 0xD8C8BF],
                [0x43342D, 0xFF8A8A, 0xBDF08F, 0xFFDA8A, 0x9BC3FF, 0xF0B3FF, 0x8EEDF0, 0xFFFFFF],
            ),
            "midnight-ink" => mk(
                "Midnight Ink",
                [0x0F1117, 0xC9CEDB, 0x151821, 0x1B1F2A, 0x262B38, 0x646B80, 0xA3B8FF, 0x2A3147, 0xA3B8FF, 0xF0A35E],
                [0x1C202B, 0xE06C75, 0x98C379, 0xE5C07B, 0x61AFEF, 0xC678DD, 0x56B6C2, 0xABB2BF],
                [0x3E4452, 0xEF8C93, 0xB5D99C, 0xF0D49F, 0x8CC5F5, 0xD7A1E8, 0x82CDD6, 0xE6E9EF],
            ),
            // Every text colour is ≥ 7:1 against the background (WCAG AAA);
            // see the `high_contrast_meets_wcag_aaa` test.
            "high-contrast" => mk(
                "High Contrast",
                [0x000000, 0xFFFFFF, 0x000000, 0x141414, 0xA0A0A0, 0xC8C8C8, 0xFFFF00, 0x1E3A8A, 0xFFD400, 0xFFA040],
                [0x3A3A3A, 0xFF7070, 0x5CFF5C, 0xFFFF55, 0x7AB0FF, 0xFF85FF, 0x55FFFF, 0xEAEAEA],
                [0xA8A8A8, 0xFF9C9C, 0x9CFF9C, 0xFFFF9C, 0xA6CBFF, 0xFFB0FF, 0x9CFFFF, 0xFFFFFF],
            ),
            _ => return None,
        })
    }

    pub fn kraken_abyss() -> Self {
        Theme {
            name: "Kraken Abyss".into(),
            background: Rgb::hex(0x0A0C10),
            foreground: Rgb::hex(0xE4E8F1),
            surface: Rgb::hex(0x11141B),
            elevated: Rgb::hex(0x181C26),
            border: Rgb::hex(0x232836),
            muted: Rgb::hex(0x6E7590),
            cursor: Rgb::hex(0x3DE0F5),
            cursor_text: Rgb::hex(0x0A0C10),
            selection: Rgb::hex(0x283250),
            accent: Rgb::hex(0x8B7CFF),
            ssh: Rgb::hex(0xFF9F43),
            normal: [
                Rgb::hex(0x1A1E28), // black
                Rgb::hex(0xFF5C7A), // red
                Rgb::hex(0x3DF5A6), // green
                Rgb::hex(0xFFD166), // yellow
                Rgb::hex(0x5CA8FF), // blue
                Rgb::hex(0xC77DFF), // magenta
                Rgb::hex(0x3DE0F5), // cyan
                Rgb::hex(0xC9CFDD), // white
            ],
            bright: [
                Rgb::hex(0x3A4152),
                Rgb::hex(0xFF8AA0),
                Rgb::hex(0x7BFFC6),
                Rgb::hex(0xFFE299),
                Rgb::hex(0x8EC4FF),
                Rgb::hex(0xDCA8FF),
                Rgb::hex(0x80EEFF),
                Rgb::hex(0xFFFFFF),
            ],
        }
    }

    /// Colour for an index in the xterm 256-colour palette.
    pub fn indexed(&self, i: u8) -> Rgb {
        match i {
            0..=7 => self.normal[i as usize],
            8..=15 => self.bright[i as usize - 8],
            16..=231 => {
                let i = i - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Rgb(level(i / 36), level((i / 6) % 6), level(i % 6))
            }
            232..=255 => {
                let v = 8 + (i - 232) * 10;
                Rgb(v, v, v)
            }
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::kraken_abyss()
    }
}

/// Per-colour overrides on top of a theme (`[colors]` in the config file).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ThemeOverrides {
    pub background: Option<Rgb>,
    pub foreground: Option<Rgb>,
    pub surface: Option<Rgb>,
    pub elevated: Option<Rgb>,
    pub border: Option<Rgb>,
    pub muted: Option<Rgb>,
    pub cursor: Option<Rgb>,
    pub cursor_text: Option<Rgb>,
    pub selection: Option<Rgb>,
    pub accent: Option<Rgb>,
    pub ssh: Option<Rgb>,
    pub normal: Option<[Rgb; 8]>,
    pub bright: Option<[Rgb; 8]>,
}

impl ThemeOverrides {
    pub fn apply(&self, t: &mut Theme) {
        macro_rules! set {
            ($($f:ident),*) => { $( if let Some(v) = self.$f { t.$f = v; } )* };
        }
        set!(background, foreground, surface, elevated, border, muted, cursor, cursor_text, selection, accent, ssh, normal, bright);
    }
}

/// `theme = "deep-sea"`, or (older style) a `[theme]` table of colours.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ThemeSetting {
    Name(String),
    Colors(ThemeOverrides),
}

impl Default for ThemeSetting {
    fn default() -> Self {
        ThemeSetting::Name("kraken-abyss".into())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FontConfig {
    /// Preferred families, first one installed wins.
    pub family: Vec<String>,
    /// Size in points.
    pub size: f32,
    /// Line height as a multiple of the font size.
    pub line_height: f32,
    /// Programming ligatures: `->` `=>` `!=` `>=` drawn as single symbols.
    pub ligatures: bool,
}

impl Default for FontConfig {
    fn default() -> Self {
        FontConfig {
            family: vec![
                "JetBrainsMono Nerd Font".into(),
                "JetBrainsMonoNL Nerd Font".into(),
                "JetBrains Mono".into(),
                "SF Mono".into(),
                "Menlo".into(),
            ],
            size: 14.0,
            line_height: 1.35,
            ligatures: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CursorShapeSetting {
    #[default]
    Block,
    Beam,
    Underline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CursorBlink {
    /// Blink only when the running program asks for a blinking cursor.
    #[default]
    App,
    /// Always blink.
    On,
    /// Never blink, even if a program asks.
    Off,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CursorConfig {
    /// The cursor shape when the running program doesn't choose one.
    pub style: CursorShapeSetting,
    pub blink: CursorBlink,
    /// Half a blink cycle (on, or off), in milliseconds.
    pub blink_interval_ms: u64,
}

impl Default for CursorConfig {
    fn default() -> Self {
        CursorConfig { style: CursorShapeSetting::Block, blink: CursorBlink::App, blink_interval_ms: 530 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionAsMeta {
    None,
    Left,
    Right,
    Both,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    pub font: FontConfig,
    pub cursor: CursorConfig,
    /// The resolved theme (built-in theme plus any colour overrides).
    #[serde(skip)]
    pub theme: Theme,
    #[serde(rename = "theme")]
    pub theme_setting: ThemeSetting,
    pub colors: ThemeOverrides,
    /// Inner padding around the terminal grid, in points.
    pub padding: f32,
    pub scrollback: usize,
    /// Which Option key acts as Meta (sends ESC prefix) instead of typing symbols.
    pub option_as_meta: OptionAsMeta,
    /// Copy text to the clipboard as soon as it is selected.
    pub copy_on_select: bool,
    /// Shell program; `None` uses your login shell.
    pub shell: Option<String>,
    /// Animation speed: 1 = normal, 2 = twice as fast, 0.5 = slower, 0 = off.
    pub animation_speed: f32,
    /// Show the animated Kraken when the app starts.
    pub welcome: bool,
    /// Window background opacity, 0.3–1.0 (1 = solid).
    pub opacity: f32,
    /// Blur whatever shows through a see-through window.
    pub blur: bool,
    /// Reopen your windows, tabs and splits next time.
    pub restore_session: bool,
    /// Where files open: "auto" (Cursor, VS Code, Zed or Sublime if
    /// installed), one of those names, "terminal" ($EDITOR in a split),
    /// "system", or a path to an editor's command-line tool.
    pub editor: String,
    /// Load Kraken's shell hooks (exit codes, command timing, cwd tracking)
    /// for zsh, bash, fish, nushell and PowerShell.
    pub shell_integration: bool,
    /// Notify when a command running longer than this many seconds finishes
    /// while you're looking elsewhere. 0 turns it off.
    pub notify_after_secs: u64,
    /// How much to darken panes you're not typing in (0 = off, up to 0.85).
    pub dim_inactive: f32,
    /// A soft accent glow around the focused pane when a tab is split.
    pub focus_glow: bool,
    pub window_width: f64,
    pub window_height: f64,
    /// Keybinding overrides: chord (`"cmd+shift+t"`) → action id
    /// (`"new-window"`), or `"none"` to unbind. Parsed by the app.
    pub keys: std::collections::BTreeMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            font: FontConfig::default(),
            cursor: CursorConfig::default(),
            theme: Theme::default(),
            theme_setting: ThemeSetting::default(),
            colors: ThemeOverrides::default(),
            padding: 12.0,
            scrollback: 10_000,
            option_as_meta: OptionAsMeta::Left,
            copy_on_select: false,
            shell: None,
            animation_speed: 1.0,
            welcome: true,
            opacity: 1.0,
            blur: true,
            restore_session: true,
            editor: "auto".into(),
            shell_integration: true,
            notify_after_secs: 10,
            dim_inactive: 0.38,
            focus_glow: true,
            window_width: 1100.0,
            window_height: 700.0,
            keys: Default::default(),
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".config/kraken/kraken.toml"))
    }

    /// Load the user's config, falling back to defaults on any error.
    pub fn load() -> Self {
        Self::load_checked().unwrap_or_else(|e| {
            log::error!("{e}");
            Config::default()
        })
    }

    /// Load the config, reporting parse errors (for live reload).
    pub fn load_checked() -> Result<Self, String> {
        let Some(path) = Self::path() else { return Ok(Config::default()) };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => return Ok(Config::default()),
        };
        let mut cfg: Config = toml::from_str(&text).map_err(|e| {
            let msg = e.message().to_string();
            let line = e.span().map(|s| text[..s.start].lines().count().max(1));
            match line {
                Some(l) => format!("kraken.toml line {l}: {msg}"),
                None => format!("kraken.toml: {msg}"),
            }
        })?;
        cfg.resolve_theme()?;
        cfg.opacity = cfg.opacity.clamp(0.3, 1.0);
        Ok(cfg)
    }

    fn resolve_theme(&mut self) -> Result<(), String> {
        let mut theme = match &self.theme_setting {
            ThemeSetting::Name(id) => Theme::builtin(id).ok_or_else(|| {
                let ids: Vec<_> = Theme::BUILTIN.iter().map(|(id, _)| *id).collect();
                format!("unknown theme {id:?}; try one of: {}", ids.join(", "))
            })?,
            ThemeSetting::Colors(o) => {
                let mut t = Theme::default();
                o.apply(&mut t);
                t
            }
        };
        self.colors.apply(&mut theme);
        self.theme = theme;
        Ok(())
    }

    /// Create the config file with a commented template if it doesn't exist.
    pub fn ensure_file() -> std::io::Result<PathBuf> {
        let path = Self::path().ok_or_else(|| std::io::Error::other("no home directory"))?;
        if !path.exists() {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&path, TEMPLATE)?;
        }
        Ok(path)
    }

    /// Set a top-level `key = value` in the config file, keeping everything
    /// else (comments, sections) as it is. `value` is TOML, e.g. `"\"ember\""`.
    pub fn set_key(key: &str, value: &str) -> std::io::Result<()> {
        let path = Self::ensure_file()?;
        let text = std::fs::read_to_string(&path)?;
        std::fs::write(&path, set_top_level_key(&text, key, value))
    }
}

/// Replace (or insert) `key = value` before the first `[section]`.
pub fn set_top_level_key(text: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let first_section = lines.iter().position(|l| l.trim_start().starts_with('[')).unwrap_or(lines.len());
    let is_key = |l: &str| {
        let t = l.trim_start();
        t.strip_prefix(key).is_some_and(|rest| rest.trim_start().starts_with('='))
    };
    let new_line = format!("{key} = {value}");
    if let Some(i) = lines[..first_section].iter().position(|l| is_key(l)) {
        lines[i] = new_line;
    } else {
        // After any leading comment block, so the header stays on top.
        let at = lines[..first_section].iter().position(|l| !l.trim_start().starts_with('#')).unwrap_or(first_section);
        lines.insert(at, new_line);
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

const TEMPLATE: &str = r##"# Kraken settings. Save this file and Kraken applies changes instantly.
# Every line is optional; delete anything you don't need.

# Colour theme: kraken-abyss, deep-sea, neon-reef, ember, midnight-ink, high-contrast
theme = "kraken-abyss"

# Animation speed: 1 = normal, 2 = faster, 0.5 = slower, 0 = off
animation_speed = 1.0

# Show the animated Kraken when Kraken starts
welcome = true

# Window background opacity (0.3 – 1.0) and blur behind it
opacity = 1.0
blur = true

# Reopen your windows, tabs and splits next time
restore_session = true

# Where ⌘-click, the sidebar and ⌘P open files:
#   "auto" (Cursor, VS Code, Zed, Sublime — first one installed),
#   "cursor", "code", "zed", "subl", "terminal" ($EDITOR in a split), "system"
editor = "auto"

# Space around the text, in points
padding = 12

# Lines of history kept per pane. Each full line costs ~24 bytes per column,
# so 10000 lines of a 200-column pane can use ~48 MB; 100000 lines ~480 MB.
scrollback = 10000

# Which Option key acts as Meta (Alt) for shell shortcuts: none, left, right, both
option_as_meta = "left"

# Copy text as soon as you select it
copy_on_select = false

# Notify when a command that ran this many seconds finishes while you're
# looking elsewhere (0 = never)
notify_after_secs = 10

# Split panes: darken the ones you're not typing in (0 = off, up to 0.85),
# and draw a soft glow around the one you are
dim_inactive = 0.38
focus_glow = true

# Exit codes, timings and folder tracking (zsh, bash, fish, nushell, PowerShell)
shell_integration = true

[font]
# First installed font wins
family = ["JetBrainsMono Nerd Font", "JetBrains Mono", "SF Mono", "Menlo"]
size = 14
line_height = 1.35
# Draw -> => != >= as single symbols (fonts that support it)
ligatures = true

[cursor]
# block, beam or underline (programs like vim can still change it)
style = "block"
# "app" = blink when the program asks, "on" = always, "off" = never
blink = "app"
blink_interval_ms = 530

# Override single colours of the theme (hex). Uncomment to use.
[colors]
# background = "#0A0C10"
# cursor = "#3DE0F5"
# accent = "#8B7CFF"

# Change shortcuts: "modifiers+key" = "action". Modifiers: cmd, ctrl, alt, shift.
# Keys: a letter/symbol, or left right up down enter tab escape backspace space f1–f12.
# Actions: new-tab, new-window, split-right, split-down, close-pane, zoom-pane,
#   next-tab, prev-tab, go-to-tab:3, go-to-tab:last, focus:left, move-pane:up,
#   resize:right, find, find-file, palette, agents, toggle-sidebar, rename-tab,
#   copy, paste, select-all, clear, prev-prompt, next-prompt, scroll-top,
#   scroll-bottom, font-bigger, font-smaller, font-reset, shortcuts, summon,
#   open-settings, reload-settings, pane-to-new-tab, set-theme:ember,
#   set-tab-color:red, set-opacity:0.9, set-blur:on, quit
# "none" unbinds a key so it reaches the shell instead.
[keys]
# "cmd+shift+t" = "new-window"
# "cmd+k" = "none"
"##;

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2 contrast ratio between two colours.
    fn contrast(a: Rgb, b: Rgb) -> f64 {
        let lum = |c: Rgb| {
            let ch = |v: u8| {
                let v = v as f64 / 255.0;
                if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * ch(c.0) + 0.7152 * ch(c.1) + 0.0722 * ch(c.2)
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn high_contrast_meets_wcag_aaa() {
        let t = Theme::builtin("high-contrast").unwrap();
        let mut texts = vec![("foreground", t.foreground), ("muted", t.muted), ("accent", t.accent), ("cursor", t.cursor)];
        // ANSI black (index 0) is a background colour by convention.
        texts.extend(t.normal[1..].iter().map(|c| ("normal", *c)));
        texts.extend(t.bright.iter().map(|c| ("bright", *c)));
        for (name, c) in texts {
            let r = contrast(c, t.background);
            assert!(r >= 7.0, "{name} {c:?} is only {r:.2}:1 on the background");
        }
        assert!(contrast(t.foreground, t.selection) >= 7.0, "text on selection");
        assert!(contrast(t.cursor_text, t.cursor) >= 7.0, "text under the cursor");
    }

    #[test]
    fn template_parses_to_defaults() {
        let mut cfg: Config = toml::from_str(TEMPLATE).unwrap();
        cfg.resolve_theme().unwrap();
        assert_eq!(cfg.theme.background, Theme::default().background);
    }

    #[test]
    fn theme_by_name_with_overrides() {
        let mut cfg: Config = toml::from_str("theme = \"ember\"\n[colors]\ncursor = \"#00FF00\"\n").unwrap();
        cfg.resolve_theme().unwrap();
        assert_eq!(cfg.theme.name, "Ember");
        assert_eq!(cfg.theme.cursor, Rgb(0, 255, 0));
    }

    #[test]
    fn old_theme_table_still_works() {
        let mut cfg: Config = toml::from_str("[theme]\nbackground = \"#000000\"\n").unwrap();
        cfg.resolve_theme().unwrap();
        assert_eq!(cfg.theme.background, Rgb(0, 0, 0));
    }

    #[test]
    fn set_key_replaces_or_inserts() {
        let text = "# header\ntheme = \"ember\"\n\n[font]\nsize = 14\n";
        let out = set_top_level_key(text, "theme", "\"deep-sea\"");
        assert!(out.contains("theme = \"deep-sea\"") && !out.contains("ember"));
        let out = set_top_level_key("# c\n[font]\nsize = 1\n", "animation_speed", "2.0");
        assert_eq!(out, "# c\nanimation_speed = 2.0\n[font]\nsize = 1\n");
        // A key with the same name inside a section is left alone.
        let out = set_top_level_key("[x]\ntheme = 1\n", "theme", "\"a\"");
        assert!(out.starts_with("theme = \"a\"\n[x]\ntheme = 1"));
    }

    #[test]
    fn every_builtin_theme_resolves() {
        for (id, _) in Theme::BUILTIN {
            assert!(Theme::builtin(id).is_some(), "{id}");
        }
    }
}
