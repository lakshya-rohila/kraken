//! ⌘B file sidebar: the focused pane's project as a tree, coloured by git
//! status. Also the file walker behind the ⌘P file finder.

use kraken_config::{Rgb, Theme};
use kraken_mux::layout::R;
use kraken_render::{FontKind, Frame, Rect, Span, TextLine};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

pub const WIDTH_PT: f32 = 270.0;

/// Folders that are huge, generated, or private to tools.
const SKIP_DIRS: &[&str] =
    &[".git", "node_modules", "target", ".next", "dist", "build", ".venv", "venv", "__pycache__", ".cache", ".DS_Store"];

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
}

pub struct Sidebar {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
    expanded: HashSet<PathBuf>,
    pub selected: usize,
    pub scroll: usize,
    /// Keyboard focus is in the sidebar (not the terminal).
    pub focused: bool,
    /// You navigated somewhere yourself: stop following the pane's folder.
    pub pinned: bool,
    /// Git changes keyed by path relative to `git_root`.
    pub changes: BTreeMap<String, char>,
    pub git_root: Option<PathBuf>,
}

impl Sidebar {
    pub fn new(root: PathBuf) -> Self {
        let mut s = Sidebar {
            root,
            entries: Vec::new(),
            expanded: HashSet::new(),
            selected: 0,
            scroll: 0,
            focused: true,
            pinned: false,
            changes: BTreeMap::new(),
            git_root: None,
        };
        s.rebuild();
        s
    }

    pub fn set_root(&mut self, root: PathBuf) {
        if root != self.root {
            self.root = root;
            self.expanded.clear();
            self.selected = 0;
            self.scroll = 0;
            self.rebuild();
        }
    }

    /// Re-read the visible part of the tree from disk.
    pub fn rebuild(&mut self) {
        let keep = self.entries.get(self.selected).map(|e| e.path.clone());
        self.entries.clear();
        let root = self.root.clone();
        self.add_dir(&root, 0);
        if let Some(k) = keep {
            self.selected = self.entries.iter().position(|e| e.path == k).unwrap_or(self.selected);
        }
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
    }

    fn add_dir(&mut self, dir: &Path, depth: usize) {
        for (path, is_dir) in list_dir(dir) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let open = is_dir && self.expanded.contains(&path);
            self.entries.push(Entry { path: path.clone(), name, depth, is_dir });
            if open {
                self.add_dir(&path, depth + 1);
            }
        }
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    pub fn move_by(&mut self, d: i32) {
        if !self.entries.is_empty() {
            self.selected = (self.selected as i32 + d).clamp(0, self.entries.len() as i32 - 1) as usize;
        }
    }

    /// Open or close the selected folder. Returns false if it's a file.
    pub fn toggle(&mut self) -> bool {
        let Some(e) = self.selected_entry().cloned() else { return false };
        if !e.is_dir {
            return false;
        }
        if !self.expanded.remove(&e.path) {
            self.expanded.insert(e.path);
        }
        self.rebuild();
        true
    }

    pub fn expand(&mut self) {
        if let Some(e) = self.selected_entry().filter(|e| e.is_dir).cloned()
            && self.expanded.insert(e.path)
        {
            self.rebuild();
        }
    }

    /// Collapse the folder, or jump to its parent when already closed.
    pub fn collapse(&mut self) {
        let Some(e) = self.selected_entry().cloned() else { return };
        if e.is_dir && self.expanded.remove(&e.path) {
            self.rebuild();
        } else if let Some(i) = self.entries[..self.selected].iter().rposition(|p| p.depth + 1 == e.depth) {
            self.selected = i;
        }
    }

    pub fn go_up(&mut self) {
        if let Some(parent) = self.root.parent().map(Path::to_path_buf) {
            self.pinned = true;
            self.set_root(parent);
        }
    }

    /// Git status letter for a path (folders show if anything inside changed).
    pub fn change_of(&self, path: &Path, is_dir: bool) -> Option<char> {
        let rel = path.strip_prefix(self.git_root.as_ref()?).ok()?.to_string_lossy().into_owned();
        if let Some(c) = self.changes.get(&rel) {
            return Some(*c);
        }
        if is_dir {
            let prefix = format!("{rel}/");
            if self.changes.keys().any(|k| k.starts_with(&prefix)) {
                return Some('•');
            }
        }
        None
    }
}

/// Sorted directory listing: folders first, then files, case-insensitively.
pub fn list_dir(dir: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut items: Vec<(PathBuf, bool)> = rd
        .filter_map(Result::ok)
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n != ".git" && n != ".DS_Store"
        })
        .map(|e| {
            let is_dir =
                e.file_type().map(|t| t.is_dir()).unwrap_or(false) || (e.file_type().is_ok_and(|t| t.is_symlink()) && e.path().is_dir());
            (e.path(), is_dir)
        })
        .collect();
    items.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| {
            a.0.file_name().map(|n| n.to_string_lossy().to_lowercase()).cmp(&b.0.file_name().map(|n| n.to_string_lossy().to_lowercase()))
        })
    });
    items
}

