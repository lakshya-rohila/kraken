//! ⌘⇧P command palette: type a few letters, run any command.

use crate::actions::{self, Action, Command};
use kraken_config::{Rgb, Theme};
use kraken_mux::layout::R;
use kraken_render::{FontKind, Frame, Rect, Span, TextLine};

const VISIBLE_ROWS: usize = 9;

pub struct PaletteState {
    pub query: String,
    pub selected: usize,
    commands: Vec<Command>,
    /// (command index, matched character positions), best first.
    results: Vec<(usize, Vec<usize>)>,
    pub placeholder: String,
    /// What the items are called in the footer ("commands", "files").
    pub noun: &'static str,
    /// Listing files: ⌥↩ / ⇧↩ do file things.
    pub files: bool,
}

impl PaletteState {
    pub fn new(keymap: &crate::keys::Keymap) -> Self {
        Self::with_items(actions::commands(keymap), "Type a command…".into(), "commands", false)
    }

    /// The ⌘P file finder: `files` are paths relative to `root`.
    pub fn files(root: &std::path::Path, files: Vec<String>) -> Self {
        let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let items = files
            .into_iter()
            .map(|rel| Command { action: Action::OpenFile(root.join(&rel), None, None), name: rel, keys: String::new() })
            .collect();
        Self::with_items(items, format!("Find a file in {name}…   (⌥↩ nvim · ⇧↩ paste path)"), "files", true)
    }

    fn with_items(commands: Vec<Command>, placeholder: String, noun: &'static str, files: bool) -> Self {
        let mut p = PaletteState { query: String::new(), selected: 0, commands, results: Vec::new(), placeholder, noun, files };
        p.refilter();
        p
    }

    pub fn refilter(&mut self) {
        let mut scored: Vec<(i32, usize, Vec<usize>)> = self
            .commands
            .iter()
            .enumerate()
            .filter_map(|(i, c)| actions::fuzzy(&self.query, &c.name).map(|(s, pos)| (s, i, pos)))
            .collect();
        if !self.query.is_empty() {
            // Stable: equal scores keep the list's natural order.
            scored.sort_by_key(|a| std::cmp::Reverse(a.0));
        }
        self.results = scored.into_iter().map(|(_, i, pos)| (i, pos)).collect();
        self.selected = 0;
    }

    pub fn move_by(&mut self, delta: i32) {
        let n = self.results.len() as i32;
        if n > 0 {
            self.selected = (self.selected as i32 + delta).rem_euclid(n) as usize;
        }
    }

    pub fn selected_action(&self) -> Option<Action> {
        self.results.get(self.selected).map(|(i, _)| self.commands[*i].action.clone())
    }
}

pub struct PaletteGeom {
    pub window: R,
    pub scale: f32,
    pub char_w: f32,
    pub font_px: f32,
}

