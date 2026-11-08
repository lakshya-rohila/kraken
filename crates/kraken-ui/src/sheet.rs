//! The ⌘/ shortcut sheet: every key and what it does, as coloured keycaps.

use kraken_config::{Rgb, Theme};
use kraken_mux::layout::R;
use kraken_render::{FontKind, Frame, Rect, Span, TextLine};

pub struct Section {
    pub title: &'static str,
    /// Index into the theme's ANSI colours, used for the section's accent.
    pub color: usize,
    pub items: &'static [(&'static str, &'static str)],
}

/// Keys are separated by spaces; each space-separated group becomes one keycap.
pub const SECTIONS: &[Section] = &[
    Section {
        title: "TABS & WINDOWS",
        color: 6,
        items: &[
            ("⌘T", "New tab in this folder"),
            ("⌘⇧R", "Rename tab (or double-click it)"),
            ("⌘1…⌘9", "Jump to tab (⌘9 = last)"),
            ("⌃Tab ⌃⇧Tab", "Next / previous tab"),
            ("⌘⇧] ⌘⇧[", "Next / previous tab"),
            ("⌘W", "Close pane → tab → window"),
            ("⌘N", "New window"),
            ("⌘Q", "Quit Kraken"),
        ],
    },
    Section {
        title: "PANES",
        color: 5,
        items: &[
            ("⌘D", "Split right"),
            ("⌘⇧D", "Split down"),
            ("⌘⌥ ←↑↓→", "Move focus between panes"),
            ("⌘⌃ ←↑↓→", "Resize: move the divider"),
            ("⌘⇧↩", "Zoom pane / unzoom"),
            ("⌘⌥⇧ ←↑↓→", "Swap pane with neighbour"),
            ("⌘⌥ Drag", "Move pane: edge · swap · tab"),
        ],
    },
    Section {
        title: "MOUSE",
        color: 3,
        items: &[
            ("Click", "Focus a pane or tab"),
            ("Drag", "Select · resize divider · move tab"),
            ("2× 3×", "Select word / line"),
            ("⌥ Drag", "Block selection"),
            ("⇧ Click", "Extend selection"),
        ],
    },
    Section {
        title: "EDITING",
        color: 2,
        items: &[
            ("⌘C ⌘V", "Copy / paste"),
            ("⌘A", "Select all"),
            ("⌥← ⌥→", "Jump word left / right"),
            ("⌘← ⌘→", "Start / end of line"),
            ("⌥⌫", "Delete word"),
            ("⌘⌫", "Delete to line start"),
            ("⇧↩", "New line without sending (agents)"),
        ],
    },
    Section {
        title: "VIEW",
        color: 4,
        items: &[
            ("⌘= ⌘- ⌘0", "Font bigger / smaller / reset"),
            ("⌘K", "Clear screen & history"),
            ("⌘↑ ⌘↓", "Previous / next prompt"),
            ("⌘⇧↑ ⌘⇧↓", "Top / bottom of history"),
            ("⇧PgUp ⇧PgDn", "Scroll a page"),
        ],
    },
    Section {
        title: "KRAKEN",
        color: 1,
        items: &[
            ("⌘⇧P", "Command palette (themes too)"),
            ("⌘⇧A", "Agents: who's working / waiting"),
            ("⌘P", "Find a file · ⌥↩ nvim · ⇧↩ paste"),
            ("⌘B", "File sidebar · ␣ Quick Look"),
            ("⌘ Click", "Open link / file:line in editor"),
            ("⌘F", "Find · ↩ older ⇧↩ newer ⌘R regex"),
            ("⌘,", "Settings (applies on save)"),
            ("⌘/", "Show / hide this sheet"),
            ("⌘⇧K", "Summon the Kraken"),
        ],
    },
];

pub struct SheetGeom {
    pub window: R,
    pub scale: f32,
    /// Monospace character width / font size for the sheet's text.
    pub char_w: f32,
    pub font_px: f32,
}

fn width_cells(s: &str) -> usize {
    s.chars().count()
}

fn rgb(c: Rgb) -> [u8; 3] {
    [c.0, c.1, c.2]
}

