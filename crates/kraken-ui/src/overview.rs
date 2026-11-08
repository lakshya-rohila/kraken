//! Tab overview (⌘⇧\): every tab as a live miniature in a grid.
//!
//! Each pane is drawn as a mini-map rather than tiny text: every run of
//! characters becomes a thin coloured bar, which reads as "that's my code /
//! my log / my agent" at a glance and costs a few thousand rectangles instead
//! of shaping thousands of unreadable glyphs.

use kraken_config::{Rgb, Theme};
use kraken_mux::layout::R;
use kraken_render::{FontKind, Frame, Rect, Span, TextLine};

pub use kraken_term::view::MiniRun as Run;

pub struct PaneThumb {
    /// Where the pane sits in its tab, as fractions (0..1) of the tab area.
    pub at: R,
    pub cols: usize,
    pub rows: usize,
    pub runs: Vec<Run>,
    pub focused: bool,
}

pub struct TabCard {
    pub title: String,
    pub color: Option<Rgb>,
    pub working: usize,
    pub waiting: usize,
    pub active: bool,
    pub panes: Vec<PaneThumb>,
}

pub struct OverviewGeom {
    pub area: R,
    pub scale: f32,
    pub ui_px: f32,
    /// Window aspect ratio (w / h) for the thumbnails.
    pub aspect: f32,
}

/// Grid of card rectangles (thumbnail + title row) for `n` tabs.
pub fn layout(n: usize, g: &OverviewGeom) -> (Vec<R>, usize) {
    let n = n.max(1);
    let s = g.scale;
    let margin = 36.0 * s;
    let gap = 28.0 * s;
    let title_h = g.ui_px * 2.2;
    // Room at the bottom for the key hint.
    let hint_h = g.ui_px * 3.0;
    let (aw, ah) = (g.area.w - margin * 2.0, g.area.h - margin * 2.0 - hint_h);
    // Pick the column count that makes the biggest thumbnails.
    let mut best = (1usize, 0.0f32);
    for cols in 1..=n {
        let rows = n.div_ceil(cols);
        let w = ((aw - gap * (cols - 1) as f32) / cols as f32).min((ah - gap * (rows - 1) as f32) / rows as f32 * g.aspect);
        let thumb_w = w.min(((ah - gap * (rows - 1) as f32) / rows as f32 - title_h) * g.aspect);
        if thumb_w > best.1 {
            best = (cols, thumb_w);
        }
    }
    let (cols, tw) = best;
    let tw = tw.min(620.0 * s);
    let th = tw / g.aspect;
    let rows = n.div_ceil(cols);
    let grid_w = cols as f32 * tw + (cols - 1) as f32 * gap;
    let grid_h = rows as f32 * (th + title_h) + (rows - 1) as f32 * gap;
    let x0 = g.area.x + (g.area.w - grid_w) / 2.0;
    let y0 = g.area.y + (g.area.h - hint_h - grid_h) / 2.0;
    let rects = (0..n)
        .map(|i| {
            let (c, r) = (i % cols, i / cols);
            // The last row is centred when it isn't full.
            let in_row = if r == rows - 1 { n - r * cols } else { cols };
            let row_x = x0 + (grid_w - (in_row as f32 * tw + (in_row - 1) as f32 * gap)) / 2.0;
            R { x: row_x + c as f32 * (tw + gap), y: y0 + r as f32 * (th + title_h + gap), w: tw, h: th + title_h }
        })
        .collect();
    (rects, cols)
}

fn push(frame: &mut Frame, r: R, c: Rgb, a: u8) {
    frame.overlay_rects.push(Rect { x: r.x, y: r.y, w: r.w, h: r.h, color: [c.0, c.1, c.2, a] });
}

