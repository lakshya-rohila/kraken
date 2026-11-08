//! Turns the terminal grid into renderer primitives (rects + styled text).

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor};
use kraken_config::{Rgb, Theme};
use kraken_render::{FontKind, Frame, Rect, Span, TextLine};

/// Where and how big the grid is drawn, in physical pixels.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub x: f32,
    pub y: f32,
    pub cell_w: f32,
    pub cell_h: f32,
    pub font_px: f32,
    pub focused: bool,
    /// The blink phase: `false` hides a blinking cursor for this frame.
    pub cursor_visible: bool,
}

impl Layout {
    /// Convert a pixel position into a viewport cell (row, col), clamped to the grid.
    pub fn cell_at(&self, px: f64, py: f64, cols: usize, rows: usize) -> (usize, usize, bool) {
        let fx = ((px as f32 - self.x) / self.cell_w).max(0.0);
        let fy = ((py as f32 - self.y) / self.cell_h).max(0.0);
        let col = (fx as usize).min(cols.saturating_sub(1));
        let row = (fy as usize).min(rows.saturating_sub(1));
        let right_half = fx.fract() > 0.5;
        (row, col, right_half)
    }
}

fn rgb(c: Rgb) -> [u8; 3] {
    [c.0, c.1, c.2]
}

fn named_default(n: NamedColor, t: &Theme) -> Rgb {
    let i = n as usize;
    match n {
        NamedColor::Foreground | NamedColor::BrightForeground => t.foreground,
        NamedColor::Background => t.background,
        NamedColor::Cursor => t.cursor,
        NamedColor::DimForeground => t.foreground.mix(t.background, 0.34),
        _ if i < 16 => t.indexed(i as u8),
        // DimBlack..DimWhite follow Cursor in the enum.
        _ => {
            let base = i.saturating_sub(NamedColor::DimBlack as usize).min(7);
            t.normal[base].mix(t.background, 0.34)
        }
    }
}

pub fn resolve(c: Color, colors: &Colors, t: &Theme) -> Rgb {
    let conv = |r: alacritty_terminal::vte::ansi::Rgb| Rgb(r.r, r.g, r.b);
    match c {
        Color::Spec(r) => conv(r),
        Color::Indexed(i) => colors[i as usize].map(conv).unwrap_or_else(|| t.indexed(i)),
        Color::Named(n) => colors[n].map(conv).unwrap_or_else(|| named_default(n, t)),
    }
}

/// Append the terminal's cells, selection and cursor to `frame`.
/// Where an image placeholder sits on screen (top-left of its cell).
#[derive(Debug, Clone, Copy)]
pub struct ImageMark {
    pub slot: usize,
    /// Which row of the image this placeholder starts.
    pub row: u32,
    pub x: f32,
    pub y: f32,
}

