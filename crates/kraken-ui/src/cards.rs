//! Notification cards that slide in from the right when an agent needs you:
//! it went quiet waiting for input, rang the bell, or is asking permission.
//! The card shows the agent's actual question (read from its screen) and,
//! when it offers numbered choices or y/n, buttons that answer it directly.

use kraken_config::{Rgb, Theme};
use kraken_mux::layout::{PaneId, R};
use kraken_render::{FontKind, Frame, Rect, Span, TextLine};
use std::time::Instant;

pub const SLIDE_IN_MS: f32 = 380.0;
pub const SLIDE_OUT_MS: f32 = 200.0;
const MAX_VISIBLE: usize = 4;

/// A choice the card can send to the agent.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub label: String,
    /// Bytes typed into the pane when clicked.
    pub keys: String,
}

#[derive(Debug, Clone)]
pub struct Card {
    pub pane: PaneId,
    pub agent: String,
    pub where_: String,
    pub question: Vec<String>,
    pub choices: Vec<Choice>,
    pub born: Instant,
    /// Sliding out (set when dismissed).
    pub leaving: Option<Instant>,
}

/// What the agent is asking, read from the bottom of its screen.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ask {
    pub question: Vec<String>,
    pub choices: Vec<Choice>,
}

/// Strip box-drawing frames and selection arrows that TUIs draw around text.
fn clean(line: &str) -> String {
    let t: String = line.chars().map(|c| if "│╭╮╰╯─┃┏┓┗┛━╌┆".contains(c) { ' ' } else { c }).collect();
    t.trim().to_string()
}

/// "1. Yes" / "❯ 2) Always" → (2, "Always").
fn numbered(line: &str) -> Option<(char, String)> {
    let t = line.trim_start_matches(['❯', '›', '>', '→', '*', ' ']);
    let mut it = t.chars();
    let d = it.next().filter(|c| c.is_ascii_digit() && *c != '0')?;
    let rest: String = it.collect();
    let rest = rest.strip_prefix('.').or_else(|| rest.strip_prefix(')'))?;
    // A real option has a space after "1." ("1.5 seconds" is not an option).
    if !rest.starts_with(' ') {
        return None;
    }
    let label = rest.trim();
    (!label.is_empty()).then(|| (d, label.to_string()))
}

fn short(label: &str, max: usize) -> String {
    // "Yes, allow all edits during this session (shift+tab)" → "Yes, allow all…"
    let base = label.split(" (").next().unwrap_or(label).trim();
    if base.chars().count() <= max { base.to_string() } else { format!("{}…", base.chars().take(max - 1).collect::<String>().trim_end()) }
}

/// Look at the last lines of the screen (top to bottom) for a question and choices.
pub fn read_ask(lines: &[String]) -> Ask {
    let lines: Vec<String> = lines.iter().map(|l| clean(l)).collect();
    let mut ask = Ask::default();
    // Numbered options at the bottom: take the last consecutive run.
    let opt_rows: Vec<usize> = lines.iter().enumerate().filter(|(_, l)| numbered(l).is_some()).map(|(i, _)| i).collect();
    if let Some(&first) = opt_rows.first() {
        let mut run_start = first;
        for w in opt_rows.windows(2) {
            if w[1] > w[0] + 2 {
                run_start = w[1];
            }
        }
        for l in &lines[run_start..] {
            if let Some((d, label)) = numbered(l)
                && ask.choices.len() < 4
            {
                ask.choices.push(Choice { label: format!("{d} {}", short(&label, 16)), keys: d.to_string() });
            }
        }
        // The question is the nearest non-empty line above the options.
        if let Some(q) = lines[..run_start].iter().rev().find(|l| !l.is_empty()) {
            ask.question.push(q.clone());
        }
        return ask;
    }
    // y/n prompts.
    if let Some(l) = lines.iter().rev().find(|l| {
        let lower = l.to_lowercase();
        lower.contains("(y/n)") || lower.contains("[y/n]")
    }) {
        ask.question.push(l.clone());
        ask.choices = vec![Choice { label: "Yes".into(), keys: "y\r".into() }, Choice { label: "No".into(), keys: "n\r".into() }];
        return ask;
    }
    // Otherwise: the last line that reads like a question, if any.
    if let Some(q) = lines.iter().rev().take(12).find(|l| l.ends_with('?') && l.len() > 3) {
        ask.question.push(q.clone());
    }
    ask
}

