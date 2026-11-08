//! ⌘F search: find text (or a regex) in a pane's screen and history.

use kraken_config::{Rgb, Theme};
use kraken_mux::layout::{PaneId, R};
use kraken_render::{FontKind, Frame, Rect, Span, TextLine};
use kraken_term::alacritty_terminal::event::EventListener;
use kraken_term::alacritty_terminal::grid::Dimensions;
use kraken_term::alacritty_terminal::index::{Column, Direction, Point};
use kraken_term::alacritty_terminal::term::Term;
use kraken_term::alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use kraken_term::view::Layout;

/// More matches than this are counted as "1000+".
const MAX_MATCHES: usize = 1000;

pub struct SearchState {
    pub pane: PaneId,
    pub query: String,
    /// Off: the query is literal text. On: it's a regular expression.
    pub regex_mode: bool,
    pub matches: Vec<Match>,
    pub current: Option<usize>,
    /// Needs recomputing (query changed or new output arrived).
    pub dirty: bool,
    /// Scroll to the current match after the next recompute.
    pub jump: bool,
    pub invalid: bool,
}

/// Escape regex syntax so a plain-text query matches literally.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if "\\.+*?()|[]{}^$#&-~".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

impl SearchState {
    pub fn new(pane: PaneId) -> Self {
        SearchState {
            pane,
            query: String::new(),
            regex_mode: false,
            matches: Vec::new(),
            current: None,
            dirty: false,
            jump: false,
            invalid: false,
        }
    }

    pub fn edited(&mut self) {
        self.dirty = true;
        self.jump = true;
        self.current = None;
    }

    /// Find all matches. Keeps the current match when it still exists;
    /// otherwise picks the one nearest the bottom of the screen.
    pub fn recompute<T: EventListener>(&mut self, term: &mut Term<T>) {
        self.dirty = false;
        let previous = self.current.and_then(|i| self.matches.get(i)).map(|m| *m.start());
        self.matches.clear();
        self.invalid = false;
        if self.query.is_empty() {
            self.current = None;
            return;
        }
        let pattern = if self.regex_mode { self.query.clone() } else { escape(&self.query) };
        let Ok(mut regex) = RegexSearch::new(&pattern) else {
            self.invalid = true;
            self.current = None;
            return;
        };
        let start = Point::new(term.topmost_line(), Column(0));
        let end = Point::new(term.bottommost_line(), term.last_column());
        self.matches.extend(RegexIter::new(start, end, Direction::Right, term, &mut regex).take(MAX_MATCHES + 1));

        self.current = previous
            .and_then(|p| self.matches.iter().position(|m| *m.start() == p))
            .or_else(|| {
                // Nearest match at or above the bottom of what's on screen.
                let bottom = term.screen_lines() as i32 - 1 - term.grid().display_offset() as i32;
                self.matches.iter().rposition(|m| m.start().line.0 <= bottom)
            })
            .or_else(|| (!self.matches.is_empty()).then_some(0));
        if self.jump {
            self.jump = false;
            self.scroll_to_current(term);
        }
    }

    /// Move to the next (`up = true`: older) match and scroll to it.
    pub fn step<T: EventListener>(&mut self, term: &mut Term<T>, up: bool) {
        let n = self.matches.len().min(MAX_MATCHES);
        if n == 0 {
            return;
        }
        let cur = self.current.unwrap_or(0);
        self.current = Some(if up { (cur + n - 1) % n } else { (cur + 1) % n });
        self.scroll_to_current(term);
    }

    fn scroll_to_current<T: EventListener>(&self, term: &mut Term<T>) {
        if let Some(m) = self.current.and_then(|i| self.matches.get(i)) {
            term.scroll_to_point(*m.start());
        }
    }

