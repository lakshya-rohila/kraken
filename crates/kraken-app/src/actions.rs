//! Every command Kraken can run. Keyboard shortcuts and the command palette
//! both go through this list, so they can never disagree.

use kraken_mux::layout::{Dir, Direction};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    NewTab,
    NewWindow,
    Split(Dir),
    ClosePane,
    ZoomPane,
    NextTab,
    PrevTab,
    Focus(Direction),
    /// Move the nearest divider along an axis (keyboard only).
    Resize(Dir, f32),
    /// Tab index; `usize::MAX` = last tab (keyboard only).
    GoToTab(usize),
    Find,
    Copy,
    Paste,
    SelectAll,
    Clear,
    PrevPrompt,
    NextPrompt,
    ScrollTop,
    ScrollBottom,
    FontBigger,
    FontSmaller,
    FontReset,
    Shortcuts,
    Palette,
    Summon,
    OpenSettings,
    ReloadSettings,
    SetTheme(&'static str),
    SetAnimation(f32),
    /// Swap the focused pane with its neighbour.
    MovePane(Direction),
    PaneToNewTab,
    SetOpacity(f32),
    SetBlur(bool),
    ToggleSidebar,
    RenameTab,
    /// Colour the current tab: an ANSI colour index (99 = the theme's orange), or none.
    SetTabColor(Option<usize>),
    /// The ⌘⇧A agents dashboard.
    Agents,
    /// The ⌘⇧\ tab overview: every tab as a live thumbnail.
    TabOverview,
    FindFile,
    /// Open in your editor, optionally at line/column.
    OpenFile(std::path::PathBuf, Option<u32>, Option<u32>),
    /// Edit with $EDITOR in a new split.
    EditInTerminal(std::path::PathBuf, Option<u32>),
    /// Type the (quoted) path into the focused pane.
    InsertPath(std::path::PathBuf),
    Quit,
}

const DIRS: [(&str, Direction); 4] =
    [("left", Direction::Left), ("right", Direction::Right), ("up", Direction::Up), ("down", Direction::Down)];