pub fn draw(frame: &mut Frame, t: &Theme, g: &PaletteGeom, st: &PaletteState, p: f32) {
    let s = g.scale;
    let win = g.window;
    let row_h = (g.font_px * 2.0).round();
    let w = (640.0 * s).min(win.w - 60.0 * s);
    let rows = st.results.len().min(VISIBLE_ROWS);
    let h = row_h * (1.6 + rows.max(1) as f32 + 1.1);
    let x = win.x + (win.w - w) / 2.0;
    let y = win.y + win.h * 0.12 - (1.0 - p) * 14.0 * s;
    let a = (255.0 * p) as u8;
    let (bg, e) = (t.background, t.elevated);

    frame.overlay_rects.push(Rect { x: win.x, y: win.y, w: win.w, h: win.h, color: [bg.0, bg.1, bg.2, (110.0 * p) as u8] });
    let border = t.accent.mix(t.border, 0.3);
    frame.overlay_rects.push(Rect { x: x - s, y: y - s, w: w + 2.0 * s, h: h + 2.0 * s, color: [border.0, border.1, border.2, a] });
    frame.overlay_rects.push(Rect { x, y, w, h, color: [e.0, e.1, e.2, a] });

    let fade = |c: Rgb| e.mix(c, p);
    let span = |text: String, c: Rgb, bold: bool| {
        let c = fade(c);
        Span { text, color: [c.0, c.1, c.2], bold, italic: false }
    };
    let clip = Some([x as i32, y as i32, (x + w) as i32, (y + h) as i32]);
    let text = |lx: f32, ly: f32, spans: Vec<Span>| TextLine {
        x: lx,
        y: ly + (row_h - g.font_px * 1.3) / 2.0,
        font: FontKind::Mono,
        size: g.font_px,
        line_height: g.font_px * 1.3,
        spans,
        clip,
        cell_width: Some(g.char_w),
    };
    let pad = 14.0 * s;

    // Input row.
    let input_h = row_h * 1.4;
    let mut input = vec![span("❯ ".into(), t.accent, true)];
    if st.query.is_empty() {
        input.push(span(st.placeholder.clone(), t.muted, false));
    } else {
        input.push(span(st.query.clone(), t.foreground, false));
    }
    input.push(span("▏".into(), t.cursor, false));
    frame.overlay_lines.push(text(x + pad, y + (input_h - row_h) / 2.0, input));
    let line = t.border;
    frame.overlay_rects.push(Rect { x, y: y + input_h, w, h: s, color: [line.0, line.1, line.2, a] });

    // Results, scrolled so the selection stays visible.
    let first = st.selected.saturating_sub(VISIBLE_ROWS - 1);
    let list_y = y + input_h + row_h * 0.2;
    if st.results.is_empty() {
        frame.overlay_lines.push(text(x + pad, list_y, vec![span("No matching commands".into(), t.muted, false)]));
    }
    for (row, (ci, positions)) in st.results.iter().enumerate().skip(first).take(VISIBLE_ROWS) {
        let ry = list_y + (row - first) as f32 * row_h;
        let cmd = &st.commands[*ci];
        let selected = row == st.selected;
        if selected {
            let sel = t.selection;
            frame.overlay_rects.push(Rect { x: x + 6.0 * s, y: ry, w: w - 12.0 * s, h: row_h, color: [sel.0, sel.1, sel.2, a] });
            let ac = t.accent;
            frame.overlay_rects.push(Rect { x: x + 6.0 * s, y: ry, w: 3.0 * s, h: row_h, color: [ac.0, ac.1, ac.2, a] });
        }
        // Name, with the matched letters lit up.
        let mut spans: Vec<Span> = Vec::new();
        for (i, ch) in cmd.name.chars().enumerate() {
            let hit = positions.contains(&i);
            let c = if hit {
                t.accent.mix(Rgb(255, 255, 255), 0.25)
            } else if selected {
                t.foreground
            } else {
                t.foreground.mix(t.muted, 0.35)
            };
            match spans.last_mut() {
                Some(last)
                    if last.bold == hit
                        && last.color == {
                            let f = fade(c);
                            [f.0, f.1, f.2]
                        } =>
                {
                    last.text.push(ch)
                }
                _ => spans.push(span(ch.to_string(), c, hit)),
            }
        }
        frame.overlay_lines.push(text(x + pad + 4.0 * s, ry, spans));
        if !cmd.keys.is_empty() {
            let kw = cmd.keys.chars().count() as f32 * g.char_w;
            let kx = x + w - pad - kw - 8.0 * s;
            let cap = t.border.mix(e, 0.2);
            frame.overlay_rects.push(Rect {
                x: kx - 6.0 * s,
                y: ry + row_h * 0.16,
                w: kw + 12.0 * s,
                h: row_h * 0.68,
                color: [cap.0, cap.1, cap.2, a],
            });
            frame.overlay_lines.push(text(kx, ry, vec![span(cmd.keys.clone(), t.muted.mix(t.foreground, 0.3), false)]));
        }
    }

    // Footer.
    let fy = y + h - row_h * 1.05;
    let verb = if st.files { "open" } else { "run" };
    let footer = format!("↑↓ move   ↩ {verb}   esc close   {} {}", st.results.len(), st.noun);
    frame.overlay_lines.push(text(x + pad, fy, vec![span(footer, t.muted.mix(e, 0.2), false)]));
}