/// Draw the sheet. `p` is the open progress 0..=1 (eased), which slides and fades it in.
pub fn draw(frame: &mut Frame, t: &Theme, g: &SheetGeom, p: f32) {
    let s = g.scale;
    let w = g.window;
    let row_h = (g.font_px * 1.9).round();
    let key_cells = 15usize;
    let desc_cells = 34usize;
    let col_w = (key_cells + desc_cells) as f32 * g.char_w;
    let pad = 28.0 * s;
    let col_gap = 36.0 * s;
    let two_cols = w.w > col_w * 2.0 + col_gap + pad * 2.0 + 40.0 * s;

    // Split sections into columns, balancing row counts.
    let rows_of = |sec: &Section| sec.items.len() + 2;
    let total: usize = SECTIONS.iter().map(rows_of).sum();
    let mut columns: Vec<Vec<&Section>> = vec![Vec::new()];
    let mut acc = 0;
    for sec in SECTIONS {
        if two_cols && columns.len() == 1 && acc >= total / 2 {
            columns.push(Vec::new());
        }
        acc += rows_of(sec);
        columns.last_mut().unwrap().push(sec);
    }
    let max_rows = columns.iter().map(|c| c.iter().map(|s| rows_of(s)).sum::<usize>()).max().unwrap_or(0);

    let header_h = row_h * 2.2;
    let panel_w = columns.len() as f32 * col_w + (columns.len() as f32 - 1.0) * col_gap + pad * 2.0;
    let panel_h = header_h + max_rows as f32 * row_h + pad;
    let slide = (1.0 - p) * 18.0 * s;
    let px = w.x + (w.w - panel_w) / 2.0;
    let py = (w.y + (w.h - panel_h) / 2.0).max(w.y + 8.0 * s) + slide;
    let fade = |c: Rgb, base: Rgb| base.mix(c, p);

    // Dim everything behind, then the panel with an accent border.
    let bg = t.background;
    frame.overlay_rects.push(Rect { x: w.x, y: w.y, w: w.w, h: w.h, color: [bg.0, bg.1, bg.2, (150.0 * p) as u8] });
    let a = (255.0 * p) as u8;
    let border = t.accent.mix(t.border, 0.35);
    frame.overlay_rects.push(Rect {
        x: px - s,
        y: py - s,
        w: panel_w + 2.0 * s,
        h: panel_h + 2.0 * s,
        color: [border.0, border.1, border.2, a],
    });
    let e = t.elevated;
    frame.overlay_rects.push(Rect { x: px, y: py, w: panel_w, h: panel_h, color: [e.0, e.1, e.2, a] });
    // A thin gradient bar across the top edge.
    let bar_segments = 24;
    for i in 0..bar_segments {
        let x = i as f32 / bar_segments as f32;
        let c = if x < 0.5 { t.normal[6].mix(t.accent, x * 2.0) } else { t.accent.mix(t.normal[1], (x - 0.5) * 2.0) };
        let seg_w = panel_w / bar_segments as f32;
        frame.overlay_rects.push(Rect { x: px + i as f32 * seg_w, y: py, w: seg_w + 1.0, h: 3.0 * s, color: [c.0, c.1, c.2, a] });
    }

    let clip = Some([px as i32, py as i32, (px + panel_w) as i32, (py + panel_h) as i32]);
    let line = |x: f32, y: f32, spans: Vec<Span>| TextLine {
        x,
        y,
        font: FontKind::Mono,
        size: g.font_px,
        line_height: row_h,
        spans,
        clip,
        cell_width: Some(g.char_w),
    };
    let span = |text: &str, c: Rgb, bold: bool| Span { text: text.to_string(), color: rgb(fade(c, e)), bold, italic: false };

    // Header.
    frame.overlay_lines.push(line(
        px + pad,
        py + row_h * 0.6,
        vec![span("◆ ", t.accent, true), span("KRAKEN", t.foreground, true), span("  shortcuts", t.muted, false)],
    ));
    let close_hint = "esc to close";
    frame.overlay_lines.push(line(
        px + panel_w - pad - width_cells(close_hint) as f32 * g.char_w,
        py + row_h * 0.6,
        vec![span(close_hint, t.muted, false)],
    ));

    // Columns of sections.
    for (ci, col) in columns.iter().enumerate() {
        let cx = px + pad + ci as f32 * (col_w + col_gap);
        let mut y = py + header_h;
        for sec in col {
            let sc = t.normal[sec.color];
            frame.overlay_rects.push(Rect { x: cx, y: y + row_h * 0.3, w: 3.0 * s, h: row_h * 0.45, color: [sc.0, sc.1, sc.2, a] });
            frame.overlay_lines.push(line(cx + 10.0 * s, y, vec![span(sec.title, sc, true)]));
            y += row_h * 1.15;
            for (keys, desc) in sec.items {
                // Keycaps.
                let mut kx = cx;
                let cap_h = row_h * 0.72;
                for cap in keys.split(' ') {
                    let cw = width_cells(cap) as f32 * g.char_w + 12.0 * s;
                    let capc = sc.mix(e, 0.78);
                    frame.overlay_rects.push(Rect {
                        x: kx,
                        y: y + (row_h - cap_h) / 2.0,
                        w: cw,
                        h: cap_h,
                        color: [capc.0, capc.1, capc.2, a],
                    });
                    let edge = sc.mix(e, 0.45);
                    frame.overlay_rects.push(Rect {
                        x: kx,
                        y: y + (row_h + cap_h) / 2.0 - 2.0 * s,
                        w: cw,
                        h: 2.0 * s,
                        color: [edge.0, edge.1, edge.2, a],
                    });
                    frame.overlay_lines.push(line(kx + 6.0 * s, y - s, vec![span(cap, sc.mix(Rgb(255, 255, 255), 0.35), true)]));
                    kx += cw + 6.0 * s;
                }
                frame.overlay_lines.push(line(cx + key_cells as f32 * g.char_w, y - s, vec![span(desc, t.foreground, false)]));
                y += row_h;
            }
            y += row_h * 0.85;
        }
    }
}