/// Draw the overview. `open` animates 0 → 1 as it appears (and back as it
/// closes). Returns each card's rectangle for clicks.
pub fn draw(frame: &mut Frame, t: &Theme, g: &OverviewGeom, cards: &[TabCard], selected: usize, open: f32) -> Vec<R> {
    let s = g.scale;
    let e = open.clamp(0.0, 1.0);
    let veil = t.background.mix(Rgb(0, 0, 0), 0.35);
    // Fully opaque once open: blending happens in linear light, so even a 93%
    // veil would leave bright text behind it visibly grey.
    push(frame, g.area, veil, (255.0 * e) as u8);
    let (rects, _) = layout(cards.len(), g);
    let title_h = g.ui_px * 2.2;
    let fade = |c: Rgb| veil.mix(c, e);
    for (i, (card, &r0)) in cards.iter().zip(&rects).enumerate() {
        // Cards settle in from slightly smaller, staggered a touch.
        let k = (e * 1.25 - i as f32 * 0.04).clamp(0.0, 1.0);
        let grow = 0.94 + 0.06 * k;
        let r = R { x: r0.x + r0.w * (1.0 - grow) / 2.0, y: r0.y + r0.h * (1.0 - grow) / 2.0, w: r0.w * grow, h: r0.h * grow };
        let thumb = R { x: r.x, y: r.y + title_h, w: r.w, h: r.h - title_h };
        let sel = i == selected;

        // Selection ring and glow.
        if sel {
            for (grow, a) in [(9.0f32, 30.0f32), (5.0, 60.0), (2.0, 255.0)] {
                let gs = grow * s;
                push(frame, R { x: thumb.x - gs, y: thumb.y - gs, w: thumb.w + gs * 2.0, h: thumb.h + gs * 2.0 }, t.accent, (a * k) as u8);
            }
        } else {
            push(frame, R { x: thumb.x - s, y: thumb.y - s, w: thumb.w + 2.0 * s, h: thumb.h + 2.0 * s }, fade(t.border), 255);
        }
        push(frame, thumb, fade(t.background), 255);

        // Panes, as mini-maps.
        for p in &card.panes {
            let pr = R { x: thumb.x + p.at.x * thumb.w, y: thumb.y + p.at.y * thumb.h, w: p.at.w * thumb.w, h: p.at.h * thumb.h };
            let pad = 3.0 * s;
            let inner = R { x: pr.x + pad, y: pr.y + pad, w: (pr.w - pad * 2.0).max(1.0), h: (pr.h - pad * 2.0).max(1.0) };
            let cw = inner.w / p.cols.max(1) as f32;
            let ch = inner.h / p.rows.max(1) as f32;
            for run in &p.runs {
                let x = inner.x + run.col as f32 * cw;
                let y = inner.y + run.row as f32 * ch;
                let (yy, hh) = if run.bg { (y, ch) } else { (y + ch * 0.22, (ch * 0.56).max(1.0)) };
                push(
                    frame,
                    R { x, y: yy, w: (run.len as f32 * cw).max(1.0), h: hh },
                    fade(run.color.mix(t.background, if run.bg { 0.0 } else { 0.12 })),
                    255,
                );
            }
            // Pane edges inside a split.
            if card.panes.len() > 1 {
                let c = if p.focused { t.accent } else { t.border };
                let b = s.max(1.0);
                push(frame, R { x: pr.x, y: pr.y, w: pr.w, h: b }, fade(c), 255);
                push(frame, R { x: pr.x, y: pr.y + pr.h - b, w: pr.w, h: b }, fade(c), 255);
                push(frame, R { x: pr.x, y: pr.y, w: b, h: pr.h }, fade(c), 255);
                push(frame, R { x: pr.x + pr.w - b, y: pr.y, w: b, h: pr.h }, fade(c), 255);
            }
        }

        // Title row: number, colour chip, name, agent state.
        let ty = r.y + (title_h - g.ui_px * 1.4) / 2.0 - 2.0 * s;
        let mut spans = Vec::new();
        let num = if i < 9 { format!("{}  ", i + 1) } else { "   ".into() };
        spans.push(Span { text: num, color: rgb(fade(t.muted)), bold: false, italic: false });
        if let Some(c) = card.color {
            spans.push(Span { text: "● ".into(), color: rgb(fade(c)), bold: false, italic: false });
        }
        let name_c = if sel { t.foreground } else { t.foreground.mix(t.muted, 0.35) };
        spans.push(Span { text: card.title.clone(), color: rgb(fade(name_c)), bold: sel || card.active, italic: false });
        if card.active {
            spans.push(Span { text: "  · current".into(), color: rgb(fade(t.muted)), bold: false, italic: false });
        }
        frame.overlay_lines.push(TextLine {
            x: r.x + 2.0 * s,
            y: ty,
            font: FontKind::Ui,
            size: g.ui_px,
            line_height: g.ui_px * 1.4,
            spans,
            clip: Some([r.x as i32, r.y as i32, (r.x + r.w * 0.72) as i32, (r.y + title_h) as i32]),
            cell_width: None,
        });
        let mut badge = Vec::new();
        if card.waiting > 0 {
            badge.push(Span { text: format!("◆ {} waiting", card.waiting), color: rgb(fade(t.ssh)), bold: true, italic: false });
        }
        if card.working > 0 {
            let sep = if badge.is_empty() { "" } else { "   " };
            badge.push(Span {
                text: format!("{sep}● {} working", card.working),
                color: rgb(fade(t.normal[2])),
                bold: false,
                italic: false,
            });
        }
        if !badge.is_empty() {
            let chars: usize = badge.iter().map(|b| b.text.chars().count()).sum();
            let bw = chars as f32 * g.ui_px * 0.6;
            frame.overlay_lines.push(TextLine {
                x: r.x + r.w - bw,
                y: ty,
                font: FontKind::Ui,
                size: g.ui_px * 0.92,
                line_height: g.ui_px * 1.4,
                spans: badge,
                clip: Some([(r.x + r.w * 0.5) as i32, r.y as i32, (r.x + r.w) as i32, (r.y + title_h) as i32]),
                cell_width: None,
            });
        }
    }
    // Hint along the bottom.
    let hint = "←↑↓→ choose  ·  ↩ open  ·  1–9 jump  ·  esc close";
    frame.overlay_lines.push(TextLine {
        x: g.area.x + (g.area.w - hint.chars().count() as f32 * g.ui_px * 0.55) / 2.0,
        y: g.area.y + g.area.h - g.ui_px * 2.6,
        font: FontKind::Ui,
        size: g.ui_px * 0.95,
        line_height: g.ui_px * 1.4,
        spans: vec![Span { text: hint.into(), color: rgb(fade(t.muted)), bold: false, italic: false }],
        clip: None,
        cell_width: None,
    });
    rects
}

