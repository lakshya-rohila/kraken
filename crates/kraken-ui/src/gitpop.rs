//! The branch popover: click the git branch in the status bar to see what
//! changed, and click a file to open it.

use kraken_config::{Rgb, Theme};
use kraken_mux::layout::R;
use kraken_render::{FontKind, Frame, Rect, Span, TextLine};
use kraken_term::status::{GitChange, GitInfo};

/// Files listed before "+N more".
pub const MAX_ROWS: usize = 14;

pub struct PopGeom {
    /// Left edge of the branch segment (the popover lines up with it).
    pub anchor_x: f32,
    /// Top of the status bar (the popover sits just above it).
    pub bottom: f32,
    pub window_w: f32,
    pub char_w: f32,
    pub font_px: f32,
    pub scale: f32,
}

/// What a code means, in words and colour.
fn describe(code: &str, t: &Theme) -> (&'static str, Rgb) {
    let c = code.trim();
    match c.chars().next() {
        Some('?') => ("new", t.normal[2]),
        Some('A') => ("added", t.normal[2]),
        Some('D') => ("deleted", t.normal[1]),
        Some('R') => ("renamed", t.normal[6]),
        Some('U') => ("conflict", t.normal[1]),
        _ => ("changed", t.normal[3]),
    }
}

/// Draw the popover. `changes` is `None` while `git status` runs.
/// Returns the clickable file rows: (rect, index into `changes`).
pub fn draw(
    frame: &mut Frame,
    t: &Theme,
    g: &PopGeom,
    info: Option<&GitInfo>,
    changes: Option<&[GitChange]>,
    hover: Option<usize>,
) -> Vec<(R, usize)> {
    let s = g.scale;
    let pad = 12.0 * s;
    let row_h = (g.font_px * 1.55).round();
    let rows = changes.map_or(1, |c| c.len().clamp(1, MAX_ROWS) + usize::from(c.len() > MAX_ROWS));
    let longest = changes.map(|c| c.iter().take(MAX_ROWS).map(|f| f.path.chars().count()).max().unwrap_or(0)).unwrap_or(0).clamp(24, 64);
    let w = ((longest + 11) as f32 * g.char_w + pad * 2.0).min(g.window_w - 16.0 * s);
    let header_h = row_h * 1.6;
    let footer_h = row_h * 1.3;
    let h = header_h + rows as f32 * row_h + footer_h + pad * 0.5;
    let x = g.anchor_x.clamp(8.0 * s, (g.window_w - w - 8.0 * s).max(8.0 * s));
    let y = g.bottom - h - 6.0 * s;

    // Card with a hairline border and a soft shadow.
    for (grow, a) in [(10.0f32, 18u8), (5.0, 30)] {
        let gs = grow * s;
        // Spreads up and sideways only, so it never spills onto the status bar.
        frame.overlay_rects.push(Rect { x: x - gs, y: y - gs, w: w + gs * 2.0, h: h + gs, color: [0, 0, 0, a] });
    }
    let b = t.border;
    frame.overlay_rects.push(Rect { x: x - s, y: y - s, w: w + 2.0 * s, h: h + 2.0 * s, color: [b.0, b.1, b.2, 255] });
    let e = t.elevated;
    frame.overlay_rects.push(Rect { x, y, w, h, color: [e.0, e.1, e.2, 255] });

    let line = |x: f32, y: f32, font: FontKind, size: f32, spans: Vec<Span>| TextLine {
        x,
        y,
        font,
        size,
        line_height: row_h,
        spans,
        clip: Some([x as i32, y as i32, (g.anchor_x.max(x) + w + g.window_w) as i32, (y + row_h * 2.0) as i32]),
        cell_width: if font == FontKind::Mono { Some(g.char_w) } else { None },
    };
    let span = |text: String, c: Rgb, bold: bool| Span { text, color: [c.0, c.1, c.2], bold, italic: false };

    // Header: branch and where it stands.
    let mut head = vec![span(format!(" {}", info.map_or("—", |i| i.branch.as_str())), t.normal[5], true)];
    if let Some(i) = info {
        if i.ahead > 0 {
            head.push(span(format!("   ↑{} to push", i.ahead), t.normal[6], false));
        }
        if i.behind > 0 {
            head.push(span(format!("   ↓{} to pull", i.behind), t.normal[1], false));
        }
    }
    let count = match changes {
        None => "checking…".to_string(),
        Some([]) => "nothing to commit".to_string(),
        Some(c) => format!("{} changed", c.len()),
    };
    head.push(span(format!("   {count}"), t.muted, false));
    frame.overlay_lines.push(line(x + pad, y + (header_h - row_h) / 2.0, FontKind::Mono, g.font_px, head));
    frame.overlay_rects.push(Rect { x: x + pad, y: y + header_h - s, w: w - pad * 2.0, h: s, color: [b.0, b.1, b.2, 255] });

    // Files.
    let mut hits = Vec::new();
    let mut ry = y + header_h + pad * 0.25;
    match changes {
        None | Some([]) => {
            let msg = if changes.is_none() { "Reading git status…" } else { "Working tree clean ✓" };
            let c = if changes.is_none() { t.muted } else { t.normal[2] };
            frame.overlay_lines.push(line(x + pad, ry, FontKind::Mono, g.font_px, vec![span(msg.into(), c, false)]));
        }
        Some(list) => {
            for (i, f) in list.iter().take(MAX_ROWS).enumerate() {
                let r = R { x: x + pad * 0.5, y: ry, w: w - pad, h: row_h };
                if hover == Some(i) {
                    let a = t.selection;
                    frame.overlay_rects.push(Rect { x: r.x, y: r.y, w: r.w, h: r.h, color: [a.0, a.1, a.2, 255] });
                }
                let (word, c) = describe(&f.code, t);
                frame.overlay_lines.push(line(
                    x + pad,
                    ry,
                    FontKind::Mono,
                    g.font_px,
                    vec![span(format!("{word:<9}"), c, false), span(f.path.clone(), t.foreground, false)],
                ));
                hits.push((r, i));
                ry += row_h;
            }
            if list.len() > MAX_ROWS {
                let more = format!("+{} more — see them all with git status", list.len() - MAX_ROWS);
                frame.overlay_lines.push(line(x + pad, ry, FontKind::Mono, g.font_px, vec![span(more, t.muted, false)]));
            }
        }
    }
    let foot = "click a file to open it  ·  esc to close";
    frame.overlay_lines.push(line(x + pad, y + h - footer_h, FontKind::Mono, g.font_px * 0.92, vec![span(foot.into(), t.muted, false)]));
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom() -> PopGeom {
        PopGeom { anchor_x: 300.0, bottom: 1300.0, window_w: 2000.0, char_w: 9.0, font_px: 14.0, scale: 2.0 }
    }

    #[test]
    fn lists_files_and_caps_the_list() {
        let changes: Vec<GitChange> = (0..20).map(|i| GitChange { code: " M".into(), path: format!("src/f{i}.rs") }).collect();
        let mut f = Frame::default();
        let hits = draw(&mut f, &Theme::default(), &geom(), None, Some(&changes), Some(2));
        assert_eq!(hits.len(), MAX_ROWS);
        let text: String = f.overlay_lines.iter().flat_map(|l| l.spans.iter().map(|s| s.text.clone())).collect();
        assert!(text.contains("+6 more"), "{text}");
        assert!(text.contains("20 changed"));
        // Stays above the status bar and inside the window.
        assert!(f.overlay_rects.iter().all(|r| r.y + r.h <= 1300.0 + 1.0 && r.x >= 0.0));
    }

    #[test]
    fn says_when_loading_or_clean() {
        let mut f = Frame::default();
        assert!(draw(&mut f, &Theme::default(), &geom(), None, None, None).is_empty());
        let mut f2 = Frame::default();
        draw(&mut f2, &Theme::default(), &geom(), None, Some(&[]), None);
        let text: String = f2.overlay_lines.iter().flat_map(|l| l.spans.iter().map(|s| s.text.clone())).collect();
        assert!(text.contains("Working tree clean"));
    }
}