/// Returns the cursor's cell rectangle when it is visible, so callers can
/// decorate it. Image placeholders found on screen are added to `marks`.
pub fn build<T: alacritty_terminal::event::EventListener>(
    term: &Term<T>,
    theme: &Theme,
    l: &Layout,
    frame: &mut Frame,
    marks: &mut Vec<ImageMark>,
) -> Option<Rect> {
    let content = term.renderable_content();
    let offset = content.display_offset as i32;
    let colors = content.colors;
    let rows = term.screen_lines();
    let cols = term.columns();
    let selection = content.selection;
    let cursor = content.cursor;
    let cursor_row = cursor.point.line.0 + offset;
    let block_cursor = l.focused && l.cursor_visible && cursor.shape == CursorShape::Block;

    let thickness = (l.font_px / 14.0).max(1.0).round();
    let underline_y = |row: usize| l.y + row as f32 * l.cell_h + (l.cell_h + l.font_px) * 0.5;

    // Per-row state.
    let mut spans: Vec<Span> = Vec::new();
    let mut bg_run: Option<(usize, Rgb)> = None;
    let mut current_row = 0usize;
    let mut row_rects: Vec<Rect> = Vec::new();

    let flush_row = |row: usize, spans: &mut Vec<Span>, bg_run: &mut Option<(usize, Rgb)>, end_col: usize, frame: &mut Frame| {
        if let Some((start, c)) = bg_run.take() {
            frame.under.push(Rect::new(
                l.x + start as f32 * l.cell_w,
                l.y + row as f32 * l.cell_h,
                (end_col - start) as f32 * l.cell_w,
                l.cell_h,
                rgb(c),
            ));
        }
        frame.lines.push(TextLine {
            x: l.x,
            y: l.y + row as f32 * l.cell_h,
            font: FontKind::Mono,
            size: l.font_px,
            line_height: l.cell_h,
            spans: std::mem::take(spans),
            clip: None,
            cell_width: Some(l.cell_w),
        });
    };

    for indexed in content.display_iter {
        let row = (indexed.point.line.0 + offset) as usize;
        let col = indexed.point.column.0;
        if row >= rows {
            break;
        }
        while row > current_row {
            flush_row(current_row, &mut spans, &mut bg_run, cols, frame);
            current_row += 1;
        }
        let cell = indexed.cell;
        let flags = cell.flags;
        let screen_row = row;

        let mut fg = resolve(cell.fg, colors, theme);
        let mut bg = resolve(cell.bg, colors, theme);
        let mut bg_default = cell.bg == Color::Named(NamedColor::Background);
        if flags.contains(Flags::DIM) {
            fg = fg.mix(theme.background, 0.34);
        }
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
            bg_default = false;
        }
        if flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        if selection.is_some_and(|s| s.contains(indexed.point)) {
            bg = theme.selection;
            bg_default = false;
        }
        if block_cursor && row as i32 == cursor_row && col == cursor.point.column.0 {
            bg = theme.cursor;
            fg = theme.cursor_text;
            bg_default = false;
        }

        // Background runs.
        let want = if bg_default { None } else { Some(bg) };
        match (bg_run, want) {
            (Some((_, c)), Some(w)) if c == w => {}
            (run, w) => {
                if let Some((start, c)) = run {
                    frame.under.push(Rect::new(
                        l.x + start as f32 * l.cell_w,
                        l.y + row as f32 * l.cell_h,
                        (col - start) as f32 * l.cell_w,
                        l.cell_h,
                        rgb(c),
                    ));
                }
                bg_run = w.map(|w| (col, w));
            }
        }

        // Decorations.
        let cx = l.x + col as f32 * l.cell_w;
        if flags.intersects(Flags::ALL_UNDERLINES) {
            // SGR 58 sets the underline colour separately (LSP diagnostics use it).
            let color = cell.underline_color().map_or(fg, |c| resolve(c, colors, theme));
            let stroke = Stroke { x: cx, w: l.cell_w, y: underline_y(row), t: thickness, cell_w: l.cell_w };
            underline(Underline::from_flags(flags), stroke, rgb(color), &mut row_rects);
        }
        if flags.contains(Flags::STRIKEOUT) {
            let y = l.y + row as f32 * l.cell_h + l.cell_h * 0.5;
            row_rects.push(Rect::new(cx, y, l.cell_w, thickness, rgb(fg)));
        }

        // Text.
        if flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
            continue;
        }
        let bold = flags.contains(Flags::BOLD);
        let italic = flags.contains(Flags::ITALIC);
        let color = rgb(fg);
        let span = match spans.last_mut() {
            Some(s) if s.color == color && s.bold == bold && s.italic == italic => s,
            _ => {
                spans.push(Span { text: String::new(), color, bold, italic });
                spans.last_mut().expect("just pushed")
            }
        };
        let ch = match crate::images::decode_placeholder(cell.c) {
            Some((slot, row)) => {
                marks.push(ImageMark { slot, row, x: cx, y: l.y + screen_row as f32 * l.cell_h });
                ' '
            }
            // alacritty keeps a literal tab in the cell where it started (so
            // copying preserves it); drawn as-is, the shaper would expand it to
            // its own tab width and push the rest of the line off the grid.
            None if cell.c == '\0' || cell.c == '\t' => ' ',
            None => cell.c,
        };
        span.text.push(ch);
        if let Some(zw) = cell.zerowidth() {
            span.text.extend(zw);
        }
    }
    while current_row < rows {
        flush_row(current_row, &mut spans, &mut bg_run, cols, frame);
        current_row += 1;
    }
    frame.over.extend(row_rects);

    // Non-block cursor shapes are drawn on top of the text.
    if !(0..rows as i32).contains(&cursor_row) || cursor.shape == CursorShape::Hidden {
        return None;
    }
    if l.focused && !l.cursor_visible {
        // Blinked off. Still report where it is, for the input method's window.
        let x = l.x + cursor.point.column.0 as f32 * l.cell_w;
        return Some(Rect::new(x, l.y + cursor_row as f32 * l.cell_h, l.cell_w, l.cell_h, rgb(theme.cursor)));
    }
    {
        let x = l.x + cursor.point.column.0 as f32 * l.cell_w;
        let y = l.y + cursor_row as f32 * l.cell_h;
        let wide = term.grid()[Point::new(cursor.point.line, cursor.point.column)].flags.contains(Flags::WIDE_CHAR);
        let w = if wide { l.cell_w * 2.0 } else { l.cell_w };
        let c = rgb(theme.cursor);
        let t = (thickness * 2.0).max(2.0);
        match cursor.shape {
            CursorShape::Block if l.focused => {}
            CursorShape::Beam => frame.over.push(Rect::new(x, y, t, l.cell_h, c)),
            CursorShape::Underline => frame.over.push(Rect::new(x, y + l.cell_h - t, w, t, c)),
            _ => {
                // Hollow block (also used when the window is unfocused).
                let b = thickness;
                frame.over.push(Rect::new(x, y, w, b, c));
                frame.over.push(Rect::new(x, y + l.cell_h - b, w, b, c));
                frame.over.push(Rect::new(x, y, b, l.cell_h, c));
                frame.over.push(Rect::new(x + w - b, y, b, l.cell_h, c));
            }
        }
        Some(Rect::new(x, y, w, l.cell_h, c))
    }
}