fn rgb(c: Rgb) -> [u8; 3] {
    [c.0, c.1, c.2]
}

/// The card one step away in a grid of `cols` columns.
pub fn step(selected: usize, n: usize, cols: usize, dx: i32, dy: i32) -> usize {
    if n == 0 {
        return 0;
    }
    let cols = cols.max(1) as i32;
    let (c, r) = (selected as i32 % cols, selected as i32 / cols);
    let next = (r + dy) * cols + (c + dx);
    if dx != 0 {
        next.rem_euclid(n as i32) as usize
    } else if (0..n as i32).contains(&next) {
        next as usize
    } else {
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom() -> OverviewGeom {
        OverviewGeom { area: R { x: 0.0, y: 0.0, w: 2000.0, h: 1200.0 }, scale: 2.0, ui_px: 25.0, aspect: 1.6 }
    }

    #[test]
    fn cards_fit_and_do_not_overlap() {
        for n in 1..=9 {
            let (rects, _) = layout(n, &geom());
            assert_eq!(rects.len(), n);
            for (i, a) in rects.iter().enumerate() {
                assert!(a.x >= 0.0 && a.y >= 0.0 && a.x + a.w <= 2000.0 && a.y + a.h <= 1200.0, "n={n} card {i} {a:?}");
                for b in &rects[i + 1..] {
                    let apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                    assert!(apart, "n={n}: {a:?} overlaps {b:?}");
                }
            }
        }
    }

    #[test]
    fn arrow_keys_move_through_the_grid() {
        // 5 cards in 3 columns: 0 1 2 / 3 4
        assert_eq!(step(0, 5, 3, 1, 0), 1);
        assert_eq!(step(4, 5, 3, 1, 0), 0); // wraps
        assert_eq!(step(1, 5, 3, 0, 1), 4);
        assert_eq!(step(2, 5, 3, 0, 1), 2); // nothing below: stay
        assert_eq!(step(3, 5, 3, 0, -1), 0);
    }

    #[test]
    fn draws_every_card_with_its_panes() {
        let card = |n: usize| TabCard {
            title: format!("tab {n}"),
            color: None,
            working: n % 2,
            waiting: 1,
            active: n == 0,
            panes: vec![PaneThumb {
                at: R { x: 0.0, y: 0.0, w: 1.0, h: 1.0 },
                cols: 80,
                rows: 24,
                runs: vec![Run { row: 0, col: 0, len: 10, color: Rgb(200, 200, 200), bg: false }],
                focused: true,
            }],
        };
        let cards: Vec<TabCard> = (0..4).map(card).collect();
        let mut f = Frame::default();
        let hits = draw(&mut f, &Theme::default(), &geom(), &cards, 1, 1.0);
        assert_eq!(hits.len(), 4);
        let text: String = f.overlay_lines.iter().flat_map(|l| l.spans.iter().map(|s| s.text.clone())).collect();
        assert!(text.contains("tab 3") && text.contains("waiting") && text.contains("current"));
    }
}