/// Every file under `root` (skipping heavy folders), as paths relative to it.
pub fn walk_files(root: &Path, limit: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for (path, is_dir) in list_dir(&dir) {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if is_dir {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(path);
                }
            } else if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().into_owned());
                if out.len() >= limit {
                    return out;
                }
            }
        }
    }
    out.sort_by_key(|p| (p.matches('/').count(), p.to_lowercase()));
    out
}

/// Nerd Font icon and theme colour (ANSI index) for a file name.
pub fn icon_for(name: &str, is_dir: bool, open: bool) -> (&'static str, usize) {
    if is_dir {
        return (if open { "\u{f07c}" } else { "\u{f07b}" }, 4);
    }
    let lower = name.to_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    match (lower.as_str(), ext) {
        ("cargo.toml" | "cargo.lock", _) => ("\u{e7a8}", 1),
        ("dockerfile", _) => ("\u{f308}", 4),
        ("makefile", _) => ("\u{f489}", 3),
        (_, "rs") => ("\u{e7a8}", 1),
        (_, "js" | "mjs" | "cjs") => ("\u{e74e}", 3),
        (_, "ts" | "tsx" | "jsx") => ("\u{e628}", 4),
        (_, "py") => ("\u{e73c}", 3),
        (_, "go") => ("\u{e626}", 6),
        (_, "swift") => ("\u{e755}", 1),
        (_, "java" | "kt") => ("\u{e738}", 1),
        (_, "c" | "h") => ("\u{e61e}", 4),
        (_, "cpp" | "cc" | "hpp") => ("\u{e61d}", 4),
        (_, "rb") => ("\u{e739}", 1),
        (_, "md" | "mdx") => ("\u{f48a}", 7),
        (_, "json") => ("\u{e60b}", 3),
        (_, "toml" | "yaml" | "yml" | "ini" | "conf") => ("\u{f013}", 5),
        (_, "html") => ("\u{e736}", 1),
        (_, "css" | "scss") => ("\u{e749}", 4),
        (_, "sh" | "zsh" | "bash" | "fish") => ("\u{f489}", 2),
        (_, "lock") => ("\u{f023}", 8),
        (_, "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "icns") => ("\u{f1c5}", 5),
        (_, "pdf") => ("\u{f1c1}", 1),
        (_, "zip" | "gz" | "tar") => ("\u{f410}", 3),
        _ => ("\u{f15b}", 7),
    }
}

pub struct SidebarGeom {
    pub rect: R,
    pub scale: f32,
    pub char_w: f32,
    pub font_px: f32,
}

impl SidebarGeom {
    pub fn row_h(&self) -> f32 {
        (self.font_px * 1.75).round()
    }
    pub fn header_h(&self) -> f32 {
        self.row_h() * 1.6
    }
    pub fn visible_rows(&self) -> usize {
        ((self.rect.h - self.header_h() - self.row_h() * 1.4) / self.row_h()).max(1.0) as usize
    }
    /// Entry index under a point, if any.
    pub fn row_at(&self, sb: &Sidebar, y: f32) -> Option<usize> {
        let top = self.rect.y + self.header_h();
        if y < top {
            return None;
        }
        let i = sb.scroll + ((y - top) / self.row_h()) as usize;
        (i < sb.entries.len()).then_some(i)
    }
}

/// Keep the selection on screen.
pub fn clamp_scroll(sb: &mut Sidebar, g: &SidebarGeom) {
    let rows = g.visible_rows();
    if sb.selected < sb.scroll {
        sb.scroll = sb.selected;
    } else if sb.selected >= sb.scroll + rows {
        sb.scroll = sb.selected + 1 - rows;
    }
}

fn rgb(c: Rgb) -> [u8; 3] {
    [c.0, c.1, c.2]
}

pub fn draw(frame: &mut Frame, t: &Theme, g: &SidebarGeom, sb: &Sidebar, chrome_alpha: u8) {
    let s = g.scale;
    let r = g.rect;
    let sf = t.surface;
    frame.under.push(Rect { x: r.x, y: r.y, w: r.w, h: r.h, color: [sf.0, sf.1, sf.2, chrome_alpha] });
    frame.under.push(Rect::new(r.right() - s, r.y, s, r.h, rgb(t.border)));
    let clip = Some([r.x as i32, r.y as i32, (r.right() - s) as i32, r.bottom() as i32]);
    let row_h = g.row_h();
    let line = |x: f32, y: f32, spans: Vec<Span>| TextLine {
        x,
        y: y + (row_h - g.font_px * 1.3) / 2.0,
        font: FontKind::Mono,
        size: g.font_px,
        line_height: g.font_px * 1.3,
        spans,
        clip,
        cell_width: Some(g.char_w),
    };
    let span = |text: String, c: Rgb, bold: bool| Span { text, color: rgb(c), bold, italic: false };

    // Header: project name and where it lives.
    let name = sb.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "/".into());
    let pad = 12.0 * s;
    let hy = r.y + (g.header_h() - row_h) / 2.0;
    let mut header = vec![span("\u{f07c} ".into(), t.accent, false), span(name.to_uppercase(), t.foreground, true)];
    if sb.pinned {
        header.push(span("  \u{f08d}".into(), t.muted, false));
    }
    frame.lines.push(line(r.x + pad, hy, header));
    frame.under.push(Rect::new(r.x, r.y + g.header_h() - s, r.w - s, s, rgb(t.border)));

    // Entries.
    let top = r.y + g.header_h();
    for (row, e) in sb.entries.iter().enumerate().skip(sb.scroll).take(g.visible_rows()) {
        let y = top + (row - sb.scroll) as f32 * row_h;
        if row == sb.selected {
            let c = if sb.focused { t.selection } else { t.elevated };
            frame.under.push(Rect::new(r.x + 4.0 * s, y, r.w - 9.0 * s, row_h, rgb(c)));
            if sb.focused {
                frame.under.push(Rect::new(r.x + 4.0 * s, y, 3.0 * s, row_h, rgb(t.accent)));
            }
        }
        let indent = pad + e.depth as f32 * 1.6 * g.char_w;
        let open = e.is_dir && sb.expanded.contains(&e.path);
        let chevron = if e.is_dir { if open { "▾ " } else { "▸ " } } else { "  " };
        let (icon, ci) = icon_for(&e.name, e.is_dir, open);
        let icon_color = if ci == 8 { t.muted } else { t.normal[ci].mix(t.bright[ci], 0.3) };
        let change = sb.change_of(&e.path, e.is_dir);
        let name_color = match change {
            Some('M') | Some('R') => t.normal[3],
            Some('?') | Some('A') => t.normal[2],
            Some('D') => t.normal[1],
            Some(_) => t.normal[3].mix(t.foreground, 0.4),
            None if e.name.starts_with('.') => t.muted,
            None => t.foreground.mix(t.muted, 0.15),
        };
        let mut spans = vec![
            span(chevron.into(), t.muted, false),
            span(format!("{icon} "), icon_color, false),
            span(e.name.clone(), name_color, row == sb.selected),
        ];
        if let Some(c) = change.filter(|c| *c != '•') {
            spans.push(span(format!("  {c}"), name_color.mix(t.background, 0.25), false));
        }
        frame.lines.push(line(r.x + indent, y, spans));
    }
    if sb.entries.is_empty() {
        frame.lines.push(line(r.x + pad, top, vec![span("(empty folder)".into(), t.muted, false)]));
    }

    // Footer hints.
    let fy = r.bottom() - row_h * 1.25;
    frame.under.push(Rect::new(r.x, fy - 2.0 * s, r.w - s, s, rgb(t.border)));
    let hint = if sb.focused { "↩ open  ⌥↩ nvim  ␣ peek  ⇧↩ paste" } else { "click to browse  ·  ⌘B hide" };
    frame.lines.push(line(r.x + pad, fy, vec![span(hint.into(), t.muted.mix(t.background, 0.15), false)]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_and_walk() {
        let dir = std::env::temp_dir().join("kraken-sidebar-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src/inner")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
        for f in ["README.md", "src/main.rs", "src/inner/x.rs", "node_modules/pkg/i.js"] {
            std::fs::write(dir.join(f), "").unwrap();
        }
        let mut sb = Sidebar::new(dir.clone());
        // Folders first: node_modules, src, then README.md.
        let names: Vec<_> = sb.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["node_modules", "src", "README.md"]);
        sb.selected = 1;
        assert!(sb.toggle());
        assert_eq!(sb.entries.len(), 5); // src expanded: inner/, main.rs
        let files = walk_files(&dir, 100);
        assert_eq!(files, vec!["README.md", "src/main.rs", "src/inner/x.rs"]);
    }

    #[test]
    fn git_marks_bubble_up_to_folders() {
        let mut sb = Sidebar::new(std::env::temp_dir());
        sb.git_root = Some(PathBuf::from("/repo"));
        sb.changes.insert("src/a.rs".into(), 'M');
        assert_eq!(sb.change_of(Path::new("/repo/src/a.rs"), false), Some('M'));
        assert_eq!(sb.change_of(Path::new("/repo/src"), true), Some('•'));
        assert_eq!(sb.change_of(Path::new("/repo/docs"), true), None);
    }
}