/// One run of same-coloured cells on a row, for miniature views.
#[derive(Debug, Clone, Copy)]
pub struct MiniRun {
    pub row: u16,
    pub col: u16,
    pub len: u16,
    pub color: Rgb,
    /// A background run (full cell height) rather than text.
    pub bg: bool,
}

/// The visible screen as coloured runs (the tab overview's thumbnails).
/// Returns (columns, rows, runs); stops after `max_runs` to bound the cost.
pub fn minimap<T: alacritty_terminal::event::EventListener>(
    term: &Term<T>,
    theme: &Theme,
    max_runs: usize,
) -> (usize, usize, Vec<MiniRun>) {
    let content = term.renderable_content();
    let offset = content.display_offset as i32;
    let colors = content.colors;
    let (rows, cols) = (term.screen_lines(), term.columns());
    let mut runs: Vec<MiniRun> = Vec::new();
    let mut bg_run: Option<MiniRun> = None;
    let mut text_run: Option<MiniRun> = None;
    for indexed in content.display_iter {
        let row = indexed.point.line.0 + offset;
        if row < 0 || row as usize >= rows || runs.len() >= max_runs {
            break;
        }
        let (row, col) = (row as u16, indexed.point.column.0 as u16);
        let cell = indexed.cell;
        let mut fg = resolve(cell.fg, colors, theme);
        let mut bg = resolve(cell.bg, colors, theme);
        let mut bg_default = cell.bg == Color::Named(NamedColor::Background);
        if cell.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
            bg_default = false;
        }
        if cell.flags.contains(Flags::DIM) {
            fg = fg.mix(theme.background, 0.34);
        }
        // Background runs.
        let extend = |run: &mut Option<MiniRun>, color: Rgb, is_bg: bool, runs: &mut Vec<MiniRun>| match run {
            Some(r) if r.row == row && r.col + r.len == col && r.color == color => r.len += 1,
            _ => {
                if let Some(r) = run.take() {
                    runs.push(r);
                }
                *run = Some(MiniRun { row, col, len: 1, color, bg: is_bg });
            }
        };
        if bg_default {
            if let Some(r) = bg_run.take() {
                runs.push(r);
            }
        } else {
            extend(&mut bg_run, bg, true, &mut runs);
        }
        let blank = matches!(cell.c, ' ' | '\0' | '\t') || cell.flags.intersects(Flags::HIDDEN);
        if blank {
            if let Some(r) = text_run.take() {
                runs.push(r);
            }
        } else {
            extend(&mut text_run, fg, false, &mut runs);
        }
    }
    runs.extend(bg_run);
    runs.extend(text_run);
    // Backgrounds first, so text bars draw on top of them.
    runs.sort_by_key(|r| !r.bg);
    (cols, rows, runs)
}