    /// Highlight rectangles for matches on screen (drawn under the text).
    pub fn highlights<T>(&self, term: &Term<T>, l: &Layout, t: &Theme, out: &mut Vec<Rect>) {
        let offset = term.grid().display_offset() as i32;
        let rows = term.screen_lines() as i32;
        let last_col = term.columns().saturating_sub(1);
        let top = -offset;
        let bottom = rows - 1 - offset;
        let other = t.normal[3].mix(t.background, 0.62);
        let current = t.ssh.mix(t.background, 0.15);
        for (i, m) in self.matches.iter().enumerate().take(MAX_MATCHES) {
            let (s, e) = (m.start(), m.end());
            if e.line.0 < top || s.line.0 > bottom {
                continue;
            }
            let color = if Some(i) == self.current { current } else { other };
            for line in s.line.0.max(top)..=e.line.0.min(bottom) {
                let c0 = if line == s.line.0 { s.column.0 } else { 0 };
                let c1 = if line == e.line.0 { e.column.0 } else { last_col };
                let row = (line + offset) as f32;
                out.push(Rect::new(
                    l.x + c0 as f32 * l.cell_w,
                    l.y + row * l.cell_h,
                    (c1 + 1 - c0) as f32 * l.cell_w,
                    l.cell_h,
                    [color.0, color.1, color.2],
                ));
            }
        }
    }

    pub fn count_label(&self) -> String {
        if self.invalid {
            "invalid regex".into()
        } else if self.query.is_empty() {
            String::new()
        } else if self.matches.is_empty() {
            "no matches".into()
        } else {
            let n = self.matches.len();
            let total = if n > MAX_MATCHES { format!("{MAX_MATCHES}+") } else { n.to_string() };
            format!("{}/{total}", self.current.map_or(0, |c| c + 1))
        }
    }
}

pub struct BarGeom {
    pub pane: R,
    pub scale: f32,
    pub char_w: f32,
    pub font_px: f32,
}

/// The search box, pinned to the top-right corner of the pane.
pub fn draw(frame: &mut Frame, t: &Theme, g: &BarGeom, st: &SearchState, p: f32) {
    let s = g.scale;
    let row_h = (g.font_px * 2.1).round();
    let cells = 44.0;
    let w = (cells * g.char_w + 24.0 * s).min(g.pane.w - 16.0 * s);
    let x = g.pane.x + g.pane.w - w - 10.0 * s;
    let y = g.pane.y + 8.0 * s - (1.0 - p) * 10.0 * s;
    let a = (255.0 * p) as u8;
    let e = t.elevated;
    let border = if st.invalid || (!st.query.is_empty() && st.matches.is_empty()) { t.normal[1] } else { t.accent };
    frame.overlay_rects.push(Rect { x: x - s, y: y - s, w: w + 2.0 * s, h: row_h + 2.0 * s, color: [border.0, border.1, border.2, a] });
    frame.overlay_rects.push(Rect { x, y, w, h: row_h, color: [e.0, e.1, e.2, a] });

    let fade = |c: Rgb| e.mix(c, p);
    let span = |text: String, c: Rgb| Span {
        text,
        color: {
            let c = fade(c);
            [c.0, c.1, c.2]
        },
        bold: false,
        italic: false,
    };
    let mut spans = vec![span("\u{f002}  ".into(), t.accent)];
    if st.query.is_empty() {
        spans.push(span("Find in history…".into(), t.muted));
    } else {
        spans.push(span(st.query.clone(), t.foreground));
    }
    spans.push(span("▏".into(), t.cursor));

    // Right side: count, regex toggle, hints.
    let count = st.count_label();
    let regex = if st.regex_mode { ".*" } else { "ab" };
    let right = format!("{count}  {regex}  ↑↓");
    let right_x = x + w - (right.chars().count() as f32 + 1.5) * g.char_w;
    let clip_left = [x as i32, y as i32, (right_x - g.char_w) as i32, (y + row_h) as i32];
    let line = |lx: f32, spans: Vec<Span>, clip: [i32; 4]| TextLine {
        x: lx,
        y: y + (row_h - g.font_px * 1.3) / 2.0,
        font: FontKind::Mono,
        size: g.font_px,
        line_height: g.font_px * 1.3,
        spans,
        clip: Some(clip),
        cell_width: Some(g.char_w),
    };
    frame.overlay_lines.push(line(x + 10.0 * s, spans, clip_left));
    let count_color = if st.invalid || (!st.query.is_empty() && st.matches.is_empty()) { t.normal[1] } else { t.muted };
    let regex_color = if st.regex_mode { t.accent } else { t.muted.mix(e, 0.4) };
    frame.overlay_lines.push(line(
        right_x,
        vec![span(format!("{count}  "), count_color), span(regex.into(), regex_color), span("  ↑↓".into(), t.muted)],
        [x as i32, y as i32, (x + w) as i32, (y + row_h) as i32],
    ));
}
