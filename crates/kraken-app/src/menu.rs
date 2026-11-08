//! The macOS menu bar. Every item maps to an [`Action`], the same list the
//! keyboard and the command palette use, so menus, keys and palette agree.

use crate::actions::Action;
use crate::keys::Keymap;
use kraken_mux::layout::{Dir, Direction};
use muda::accelerator::Accelerator;
use muda::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
use std::collections::HashMap;

/// Tab colours: (name, ANSI colour index into the theme).
pub const TAB_COLORS: &[(&str, usize)] =
    &[("Red", 1), ("Orange", 99), ("Yellow", 3), ("Green", 2), ("Blue", 4), ("Purple", 5), ("Cyan", 6)];

pub struct MenuBar {
    /// Kept alive for as long as the app runs.
    _menu: Menu,
    actions: HashMap<String, Action>,
    /// Every item with its action, to refresh accelerators when `[keys]` changes.
    items: Vec<(MenuItem, Action)>,
}

impl MenuBar {
    pub fn action(&self, id: &str) -> Option<Action> {
        self.actions.get(id).cloned()
    }

    /// Re-read shortcuts from the keymap (after the settings file changed).
    pub fn refresh(&self, keymap: &Keymap) {
        for (item, action) in &self.items {
            let _ = item.set_accelerator(accelerator(keymap, action));
        }
    }
}

/// A menu item's key equivalent. Only ⌘ chords are used: macOS menus
/// swallow their key equivalents, and plain ⌃ chords belong to the shell.
fn accelerator(keymap: &Keymap, action: &Action) -> Option<Accelerator> {
    keymap.chord_for(action).filter(|c| c.mods.cmd).and_then(|c| c.accelerator().parse().ok())
}

struct Builder<'k> {
    keymap: &'k Keymap,
    actions: HashMap<String, Action>,
    items: Vec<(MenuItem, Action)>,
}

impl Builder<'_> {
    fn item(&mut self, text: &str, action: Action) -> MenuItem {
        let id = format!("k{}", self.actions.len());
        self.actions.insert(id.clone(), action.clone());
        let item = MenuItem::with_id(id, text, true, accelerator(self.keymap, &action));
        self.items.push((item.clone(), action));
        item
    }
}