/// Underline styles (SGR 4, 4:2 … 4:5, 21).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Underline {
    Single,
    Double,
    Curly,
    Dotted,
    Dashed,
}

impl Underline {
    fn from_flags(flags: Flags) -> Underline {
        if flags.contains(Flags::UNDERCURL) {
            Underline::Curly
        } else if flags.contains(Flags::DOUBLE_UNDERLINE) {
            Underline::Double
        } else if flags.contains(Flags::DOTTED_UNDERLINE) {
            Underline::Dotted
        } else if flags.contains(Flags::DASHED_UNDERLINE) {
            Underline::Dashed
        } else {
            Underline::Single
        }
    }
}

/// Where one cell's underline goes: from `x`, `w` wide, at baseline `y`,
/// `t` thick, on a grid of `cell_w`-wide cells.
#[derive(Debug, Clone, Copy)]
pub struct Stroke {
    pub x: f32,
    pub w: f32,
    pub y: f32,
    pub t: f32,
    pub cell_w: f32,
}

/// Rectangles for one cell's underline. Patterns are phased on the absolute
/// x position (not the cell), so dots, dashes and waves continue seamlessly
/// across neighbouring cells.
pub fn underline(kind: Underline, span: Stroke, color: [u8; 3], out: &mut Vec<Rect>) {
    let Stroke { x, w, y, t, cell_w } = span;
    match kind {
        Underline::Single => out.push(Rect::new(x, y, w, t, color)),
        Underline::Double => {
            out.push(Rect::new(x, y - t, w, t, color));
            out.push(Rect::new(x, y + t, w, t, color));
        }
        Underline::Dotted => {
            let period = (t * 2.0).max(2.0);
            let mut dx = (period - x.rem_euclid(period)) % period;
            while dx < w {
                out.push(Rect::new(x + dx, y, t.min(w - dx), t, color));
                dx += period;
            }
        }
        Underline::Dashed => {
            // Two dashes per cell, each 60% of its half.
            let period = (cell_w / 2.0).max(t * 3.0);
            let dash = period * 0.6;
            let mut start = x - x.rem_euclid(period);
            while start < x + w {
                let a = start.max(x);
                let b = (start + dash).min(x + w);
                if b > a {
                    out.push(Rect::new(a, y, b - a, t, color));
                }
                start += period;
            }
        }
        Underline::Curly => {
            // A sine wave drawn as small squares: one wavelength per cell.
            let amp = (t * 1.5).max(1.5);
            let step = t.max(1.0);
            let mut dx = 0.0;
            while dx < w {
                let phase = (x + dx) / cell_w * std::f32::consts::TAU;
                // Whole pixel rows keep the wave crisp and symmetric.
                let wy = (y + amp * phase.sin()).round();
                out.push(Rect::new(x + dx, wy, step.min(w - dx), t, color));
                dx += step;
            }
        }
    }
}

