//! Coding agents (Claude Code, Codex, Gemini CLI…) running in panes:
//! detection, working/waiting state, and the ⌘⇧A dashboard.
//!
//! State comes from the output stream: agents animate a spinner while they
//! work, so steady output means "working" and silence means "waiting for you".

use kraken_config::{Rgb, Theme};
use kraken_mux::layout::{PaneId, R};
use kraken_render::{FontKind, Frame, Rect, Span, TextLine};
use std::time::{Duration, Instant};

/// Quiet for this long (with the agent still running) = waiting for you.
pub const QUIET: Duration = Duration::from_millis(2500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Working,
    Waiting,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub name: &'static str,
    pub state: AgentState,
    /// When it entered `state`.
    pub since: Instant,
    /// The agent asked for attention (bell / notification) since you last looked.
    pub attention: bool,
}

/// Recognise an agent from the foreground process name, and for script
/// runtimes (node, python, bun…) from its arguments.
pub fn detect(process: &str, args: impl FnOnce() -> Option<Vec<String>>) -> Option<&'static str> {
    let by_name = |n: &str| -> Option<&'static str> {
        Some(match n {
            "claude" => "Claude Code",
            "codex" => "Codex",
            "aider" => "Aider",
            "gemini" => "Gemini CLI",
            "opencode" => "OpenCode",
            "goose" => "Goose",
            "amp" => "Amp",
            "cursor-agent" => "Cursor Agent",
            "copilot" => "Copilot CLI",
            "qwen" => "Qwen Code",
            "crush" => "Crush",
            _ => return None,
        })
    };
    if let Some(a) = by_name(process) {
        return Some(a);
    }
    if !matches!(process, "node" | "bun" | "deno" | "python" | "python3" | "uv") {
        return None;
    }
    let args = args()?;
    for a in args.iter().skip(1).take(3) {
        let base = a.rsplit('/').next().unwrap_or(a).trim_end_matches(".js").trim_end_matches(".mjs");
        if let Some(name) = by_name(base) {
            return Some(name);
        }
        if a.contains("claude-code") {
            return Some("Claude Code");
        }
        if a.contains("gemini-cli") {
            return Some("Gemini CLI");
        }
    }
    None
}

/// Move an agent to its next state given when output last arrived.
/// Returns true when it just went quiet (finished a turn, now waiting).
pub fn update(agent: &mut Agent, last_output: Instant, now: Instant) -> bool {
    let quiet = now.saturating_duration_since(last_output) >= QUIET;
    let next = if quiet { AgentState::Waiting } else { AgentState::Working };
    if next == agent.state {
        return false;
    }
    let went_quiet = agent.state == AgentState::Working && next == AgentState::Waiting;
    agent.state = next;
    agent.since = now;
    if next == AgentState::Working {
        agent.attention = false;
    }
    went_quiet
}

pub fn ago(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", s / 60)
    } else {
        format!("{}h {}m", s / 3600, (s % 3600) / 60)
    }
}

/// One row of the dashboard.
pub struct Row {
    pub pane: PaneId,
    pub tab: usize,
    pub tab_label: String,
    pub folder: String,
    pub agent: Agent,
}

pub struct PanelGeom {
    pub window: R,
    pub scale: f32,
    pub char_w: f32,
    pub font_px: f32,
}