/// Actions without parameters, by their stable id (used in `[keys]`,
/// `KRAKEN_ACTIONS` and anywhere else a command is named in text).
fn simple() -> [(&'static str, Action); 32] {
    use Action::*;
    [
        ("new-tab", NewTab),
        ("new-window", NewWindow),
        ("split-right", Split(Dir::Horizontal)),
        ("split-down", Split(Dir::Vertical)),
        ("close-pane", ClosePane),
        ("zoom-pane", ZoomPane),
        ("next-tab", NextTab),
        ("prev-tab", PrevTab),
        ("find", Find),
        ("copy", Copy),
        ("paste", Paste),
        ("select-all", SelectAll),
        ("clear", Clear),
        ("prev-prompt", PrevPrompt),
        ("next-prompt", NextPrompt),
        ("scroll-top", ScrollTop),
        ("scroll-bottom", ScrollBottom),
        ("font-bigger", FontBigger),
        ("font-smaller", FontSmaller),
        ("font-reset", FontReset),
        ("shortcuts", Shortcuts),
        ("palette", Palette),
        ("summon", Summon),
        ("open-settings", OpenSettings),
        ("reload-settings", ReloadSettings),
        ("pane-to-new-tab", PaneToNewTab),
        ("toggle-sidebar", ToggleSidebar),
        ("rename-tab", RenameTab),
        ("agents", Agents),
        ("tab-overview", TabOverview),
        ("find-file", FindFile),
        ("quit", Quit),
    ]
}

impl Action {
    /// The stable text id: `split-right`, `focus:left`, `go-to-tab:3`,
    /// `set-theme:ember`… [`Action::parse`] turns it back into the action.
    pub fn id(&self) -> String {
        if let Some((id, _)) = simple().into_iter().find(|(_, a)| a == self) {
            return id.to_string();
        }
        let dir = |d: &Direction| DIRS.iter().find(|(_, x)| x == d).map_or("left", |(n, _)| n);
        match self {
            Action::Focus(d) => format!("focus:{}", dir(d)),
            Action::MovePane(d) => format!("move-pane:{}", dir(d)),
            Action::Resize(Dir::Horizontal, v) => format!("resize:{}", if *v < 0.0 { "left" } else { "right" }),
            Action::Resize(Dir::Vertical, v) => format!("resize:{}", if *v < 0.0 { "up" } else { "down" }),
            Action::GoToTab(usize::MAX) => "go-to-tab:last".into(),
            Action::GoToTab(i) => format!("go-to-tab:{}", i + 1),
            Action::SetTheme(t) => format!("set-theme:{t}"),
            Action::SetAnimation(v) => format!("set-animation:{v}"),
            Action::SetOpacity(v) => format!("set-opacity:{v}"),
            Action::SetBlur(on) => format!("set-blur:{}", if *on { "on" } else { "off" }),
            Action::SetTabColor(None) => "set-tab-color:none".into(),
            Action::SetTabColor(Some(i)) => {
                let name = crate::menu::TAB_COLORS.iter().find(|(_, x)| x == i).map_or("none", |(n, _)| n);
                format!("set-tab-color:{}", name.to_lowercase())
            }
            Action::OpenFile(p, ..) => format!("open-file:{}", p.display()),
            Action::EditInTerminal(p, _) => format!("edit-in-terminal:{}", p.display()),
            Action::InsertPath(p) => format!("insert-path:{}", p.display()),
            _ => unreachable!("every parameterless action is in simple()"),
        }
    }

    /// Parse an action id (see [`Action::id`]). Also accepts the older
    /// `KRAKEN_ACTIONS` spellings `zoom`, `tab-N` and `focus-left`.
    pub fn parse(s: &str) -> Option<Action> {
        let s = s.trim();
        if let Some((_, a)) = simple().into_iter().find(|(id, _)| *id == s) {
            return Some(a);
        }
        if s == "zoom" {
            return Some(Action::ZoomPane);
        }
        let (name, arg) = s.split_once(':').or_else(|| s.split_once('-').filter(|(n, _)| matches!(*n, "tab" | "focus")))?;
        let dir = || DIRS.iter().find(|(n, _)| *n == arg).map(|(_, d)| *d);
        Some(match name {
            "focus" => Action::Focus(dir()?),
            "move-pane" => Action::MovePane(dir()?),
            "resize" => match dir()? {
                Direction::Left => Action::Resize(Dir::Horizontal, -0.05),
                Direction::Right => Action::Resize(Dir::Horizontal, 0.05),
                Direction::Up => Action::Resize(Dir::Vertical, -0.05),
                Direction::Down => Action::Resize(Dir::Vertical, 0.05),
            },
            "go-to-tab" | "tab" if arg == "last" => Action::GoToTab(usize::MAX),
            "go-to-tab" | "tab" => Action::GoToTab(arg.parse::<usize>().ok()?.checked_sub(1)?),
            "set-theme" => Action::SetTheme(kraken_config::Theme::BUILTIN.iter().find(|(id, _)| *id == arg)?.0),
            "set-animation" => Action::SetAnimation(arg.parse().ok()?),
            "set-opacity" => Action::SetOpacity(arg.parse().ok()?),
            "set-blur" => Action::SetBlur(matches!(arg, "on" | "true")),
            "set-tab-color" if arg == "none" => Action::SetTabColor(None),
            "set-tab-color" => Action::SetTabColor(Some(crate::menu::TAB_COLORS.iter().find(|(n, _)| n.eq_ignore_ascii_case(arg))?.1)),
            "open-file" => Action::OpenFile(arg.into(), None, None),
            "edit-in-terminal" => Action::EditInTerminal(arg.into(), None),
            "insert-path" => Action::InsertPath(arg.into()),
            _ => return None,
        })
    }
}

pub struct Command {
    pub name: String,
    pub action: Action,
    /// The shortcut as keycaps (`⌘⇧D`), from the live keymap; empty if unbound.
    pub keys: String,
}

/// The command palette's list, in display order.
pub fn commands(keymap: &crate::keys::Keymap) -> Vec<Command> {
    let c = |name: &str, action: Action, _hint: &str| Command { name: name.to_string(), keys: keymap.label(&action), action };
    let mut v = vec![
        c("New Tab", Action::NewTab, "⌘T"),
        c("New Window", Action::NewWindow, "⌘N"),
        c("Split Right", Action::Split(Dir::Horizontal), "⌘D"),
        c("Split Down", Action::Split(Dir::Vertical), "⌘⇧D"),
        c("Close Pane", Action::ClosePane, "⌘W"),
        c("Zoom Pane", Action::ZoomPane, "⌘⇧↩"),
        c("Rename Tab…", Action::RenameTab, "⌘⇧R"),
        c("Agents Dashboard", Action::Agents, "⌘⇧A"),
        c("Tab Overview", Action::TabOverview, "⌘⇧\\"),
        c("Next Tab", Action::NextTab, "⌃Tab"),
        c("Previous Tab", Action::PrevTab, "⌃⇧Tab"),
        c("Focus Pane Left", Action::Focus(Direction::Left), "⌘⌥←"),
        c("Focus Pane Right", Action::Focus(Direction::Right), "⌘⌥→"),
        c("Focus Pane Up", Action::Focus(Direction::Up), "⌘⌥↑"),
        c("Focus Pane Down", Action::Focus(Direction::Down), "⌘⌥↓"),
        c("Move Pane Left", Action::MovePane(Direction::Left), "⌘⌥⇧←"),
        c("Move Pane Right", Action::MovePane(Direction::Right), "⌘⌥⇧→"),
        c("Move Pane Up", Action::MovePane(Direction::Up), "⌘⌥⇧↑"),
        c("Move Pane Down", Action::MovePane(Direction::Down), "⌘⌥⇧↓"),
        c("Move Pane to New Tab", Action::PaneToNewTab, ""),
        c("Find…", Action::Find, "⌘F"),
        c("Find File…", Action::FindFile, "⌘P"),
        c("Toggle File Sidebar", Action::ToggleSidebar, "⌘B"),
        c("Copy", Action::Copy, "⌘C"),
        c("Paste", Action::Paste, "⌘V"),
        c("Select All", Action::SelectAll, "⌘A"),
        c("Clear Screen & History", Action::Clear, "⌘K"),
        c("Jump to Previous Prompt", Action::PrevPrompt, "⌘↑"),
        c("Jump to Next Prompt", Action::NextPrompt, "⌘↓"),
        c("Scroll to Top", Action::ScrollTop, "⌘⇧↑"),
        c("Scroll to Bottom", Action::ScrollBottom, "⌘⇧↓"),
        c("Bigger Font", Action::FontBigger, "⌘="),
        c("Smaller Font", Action::FontSmaller, "⌘-"),
        c("Reset Font Size", Action::FontReset, "⌘0"),
        c("Show Shortcuts", Action::Shortcuts, "⌘/"),
        c("Summon the Kraken", Action::Summon, "⌘⇧K"),
        c("Open Settings", Action::OpenSettings, "⌘,"),
        c("Reload Settings", Action::ReloadSettings, ""),
    ];
    for (id, name) in kraken_config::Theme::BUILTIN {
        v.push(c(&format!("Theme: {name}"), Action::SetTheme(id), ""));
    }
    for (label, v_) in [("100% (solid)", 1.0), ("95%", 0.95), ("88%", 0.88), ("80%", 0.8), ("70%", 0.7)] {
        v.push(c(&format!("Window Opacity: {label}"), Action::SetOpacity(v_), ""));
    }
    v.push(c("Background Blur: On", Action::SetBlur(true), ""));
    v.push(c("Background Blur: Off", Action::SetBlur(false), ""));
    for (name, idx) in crate::menu::TAB_COLORS {
        v.push(c(&format!("Tab Color: {name}"), Action::SetTabColor(Some(*idx)), ""));
    }
    v.push(c("Tab Color: None", Action::SetTabColor(None), ""));
    v.push(c("Animations: Off", Action::SetAnimation(0.0), ""));
    v.push(c("Animations: Normal", Action::SetAnimation(1.0), ""));
    v.push(c("Animations: Fast", Action::SetAnimation(2.0), ""));
    v.push(c("Animations: Slow & Dreamy", Action::SetAnimation(0.5), ""));
    v.push(c("Quit Kraken", Action::Quit, "⌘Q"));
    v
}

/// Fuzzy match: every query character must appear in order. Returns a score
/// (higher is better) and the matched character positions for highlighting.
pub fn fuzzy(query: &str, text: &str) -> Option<(i32, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let chars: Vec<char> = text.chars().collect();
    let mut positions = Vec::new();
    let mut score = 0;
    let mut ti = 0;
    let mut prev: Option<usize> = None;
    for q in query.chars().filter(|c| !c.is_whitespace()) {
        let q = q.to_ascii_lowercase();
        let found = (ti..chars.len()).find(|&i| chars[i].to_ascii_lowercase() == q)?;
        score += 10;
        let word_start = found == 0 || !chars[found - 1].is_alphanumeric();
        if word_start {
            score += 14;
        }
        match prev {
            Some(p) if p + 1 == found => score += 9,
            Some(p) => score -= (found - p - 1).min(8) as i32,
            None => score -= found.min(10) as i32,
        }
        positions.push(found);
        prev = Some(found);
        ti = found + 1;
    }
    // Typing a whole word or phrase should beat scattered letters.
    if text.to_lowercase().contains(&query.to_lowercase()) {
        score += 25;
    }
    score -= (chars.len() as i32) / 8;
    Some((score, positions))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_prefers_word_starts_and_runs() {
        let (split_r, pos) = fuzzy("sr", "Split Right").unwrap();
        assert_eq!(pos, vec![0, 6]);
        let (scroll, _) = fuzzy("sr", "Scroll to Bottom").unwrap();
        assert!(split_r > scroll);
        assert!(fuzzy("xyz", "New Tab").is_none());
        let (exact, _) = fuzzy("theme", "Theme: Ember").unwrap();
        let (loose, _) = fuzzy("theme", "The Message").unwrap_or((i32::MIN, vec![]));
        assert!(exact > loose);
    }

    #[test]
    fn every_command_has_a_name() {
        assert!(commands(&crate::keys::Keymap::default()).iter().all(|c| !c.name.is_empty()));
    }

    #[test]
    fn every_palette_action_round_trips_through_its_id() {
        for cmd in commands(&crate::keys::Keymap::default()) {
            let id = cmd.action.id();
            assert_eq!(Action::parse(&id), Some(cmd.action.clone()), "{id}");
        }
        for (id, _) in simple() {
            assert_eq!(Action::parse(id).map(|a| a.id()).as_deref(), Some(id));
        }
    }

    #[test]
    fn legacy_spellings_still_parse() {
        assert_eq!(Action::parse("zoom"), Some(Action::ZoomPane));
        assert_eq!(Action::parse("tab-2"), Some(Action::GoToTab(1)));
        assert_eq!(Action::parse("focus-left"), Some(Action::Focus(Direction::Left)));
        assert_eq!(Action::parse("go-to-tab:last"), Some(Action::GoToTab(usize::MAX)));
        assert_eq!(Action::parse("bogus"), None);
    }
}
