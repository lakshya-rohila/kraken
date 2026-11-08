//! Keybindings: which key chord runs which [`Action`].
//!
//! The defaults below are overridden by the `[keys]` table in kraken.toml:
//!
//! ```toml
//! [keys]
//! "cmd+shift+t" = "new-window"   # add or change a binding
//! "cmd+k"       = "none"         # unbind (the key reaches the shell instead)
//! ```
//!
//! The keyboard handler, the menu bar's accelerators and the command
//! palette's shortcut hints all read this one map, so they always agree.

use crate::actions::Action;
use std::collections::HashMap;
use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl From<ModifiersState> for Mods {
    fn from(m: ModifiersState) -> Self {
        Mods { cmd: m.super_key(), ctrl: m.control_key(), alt: m.alt_key(), shift: m.shift_key() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyName {
    /// A printable key, lowercased (`"d"`, `"["`, `"="`).
    Char(String),
    Named(NamedKey),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Chord {
    pub mods: Mods,
    pub key: KeyName,
}

const NAMED: &[(&str, NamedKey, &str, &str)] = &[
    // (config name, key, keycap label, menu accelerator name)
    ("left", NamedKey::ArrowLeft, "←", "ArrowLeft"),
    ("right", NamedKey::ArrowRight, "→", "ArrowRight"),
    ("up", NamedKey::ArrowUp, "↑", "ArrowUp"),
    ("down", NamedKey::ArrowDown, "↓", "ArrowDown"),
    ("enter", NamedKey::Enter, "↩", "Enter"),
    ("tab", NamedKey::Tab, "Tab", "Tab"),
    ("escape", NamedKey::Escape, "Esc", "Escape"),
    ("backspace", NamedKey::Backspace, "⌫", "Backspace"),
    ("delete", NamedKey::Delete, "⌦", "Delete"),
    ("space", NamedKey::Space, "Space", "Space"),
    ("pageup", NamedKey::PageUp, "PgUp", "PageUp"),
    ("pagedown", NamedKey::PageDown, "PgDn", "PageDown"),
    ("home", NamedKey::Home, "Home", "Home"),
    ("end", NamedKey::End, "End", "End"),
    ("f1", NamedKey::F1, "F1", "F1"),
    ("f2", NamedKey::F2, "F2", "F2"),
    ("f3", NamedKey::F3, "F3", "F3"),
    ("f4", NamedKey::F4, "F4", "F4"),
    ("f5", NamedKey::F5, "F5", "F5"),
    ("f6", NamedKey::F6, "F6", "F6"),
    ("f7", NamedKey::F7, "F7", "F7"),
    ("f8", NamedKey::F8, "F8", "F8"),
    ("f9", NamedKey::F9, "F9", "F9"),
    ("f10", NamedKey::F10, "F10", "F10"),
    ("f11", NamedKey::F11, "F11", "F11"),
    ("f12", NamedKey::F12, "F12", "F12"),
];

impl Chord {
    /// Parse `cmd+shift+d`, `ctrl+tab`, `cmd+alt+left`, `cmd+plus`…
    pub fn parse(s: &str) -> Result<Chord, String> {
        let lower = s.trim().to_lowercase();
        // A trailing "+" key ("cmd++") would otherwise split into an empty part.
        let lower = lower.strip_suffix("++").map(|p| format!("{p}+plus")).unwrap_or(lower);
        let mut parts: Vec<&str> = lower.split('+').map(str::trim).collect();
        let key = parts.pop().filter(|k| !k.is_empty()).ok_or_else(|| format!("no key in {s:?}"))?;
        let mut mods = Mods::default();
        for m in parts {
            match m {
                "cmd" | "command" | "super" | "⌘" => mods.cmd = true,
                "ctrl" | "control" | "⌃" => mods.ctrl = true,
                "alt" | "opt" | "option" | "⌥" => mods.alt = true,
                "shift" | "⇧" => mods.shift = true,
                other => return Err(format!("unknown modifier {other:?} in {s:?}")),
            }
        }
        let key = if let Some((_, k, ..)) = NAMED.iter().find(|(n, ..)| *n == key) {
            KeyName::Named(*k)
        } else {
            match key {
                "return" => KeyName::Named(NamedKey::Enter),
                "esc" => KeyName::Named(NamedKey::Escape),
                "arrowleft" => KeyName::Named(NamedKey::ArrowLeft),
                "arrowright" => KeyName::Named(NamedKey::ArrowRight),
                "arrowup" => KeyName::Named(NamedKey::ArrowUp),
                "arrowdown" => KeyName::Named(NamedKey::ArrowDown),
                "plus" => KeyName::Char("=".into()), // the + key is shift+= on US layouts
                "minus" => KeyName::Char("-".into()),
                k if k.chars().count() == 1 => KeyName::Char(k.into()),
                k => return Err(format!("unknown key {k:?} in {s:?}")),
            }
        };
        Ok(Chord { mods, key })
    }

    fn of(s: &str) -> Chord {
        Chord::parse(s).expect("built-in chord")
    }

    /// The chord a key press produces.
    pub fn from_event(ev: &KeyEvent, mods: ModifiersState) -> Option<Chord> {
        let key = match ev.key_without_modifiers() {
            Key::Character(c) => KeyName::Char(c.to_lowercase()),
            Key::Named(k) => KeyName::Named(k),
            _ => return None,
        };
        Some(Chord { mods: mods.into(), key })
    }

    /// Keycaps for display: `⌘⇧D`, `⌘⌥←`, `⌃Tab`.
    pub fn label(&self) -> String {
        let m = self.mods;
        let mut s = String::new();
        for (on, cap) in [(m.cmd, "⌘"), (m.ctrl, "⌃"), (m.alt, "⌥"), (m.shift, "⇧")] {
            if on {
                s.push_str(cap);
            }
        }
        match &self.key {
            KeyName::Char(c) => s.push_str(&c.to_uppercase()),
            KeyName::Named(k) => s.push_str(NAMED.iter().find(|(_, n, ..)| n == k).map_or("?", |(_, _, cap, _)| cap)),
        }
        s
    }

    /// The menu bar's accelerator spelling: `Cmd+Shift+D`.
    pub fn accelerator(&self) -> String {
        let m = self.mods;
        let mut parts: Vec<String> = [(m.cmd, "Cmd"), (m.ctrl, "Ctrl"), (m.alt, "Alt"), (m.shift, "Shift")]
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, n)| n.to_string())
            .collect();
        parts.push(match &self.key {
            KeyName::Char(c) => c.to_uppercase(),
            KeyName::Named(k) => NAMED.iter().find(|(_, n, ..)| n == k).map_or("?", |(.., a)| a).to_string(),
        });
        parts.join("+")
    }
}

/// The built-in bindings. The first chord listed for an action is the one
/// menus and the palette show.
fn defaults() -> Vec<(String, Action)> {
    use Action::*;
    use kraken_mux::layout::{Dir, Direction};
    let fixed = [
        ("cmd+t", NewTab),
        ("cmd+n", NewWindow),
        ("cmd+d", Split(Dir::Horizontal)),
        ("cmd+shift+d", Split(Dir::Vertical)),
        ("cmd+w", ClosePane),
        ("cmd+q", Quit),
        ("cmd+shift+enter", ZoomPane),
        ("cmd+shift+]", NextTab),
        ("cmd+shift+[", PrevTab),
        ("ctrl+tab", NextTab),
        ("ctrl+shift+tab", PrevTab),
        ("cmd+c", Copy),
        ("cmd+v", Paste),
        ("cmd+a", SelectAll),
        ("cmd+shift+a", Agents),
        ("cmd+shift+\\", TabOverview),
        ("cmd+f", Find),
        ("cmd+b", ToggleSidebar),
        ("cmd+shift+r", RenameTab),
        ("cmd+p", FindFile),
        ("cmd+shift+p", Palette),
        ("cmd+/", Shortcuts),
        ("cmd+shift+k", Summon),
        ("cmd+k", Clear),
        ("cmd+,", OpenSettings),
        ("cmd+=", FontBigger),
        ("cmd+-", FontSmaller),
        ("cmd+0", FontReset),
        ("cmd+up", PrevPrompt),
        ("cmd+down", NextPrompt),
        ("cmd+shift+up", ScrollTop),
        ("cmd+shift+down", ScrollBottom),
        ("cmd+9", GoToTab(usize::MAX)),
    ];
    let mut v: Vec<(String, Action)> = fixed.into_iter().map(|(c, a)| (c.to_string(), a)).collect();
    for (name, d) in [("left", Direction::Left), ("right", Direction::Right), ("up", Direction::Up), ("down", Direction::Down)] {
        v.push((format!("cmd+alt+{name}"), Focus(d)));
        v.push((format!("cmd+alt+shift+{name}"), MovePane(d)));
        let resize = Action::parse(&format!("resize:{name}")).expect("resize id");
        v.push((format!("cmd+ctrl+{name}"), resize));
    }
    for i in 1..=8 {
        v.push((format!("cmd+{i}"), GoToTab(i - 1)));
    }
    v
}

#[derive(Debug, Clone)]
pub struct Keymap {
    /// `None` = explicitly unbound by the user (the key goes to the shell).
    map: HashMap<Chord, Option<Action>>,
    /// Chords in priority order, for picking which one to display.
    order: Vec<Chord>,
}

impl Default for Keymap {
    fn default() -> Self {
        let mut km = Keymap { map: HashMap::new(), order: Vec::new() };
        for (chord, action) in defaults() {
            km.bind(Chord::of(&chord), Some(action), false);
        }
        km
    }
}

impl Keymap {
    /// Defaults plus the user's `[keys]` table. Bad entries are skipped and
    /// described in the returned errors (shown as a toast).
    pub fn with_overrides(overrides: &std::collections::BTreeMap<String, String>) -> (Keymap, Vec<String>) {
        let mut km = Keymap::default();
        let mut errors = Vec::new();
        for (chord, action) in overrides {
            let chord = match Chord::parse(chord) {
                Ok(c) => c,
                Err(e) => {
                    errors.push(format!("[keys] {e}"));
                    continue;
                }
            };
            let action = match action.trim() {
                "none" | "" => None,
                id => match Action::parse(id) {
                    Some(a) => Some(a),
                    None => {
                        errors.push(format!("[keys] unknown action {id:?}"));
                        continue;
                    }
                },
            };
            km.bind(chord, action, true);
        }
        (km, errors)
    }

    fn bind(&mut self, chord: Chord, action: Option<Action>, user: bool) {
        self.order.retain(|c| c != &chord);
        if user {
            self.order.insert(0, chord.clone());
        } else {
            self.order.push(chord.clone());
        }
        self.map.insert(chord, action);
    }

    /// The action for a key press, if it is a shortcut.
    pub fn lookup(&self, ev: &KeyEvent, mods: ModifiersState) -> Option<Action> {
        self.lookup_chord(&Chord::from_event(ev, mods)?)
    }

    pub fn lookup_chord(&self, chord: &Chord) -> Option<Action> {
        if let Some(bound) = self.map.get(chord) {
            return bound.clone();
        }
        // ⌘⇧= is ⌘+ on most layouts, ⌘⇧/ is ⌘?: let Shift fall through to the
        // unshifted ⌘ binding unless the shifted chord is bound (or unbound).
        if chord.mods.shift && chord.mods.cmd {
            let plain = Chord { mods: Mods { shift: false, ..chord.mods }, key: chord.key.clone() };
            return self.map.get(&plain).cloned().flatten();
        }
        None
    }

    /// The chord shown for an action: user bindings first, then ⌘ chords
    /// (they work as menu key equivalents), then the rest.
    pub fn chord_for(&self, action: &Action) -> Option<&Chord> {
        let bound = |c: &&Chord| self.map.get(*c).is_some_and(|a| a.as_ref() == Some(action));
        self.order.iter().filter(bound).find(|c| c.mods.cmd).or_else(|| self.order.iter().find(bound))
    }

    /// Keycaps for an action, or `""` if no key runs it.
    pub fn label(&self, action: &Action) -> String {
        self.chord_for(action).map(Chord::label).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kraken_mux::layout::{Dir, Direction};

    fn chord(s: &str) -> Chord {
        Chord::parse(s).unwrap()
    }

    #[test]
    fn parses_and_labels_chords() {
        assert_eq!(chord("cmd+shift+d").label(), "⌘⇧D");
        assert_eq!(chord("Cmd+Alt+Left").label(), "⌘⌥←");
        assert_eq!(chord("ctrl+tab").label(), "⌃Tab");
        assert_eq!(chord("cmd++"), chord("cmd+="));
        assert_eq!(chord("cmd+shift+enter").accelerator(), "Cmd+Shift+Enter");
        assert!(Chord::parse("hyper+x").is_err());
        assert!(Chord::parse("cmd+banana").is_err());
    }

    #[test]
    fn defaults_cover_the_old_hardcoded_shortcuts() {
        let km = Keymap::default();
        let l = |s: &str| km.lookup_chord(&chord(s));
        assert_eq!(l("cmd+d"), Some(Action::Split(Dir::Horizontal)));
        assert_eq!(l("cmd+shift+d"), Some(Action::Split(Dir::Vertical)));
        assert_eq!(l("cmd+shift+="), Some(Action::FontBigger)); // ⌘+ falls through
        assert_eq!(l("cmd+alt+shift+left"), Some(Action::MovePane(Direction::Left)));
        assert_eq!(l("cmd+ctrl+up"), Some(Action::Resize(Dir::Vertical, -0.05)));
        assert_eq!(l("cmd+3"), Some(Action::GoToTab(2)));
        assert_eq!(l("ctrl+shift+tab"), Some(Action::PrevTab));
        assert_eq!(l("shift+d"), None);
        assert_eq!(km.label(&Action::NextTab), "⌘⇧]");
    }

    #[test]
    fn user_overrides_rebind_and_unbind() {
        let mut o = std::collections::BTreeMap::new();
        o.insert("cmd+k".to_string(), "none".to_string());
        o.insert("cmd+shift+t".to_string(), "new-window".to_string());
        o.insert("cmd+shift+d".to_string(), "none".to_string());
        o.insert("cmd+nope".to_string(), "new-tab".to_string());
        o.insert("cmd+y".to_string(), "fly-to-moon".to_string());
        let (km, errors) = Keymap::with_overrides(&o);
        assert_eq!(errors.len(), 2);
        assert_eq!(km.lookup_chord(&chord("cmd+k")), None);
        // Unbinding a shifted chord must not fall through to the plain one.
        assert_eq!(km.lookup_chord(&chord("cmd+shift+d")), None);
        assert_eq!(km.lookup_chord(&chord("cmd+shift+t")), Some(Action::NewWindow));
        assert_eq!(km.label(&Action::NewWindow), "⌘⇧T");
        assert_eq!(km.label(&Action::Clear), "");
    }
}