pub fn build(keymap: &Keymap) -> MenuBar {
    let mut b = Builder { keymap, actions: HashMap::new(), items: Vec::new() };
    let sep = PredefinedMenuItem::separator;
    let menu = Menu::new();

    let about = AboutMetadata {
        name: Some("Kraken".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        comments: Some("One creature, many arms.".into()),
        ..Default::default()
    };
    let app = Submenu::with_items(
        "Kraken",
        true,
        &[
            &PredefinedMenuItem::about(Some("About Kraken"), Some(about)),
            &sep(),
            &b.item("Settings…", Action::OpenSettings),
            &b.item("Reload Settings", Action::ReloadSettings),
            &sep(),
            &PredefinedMenuItem::hide(Some("Hide Kraken")),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &sep(),
            // Our own Quit, so the session is saved first.
            &b.item("Quit Kraken", Action::Quit),
        ],
    )
    .expect("app menu");

    let colors = Submenu::new("Tab Color", true);
    for (name, idx) in TAB_COLORS {
        let _ = colors.append(&b.item(name, Action::SetTabColor(Some(*idx))));
    }
    let _ = colors.append(&sep());
    let _ = colors.append(&b.item("None", Action::SetTabColor(None)));

    let shell = Submenu::with_items(
        "Shell",
        true,
        &[
            &b.item("New Tab", Action::NewTab),
            &b.item("New Window", Action::NewWindow),
            &sep(),
            &b.item("Split Right", Action::Split(Dir::Horizontal)),
            &b.item("Split Down", Action::Split(Dir::Vertical)),
            &sep(),
            &b.item("Rename Tab…", Action::RenameTab),
            &colors,
            &sep(),
            &b.item("Clear Screen & History", Action::Clear),
            &sep(),
            &b.item("Close Pane", Action::ClosePane),
        ],
    )
    .expect("shell menu");

    let edit = Submenu::with_items(
        "Edit",
        true,
        &[
            &b.item("Copy", Action::Copy),
            &b.item("Paste", Action::Paste),
            &b.item("Select All", Action::SelectAll),
            &sep(),
            &b.item("Find…", Action::Find),
            &b.item("Find File…", Action::FindFile),
            &sep(),
            &b.item("Jump to Previous Prompt", Action::PrevPrompt),
            &b.item("Jump to Next Prompt", Action::NextPrompt),
            &b.item("Scroll to Top", Action::ScrollTop),
            &b.item("Scroll to Bottom", Action::ScrollBottom),
        ],
    )
    .expect("edit menu");

    let themes = Submenu::new("Theme", true);
    for (id, name) in kraken_config::Theme::BUILTIN {
        let _ = themes.append(&b.item(name, Action::SetTheme(id)));
    }
    let opacity = Submenu::new("Window Opacity", true);
    for (label, v) in [("100% (solid)", 1.0), ("95%", 0.95), ("88%", 0.88), ("80%", 0.8), ("70%", 0.7)] {
        let _ = opacity.append(&b.item(label, Action::SetOpacity(v)));
    }
    let _ = opacity.append(&sep());
    let _ = opacity.append(&b.item("Blur On", Action::SetBlur(true)));
    let _ = opacity.append(&b.item("Blur Off", Action::SetBlur(false)));
    let anims = Submenu::new("Animations", true);
    for (label, v) in [("Off", 0.0), ("Slow & Dreamy", 0.5), ("Normal", 1.0), ("Fast", 2.0)] {
        let _ = anims.append(&b.item(label, Action::SetAnimation(v)));
    }
    let view = Submenu::with_items(
        "View",
        true,
        &[
            &b.item("Command Palette…", Action::Palette),
            &b.item("Agents", Action::Agents),
            &b.item("Tab Overview", Action::TabOverview),
            &b.item("File Sidebar", Action::ToggleSidebar),
            &sep(),
            &b.item("Zoom Pane", Action::ZoomPane),
            &sep(),
            &b.item("Bigger Font", Action::FontBigger),
            &b.item("Smaller Font", Action::FontSmaller),
            &b.item("Reset Font Size", Action::FontReset),
            &sep(),
            &themes,
            &opacity,
            &anims,
            &sep(),
            &PredefinedMenuItem::fullscreen(None),
        ],
    )
    .expect("view menu");

    let window = Submenu::with_items(
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(None),
            &sep(),
            &b.item("Next Tab", Action::NextTab),
            &b.item("Previous Tab", Action::PrevTab),
            &sep(),
            &b.item("Focus Pane Left", Action::Focus(Direction::Left)),
            &b.item("Focus Pane Right", Action::Focus(Direction::Right)),
            &b.item("Focus Pane Up", Action::Focus(Direction::Up)),
            &b.item("Focus Pane Down", Action::Focus(Direction::Down)),
            &sep(),
            &b.item("Move Pane Left", Action::MovePane(Direction::Left)),
            &b.item("Move Pane Right", Action::MovePane(Direction::Right)),
            &b.item("Move Pane Up", Action::MovePane(Direction::Up)),
            &b.item("Move Pane Down", Action::MovePane(Direction::Down)),
            &b.item("Move Pane to New Tab", Action::PaneToNewTab),
            &sep(),
            &PredefinedMenuItem::bring_all_to_front(None),
        ],
    )
    .expect("window menu");
    for i in 1..=9 {
        let action = Action::GoToTab(if i == 9 { usize::MAX } else { i - 1 });
        let label = if i == 9 { "Last Tab".to_string() } else { format!("Tab {i}") };
        let _ = window.append(&b.item(&label, action));
    }

    let help = Submenu::with_items(
        "Help",
        true,
        &[
            &b.item("Keyboard Shortcuts", Action::Shortcuts),
            &b.item("Command Palette…", Action::Palette),
            &sep(),
            &b.item("Summon the Kraken", Action::Summon),
        ],
    )
    .expect("help menu");

    let _ = menu.append_items(&[&app, &shell, &edit, &view, &window, &help]);
    menu.init_for_nsapp();
    window.set_as_windows_menu_for_nsapp();
    MenuBar { _menu: menu, actions: b.actions, items: b.items }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_default_cmd_shortcut_is_a_valid_accelerator() {
        let km = crate::keys::Keymap::default();
        for cmd in crate::actions::commands(&km) {
            if let Some(c) = km.chord_for(&cmd.action).filter(|c| c.mods.cmd) {
                assert!(c.accelerator().parse::<muda::accelerator::Accelerator>().is_ok(), "{}", c.accelerator());
            }
        }
    }

    #[test]
    fn accelerators_parse() {
        for k in [
            "Cmd+T",
            "Cmd+Shift+D",
            "Cmd+Alt+ArrowLeft",
            "Cmd+Shift+Enter",
            "Cmd+,",
            "Cmd+/",
            "Cmd+=",
            "Cmd+Shift+]",
            "Cmd+Alt+Shift+ArrowUp",
        ] {
            assert!(k.parse::<muda::accelerator::Accelerator>().is_ok(), "{k}");
        }
    }
}