pub struct CardGeom {
    /// Area cards stack inside (below the tab bar, above the status bar).
    pub area: R,
    pub scale: f32,
    pub char_w: f32,
    pub font_px: f32,
}

/// What a click on a card hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CardHit {
    Open(usize),
    Choice(usize, usize),
    Close(usize),
}

fn rgb(c: Rgb) -> [u8; 3] {
    [c.0, c.1, c.2]
}

/// Draw the stack; returns clickable regions for this frame.
pub fn draw(frame: &mut Frame, t: &Theme, g: &CardGeom, cards: &[Card], speed: f32) -> Vec<(R, CardHit)> {
    let s = g.scale;
    let mut hits = Vec::new();
    let w = (380.0 * s).min(g.area.w - 24.0 * s);
    let line_h = (g.font_px * 1.45).round();
    let pad = 12.0 * s;
    let mut y = g.area.y + 12.0 * s;
    for (i, c) in cards.iter().enumerate().take(MAX_VISIBLE) {
        // Slide in from the right with a little overshoot; slide back out.
        let p_in = crate::anim::ease_out_back(crate::anim::progress(c.born, SLIDE_IN_MS, speed));
        let p = match c.leaving {
            Some(at) => (1.0 - crate::anim::ease_out_cubic(crate::anim::progress(at, SLIDE_OUT_MS, speed))) * p_in,
            None => p_in,
        };
        let q_lines = c.question.len().clamp(1, 3);
        // Lay out answer buttons first, wrapping onto more rows as needed.
        let btn_h = line_h * 1.15;
        let inner_left = pad + 4.0 * s;
        let max_right = w - pad;
        let mut buttons: Vec<(f32, f32, f32)> = Vec::new(); // (dx, row, width)
        let (mut bx, mut row) = (inner_left, 0.0f32);
        for ch in &c.choices {
            let bw = ((ch.label.chars().count() as f32 + 2.0) * g.char_w).min(max_right - inner_left);
            if bx + bw > max_right && bx > inner_left {
                bx = inner_left;
                row += 1.0;
            }
            buttons.push((bx, row, bw));
            bx += bw + 8.0 * s;
        }
        let btn_rows = if c.choices.is_empty() { 0.0 } else { row + 1.0 };
        let h =
            pad * 2.0 + line_h * (2.0 + q_lines as f32) + btn_rows * (btn_h + 6.0 * s) + if btn_rows > 0.0 { line_h * 0.3 } else { 0.0 };
        let x = g.area.right() - w - 14.0 * s + (1.0 - p) * (w + 40.0 * s);
        let r = R { x, y, w, h };
        let alpha = (255.0 * p.clamp(0.0, 1.0)) as u8;
        let (e, o) = (t.elevated, t.ssh);
        // Shadow, border, body, and the orange "needs you" stripe.
        frame.overlay_rects.push(Rect { x: x + 3.0 * s, y: y + 5.0 * s, w, h, color: [0, 0, 0, alpha / 3] });
        let b = o.mix(t.border, 0.45);
        frame.overlay_rects.push(Rect { x: x - s, y: y - s, w: w + 2.0 * s, h: h + 2.0 * s, color: [b.0, b.1, b.2, alpha] });
        frame.overlay_rects.push(Rect { x, y, w, h, color: [e.0, e.1, e.2, alpha] });
        frame.overlay_rects.push(Rect { x, y, w: 4.0 * s, h, color: [o.0, o.1, o.2, alpha] });

        let clip = Some([x as i32, y as i32, (x + w) as i32, (y + h) as i32]);
        let text = |lx: f32, ly: f32, spans: Vec<Span>| TextLine {
            x: lx,
            y: ly,
            font: FontKind::Mono,
            size: g.font_px,
            line_height: line_h,
            spans,
            clip,
            cell_width: Some(g.char_w),
        };
        let fade = |c: Rgb| e.mix(c, p.clamp(0.0, 1.0));
        let sp = |s: String, c: Rgb, bold: bool| Span { text: s, color: rgb(fade(c)), bold, italic: false };

        let mut ty = y + pad;
        frame.overlay_lines.push(text(
            x + pad + 4.0 * s,
            ty,
            vec![sp("◆ ".into(), o, true), sp(c.agent.clone(), t.foreground, true), sp("  needs you".into(), o, false)],
        ));
        // Close ×
        let close = R { x: x + w - 26.0 * s, y: ty - 2.0 * s, w: 20.0 * s, h: line_h };
        frame.overlay_lines.push(text(close.x + 5.0 * s, ty, vec![sp("×".into(), t.muted, false)]));
        ty += line_h;
        // Where it is, on its own line.
        frame.overlay_lines.push(text(x + pad + 4.0 * s, ty, vec![sp(format!("{}  ↗ click to open", c.where_), t.muted, false)]));
        ty += line_h;
        let body: Vec<String> = if c.question.is_empty() { vec!["Waiting for your next message.".into()] } else { c.question.clone() };
        for l in body.iter().take(3) {
            frame.overlay_lines.push(text(x + pad + 4.0 * s, ty, vec![sp(l.clone(), t.foreground.mix(t.muted, 0.2), false)]));
            ty += line_h;
        }

        // Answer buttons.
        for (ci, (ch, &(dx, brow, bw))) in c.choices.iter().zip(&buttons).enumerate() {
            let bx = x + dx;
            let by = ty + line_h * 0.25 + brow * (btn_h + 6.0 * s);
            let first = ci == 0;
            let fill = if first { t.accent.mix(e, 0.25) } else { t.border.mix(e, 0.1) };
            let fill = e.mix(fill, p.clamp(0.0, 1.0));
            frame.overlay_rects.push(Rect { x: bx, y: by, w: bw, h: btn_h, color: [fill.0, fill.1, fill.2, alpha] });
            let tc = if first { Rgb(255, 255, 255) } else { t.foreground };
            frame.overlay_lines.push(text(bx + g.char_w, by + line_h * 0.08, vec![sp(ch.label.clone(), tc, first)]));
            hits.push((R { x: bx, y: by, w: bw, h: btn_h }, CardHit::Choice(i, ci)));
        }
        hits.push((close, CardHit::Close(i)));
        hits.push((r, CardHit::Open(i)));
        y += h + 10.0 * s;
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn l(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn claude_permission_dialog() {
        let screen = l(&[
            "╭──────────────────────────────────────────────╮",
            "│ Edit file                                    │",
            "│ Do you want to make this edit to main.rs?    │",
            "│ ❯ 1. Yes                                      │",
            "│   2. Yes, allow all edits during this session (shift+tab) │",
            "│   3. No, and tell Claude what to do differently (esc) │",
            "╰──────────────────────────────────────────────╯",
        ]);
        let a = read_ask(&screen);
        assert_eq!(a.question, vec!["Do you want to make this edit to main.rs?"]);
        assert_eq!(a.choices.len(), 3);
        assert_eq!(a.choices[0], Choice { label: "1 Yes".into(), keys: "1".into() });
        assert_eq!(a.choices[1].label, "2 Yes, allow all…");
        assert_eq!(a.choices[2].keys, "3");
    }

    #[test]
    fn yes_no_and_plain_questions() {
        let a = read_ask(&l(&["Overwrite config.toml? (y/n)"]));
        assert_eq!(a.choices.len(), 2);
        assert_eq!(a.choices[0].keys, "y\r");
        let a = read_ask(&l(&["Done.", "Should I also update the tests?", "", "> "]));
        assert_eq!(a.question, vec!["Should I also update the tests?"]);
        assert!(a.choices.is_empty());
        let a = read_ask(&l(&["Compiling…", "> "]));
        assert!(a.question.is_empty());
    }

    #[test]
    fn numbered_lines() {
        assert_eq!(numbered("❯ 1. Yes"), Some(('1', "Yes".into())));
        assert_eq!(numbered("  2) Always"), Some(('2', "Always".into())));
        assert_eq!(numbered("2026 was a year"), None);
        assert_eq!(numbered("1.5 seconds"), None);
    }
}