/// The dashboard: every agent in this window, its state, and where it lives.
pub fn draw(frame: &mut Frame, t: &Theme, g: &PanelGeom, rows: &[Row], selected: usize, p: f32) {
    let s = g.scale;
    let win = g.window;
    let row_h = (g.font_px * 2.6).round();
    let w = (720.0 * s).min(win.w - 60.0 * s);
    let n = rows.len().max(1);
    let header = row_h * 1.3;
    let h = header + n as f32 * row_h + row_h * 0.9;
    let x = win.x + (win.w - w) / 2.0;
    let y = win.y + win.h * 0.14 - (1.0 - p) * 14.0 * s;
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
    let line = |lx: f32, ly: f32, lh: f32, spans: Vec<Span>| TextLine {
        x: lx,
        y: ly + (lh - g.font_px * 1.3) / 2.0,
        font: FontKind::Mono,
        size: g.font_px,
        line_height: g.font_px * 1.3,
        spans,
        clip,
        cell_width: Some(g.char_w),
    };
    let pad = 16.0 * s;
    let working = rows.iter().filter(|r| r.agent.state == AgentState::Working).count();
    let waiting = rows.len() - working;
    frame.overlay_lines.push(line(
        x + pad,
        y,
        header,
        vec![
            span("◆ ".into(), t.accent, true),
            span("AGENTS".into(), t.foreground, true),
            span(format!("   {working} working · {waiting} waiting for you"), t.muted, false),
        ],
    ));
    frame.overlay_rects.push(Rect { x, y: y + header, w, h: s, color: [t.border.0, t.border.1, t.border.2, a] });

    if rows.is_empty() {
        frame.overlay_lines.push(line(
            x + pad,
            y + header,
            row_h,
            vec![span("No agents running. Start one (e.g. `claude`) in any pane.".into(), t.muted, false)],
        ));
    }
    for (i, r) in rows.iter().enumerate() {
        let ry = y + header + i as f32 * row_h;
        if i == selected {
            let sel = t.selection;
            frame.overlay_rects.push(Rect {
                x: x + 6.0 * s,
                y: ry + 3.0 * s,
                w: w - 12.0 * s,
                h: row_h - 6.0 * s,
                color: [sel.0, sel.1, sel.2, a],
            });
            frame.overlay_rects.push(Rect {
                x: x + 6.0 * s,
                y: ry + 3.0 * s,
                w: 3.0 * s,
                h: row_h - 6.0 * s,
                color: [t.accent.0, t.accent.1, t.accent.2, a],
            });
        }
        let (icon, color, word) = match (r.agent.state, r.agent.attention) {
            (AgentState::Working, _) => ("●", t.normal[2], "working"),
            (AgentState::Waiting, true) => ("◆", t.ssh, "needs you"),
            (AgentState::Waiting, false) => ("◆", t.normal[3], "waiting"),
        };
        let since = ago(r.agent.since.elapsed());
        frame.overlay_lines.push(line(
            x + pad,
            ry,
            row_h,
            vec![
                span(format!("{icon} "), color, true),
                span(format!("{:<13}", r.agent.name), t.foreground, true),
                span(format!("{word} {since}"), color, false),
            ],
        ));
        let where_ = format!("{}  ·  {}", r.tab_label, r.folder);
        let wx = x + pad + 36.0 * g.char_w;
        frame.overlay_lines.push(line(wx, ry, row_h, vec![span(where_, t.muted.mix(t.foreground, 0.25), false)]));
    }
    let fy = y + h - row_h * 0.9;
    frame.overlay_lines.push(line(
        x + pad,
        fy,
        row_h * 0.9,
        vec![span("↑↓ move   ↩ jump to agent   esc close".into(), t.muted.mix(e, 0.2), false)],
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_agents() {
        assert_eq!(detect("claude", || None), Some("Claude Code"));
        assert_eq!(detect("codex", || None), Some("Codex"));
        assert_eq!(detect("zsh", || None), None);
        let gemini = || Some(vec!["node".into(), "/opt/homebrew/lib/node_modules/@google/gemini-cli/bundle/gemini.js".into()]);
        assert_eq!(detect("node", gemini), Some("Gemini CLI"));
        let other = || Some(vec!["node".into(), "server.js".into()]);
        assert_eq!(detect("node", other), None);
    }

    #[test]
    fn working_then_waiting() {
        let t0 = Instant::now();
        let mut a = Agent { name: "Claude Code", state: AgentState::Working, since: t0, attention: false };
        assert!(!update(&mut a, t0, t0 + Duration::from_millis(500)));
        assert!(update(&mut a, t0, t0 + QUIET + Duration::from_millis(1)));
        assert_eq!(a.state, AgentState::Waiting);
        // Output again: back to working, attention cleared.
        a.attention = true;
        let t1 = t0 + Duration::from_secs(10);
        assert!(!update(&mut a, t1, t1));
        assert_eq!(a.state, AgentState::Working);
        assert!(!a.attention);
    }
}