/// Convert a viewport row/col into a grid point (accounting for scrollback).
pub fn viewport_to_point(display_offset: usize, row: usize, col: usize) -> Point {
    Point::new(Line(row as i32 - display_offset as i32), Column(col))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimap_merges_runs_by_colour() {
        use alacritty_terminal::event::VoidListener;
        use alacritty_terminal::vte::ansi::Processor;
        struct Size;
        impl Dimensions for Size {
            fn total_lines(&self) -> usize {
                4
            }
            fn screen_lines(&self) -> usize {
                4
            }
            fn columns(&self) -> usize {
                20
            }
        }
        let mut term = Term::new(alacritty_terminal::term::Config::default(), &Size, VoidListener);
        let mut p: Processor = Processor::new();
        p.advance(&mut term, b"hello world\r\n\x1b[31mred\x1b[0m \x1b[44mbg\x1b[0m");
        let (cols, rows, runs) = minimap(&term, &Theme::default(), 1000);
        assert_eq!((cols, rows), (20, 4));
        let text: Vec<_> = runs.iter().filter(|r| !r.bg).map(|r| (r.row, r.col, r.len)).collect();
        assert_eq!(text, vec![(0, 0, 5), (0, 6, 5), (1, 0, 3), (1, 4, 2)]);
        assert!(runs.iter().any(|r| r.bg && r.row == 1 && r.col == 4 && r.len == 2));
        assert!(runs.first().is_some_and(|r| r.bg), "backgrounds come first");
    }

    fn rects(kind: Underline, x: f32) -> Vec<Rect> {
        let mut v = Vec::new();
        underline(kind, Stroke { x, w: 10.0, y: 100.0, t: 1.0, cell_w: 10.0 }, [255, 0, 0], &mut v);
        v
    }

    #[test]
    fn every_style_stays_inside_its_cell() {
        for kind in [Underline::Single, Underline::Double, Underline::Curly, Underline::Dotted, Underline::Dashed] {
            let v = rects(kind, 30.0);
            assert!(!v.is_empty(), "{kind:?}");
            for r in &v {
                assert!(r.x >= 30.0 - 1e-3 && r.x + r.w <= 40.0 + 1e-3, "{kind:?} {r:?}");
                assert!((r.y - 100.0).abs() <= 3.0, "{kind:?} drifts vertically: {r:?}");
            }
        }
    }

    #[test]
    fn styles_are_visually_distinct() {
        let count = |k| rects(k, 0.0).len();
        assert_eq!(count(Underline::Single), 1);
        assert_eq!(count(Underline::Double), 2);
        assert_eq!(count(Underline::Dashed), 2);
        assert!(count(Underline::Dotted) >= 4);
        let curly = rects(Underline::Curly, 0.0);
        let (lo, hi) = curly.iter().fold((f32::MAX, f32::MIN), |(lo, hi), r| (lo.min(r.y), hi.max(r.y)));
        assert!(hi - lo >= 2.0, "undercurl should actually wave");
    }

    #[test]
    fn dots_continue_across_cell_boundaries() {
        // The dot pattern in cell 2 must be where cell 1's pattern would continue.
        let mut both = Vec::new();
        underline(Underline::Dotted, Stroke { x: 0.0, w: 20.0, y: 0.0, t: 1.0, cell_w: 10.0 }, [0; 3], &mut both);
        let mut split = Vec::new();
        underline(Underline::Dotted, Stroke { x: 0.0, w: 10.0, y: 0.0, t: 1.0, cell_w: 10.0 }, [0; 3], &mut split);
        underline(Underline::Dotted, Stroke { x: 10.0, w: 10.0, y: 0.0, t: 1.0, cell_w: 10.0 }, [0; 3], &mut split);
        let xs = |v: &[Rect]| v.iter().map(|r| r.x).collect::<Vec<_>>();
        assert_eq!(xs(&both), xs(&split));
    }
}
