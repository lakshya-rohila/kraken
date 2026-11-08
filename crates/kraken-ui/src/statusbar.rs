//! The status bar ("legend bar") along the bottom of the window. It describes
//! the focused pane: what's running, where, git state, the agents in this
//! window, and how the last command went.
//!
//! ```text
//! ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ activity line
//!  ▶ cargo   ~ › Desktop › Kraken   ⎇ main +2 ~5 ↑1   ● C  ◆ X   ⠹ 12.4s · 38% · 412 MB   22:16
//! ```
//!
//! The activity line along the top edge comes alive while a command runs (a
//! comet of light sweeps along it) and flashes green or red when one ends.

use kraken_config::{Rgb, Theme};
use kraken_render::{FontKind, Frame, Rect, Span, TextLine};
use kraken_term::status::GitInfo;
use std::time::Duration;

pub const STATUS_PT: f32 = 28.0;

/// Braille spinner frames for a running command.
const SPINNER: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
/// How long a finished command's ✓ / ✗ chip stays highlighted.
const RESULT_CHIP_SECS: f32 = 8.0;
/// How long the activity line flashes when a command ends.
const FLASH_SECS: f32 = 0.9;

// Nerd Font icons (the bundled default font has them; others fall back).
const ICON_SSH: &str = "\u{f233}";
const ICON_RUN: &str = "\u{f04b}";
const ICON_SHELL: &str = "\u{f120}";
const ICON_FOLDER: &str = "\u{f07b}";
const ICON_BRANCH: &str = "\u{e725}";
const ICON_OK: &str = "\u{f00c}";
const ICON_FAIL: &str = "\u{f00d}";

/// Everything the bar shows, gathered by the window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StatusView {
    pub program: Option<String>,
    pub ssh: Option<String>,
    pub cwd: Option<String>,
    pub git: Option<GitInfo>,
    pub running_for: Option<Duration>,
    pub last: Option<(Option<i32>, Duration)>,
    pub clock: String,
    /// Coding agents in this window: (working, waiting).
    pub agents: Option<(usize, usize)>,
    /// One entry per agent: (short name, working, asked for attention).
    pub agent_dots: Vec<(String, bool, bool)>,
    /// CPU % and memory (bytes) of the running program and its children.
    pub usage: Option<(f32, u64)>,
    /// Time since the last command finished (for the result flash and chip).
    pub finished_ago: Option<Duration>,
    /// A plain command is running in the focused pane (not an agent idling).
    pub animate: bool,
    /// Seconds on a free-running clock, for spinners and pulses. Set by the
    /// caller just before drawing (kept out of change detection).
    pub anim: f32,
}

impl StatusView {
    /// Whether the bar is animating right now (the caller keeps frames coming).
    pub fn is_animating(&self) -> bool {
        self.animate
            || self.finished_ago.is_some_and(|d| d.as_secs_f32() < FLASH_SECS)
            || self.agent_dots.iter().any(|(_, working, _)| *working)
    }
}

/// `412 MB`, `1.3 GB`.
pub fn format_bytes(b: u64) -> String {
    let mb = b as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 { format!("{:.1} GB", mb / 1024.0) } else { format!("{mb:.0} MB") }
}

/// `~/Desktop/Kraken/crates` → `[("~", false), ("Desktop", false), ("Kraken", false), ("crates", true)]`,
/// keeping the last `keep` folders (an ellipsis stands in for the rest).
pub fn crumbs(cwd: &str, keep: usize) -> Vec<(String, bool)> {
    let parts: Vec<&str> = cwd.split('/').filter(|p| !p.is_empty()).collect();
    let (root, parts) = if cwd.starts_with('~') { ("~", &parts[1.min(parts.len())..]) } else { ("/", &parts[..]) };
    let mut out = Vec::new();
    if parts.len() > keep {
        out.push(("…".to_string(), false));
    } else {
        out.push((root.to_string(), parts.is_empty()));
    }
    let start = parts.len().saturating_sub(keep);
    for (i, p) in parts[start..].iter().enumerate() {
        out.push((p.to_string(), start + i + 1 == parts.len()));
    }
    out
}

/// Geometry: bar rectangle plus the mono cell metrics to measure text.
pub struct BarGeom {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub scale: f32,
    /// Width of one monospace character at the bar's font size.
    pub char_w: f32,
    pub font_px: f32,
}

pub fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    let s = d.as_secs();
    if ms < 1000 {
        format!("{ms}ms")
    } else if s < 60 {
        format!("{:.1}s", d.as_secs_f32())
    } else if s < 3600 {
        format!("{}m {}s", s / 60, s % 60)
    } else {
        format!("{}h {}m", s / 3600, (s % 3600) / 60)
    }
}

/// Text width in mono cells: icons in the Private Use Area render 2 cells wide.
fn cells(text: &str) -> usize {
    text.chars().map(|c| if ('\u{e000}'..='\u{f8ff}').contains(&c) || c as u32 >= 0xf0000 { 2 } else { 1 }).sum()
}

/// Status-bar segments you can click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusHit {
    /// The folder: opens it in Finder.
    Folder,
    /// The git branch: shows what changed.
    Branch,
    /// The agent counts: opens the agents dashboard.
    Agents,
}

struct Segment {
    spans: Vec<(String, Rgb)>,
    /// Pill background, if any.
    bg: Option<Rgb>,
    hit: Option<StatusHit>,
}

impl Segment {
    fn plain(spans: Vec<(String, Rgb)>) -> Self {
        Segment { spans, bg: None, hit: None }
    }
    fn clickable(spans: Vec<(String, Rgb)>, hit: StatusHit) -> Self {
        Segment { spans, bg: None, hit: Some(hit) }
    }
    fn width(&self, g: &BarGeom) -> f32 {
        let n: usize = self.spans.iter().map(|(s, _)| cells(s)).sum();
        n as f32 * g.char_w + if self.bg.is_some() { 14.0 * g.scale } else { 0.0 }
    }
}

/// Draw the bar. Returns where the clickable segments ended up
/// (`hit`, x from, x to); `hover` highlights the one under the mouse.
pub fn draw(frame: &mut Frame, t: &Theme, g: &BarGeom, v: &StatusView, hover: Option<StatusHit>) -> Vec<(StatusHit, f32, f32)> {
    let mut hits = Vec::new();
    let s = g.scale;
    frame.under.push(Rect::new(g.x, g.y, g.w, g.h, rgb(t.surface)));
    activity_line(frame, t, g, v);

    // ── Left: where am I ─────────────────────────────────────────────
    let mut left = Vec::new();
    if let Some(host) = &v.ssh {
        left.push(Segment { spans: vec![(format!("{ICON_SSH} {host}"), t.background)], bg: Some(t.ssh), hit: None });
    } else if let Some(p) = &v.program {
        left.push(Segment { spans: vec![(format!("{ICON_RUN} {p}"), t.background)], bg: Some(t.accent), hit: None });
    } else {
        left.push(Segment { spans: vec![(format!("{ICON_SHELL} shell"), t.muted)], bg: Some(t.elevated), hit: None });
    }
    if let Some(cwd) = &v.cwd {
        // Breadcrumb: parents quiet, the folder you're in bright.
        let mut spans = vec![(format!("{ICON_FOLDER} "), t.normal[4])];
        for (i, (name, current)) in crumbs(cwd, 3).into_iter().enumerate() {
            if i > 0 {
                spans.push((" › ".into(), t.border.mix(t.muted, 0.5)));
            }
            spans.push((name, if current { t.foreground } else { t.muted }));
        }
        left.push(Segment::clickable(spans, StatusHit::Folder));
    }
    if let Some(git) = &v.git {
        let mut spans = vec![(format!("{ICON_BRANCH} {}", git.branch), t.normal[5])];
        let (mut new, mut changed, mut deleted) = (0, 0, 0);
        for kind in git.changes.values() {
            match kind {
                '?' | 'A' => new += 1,
                'D' => deleted += 1,
                _ => changed += 1,
            }
        }
        if git.dirty == 0 {
            spans.push(("  ✓".into(), t.normal[2]));
        } else {
            for (n, sign, c) in [(new, "+", t.normal[2]), (changed, "~", t.normal[3]), (deleted, "−", t.normal[1])] {
                if n > 0 {
                    spans.push((format!(" {sign}{n}"), c));
                }
            }
        }
        if git.ahead > 0 {
            spans.push((format!(" ↑{}", git.ahead), t.normal[6]));
        }
        if git.behind > 0 {
            spans.push((format!(" ↓{}", git.behind), t.normal[1]));
        }
        left.push(Segment::clickable(spans, StatusHit::Branch));
    }

    // ── Right: what's happening, how did it go ───────────────────────
    // Laid out right to left: the clock is pinned to the far right.
    let mut right = vec![Segment::plain(vec![(v.clock.clone(), t.muted)])];
    if let Some(d) = v.running_for {
        let frame_i = (v.anim * 12.0) as usize % SPINNER.len();
        let spin = if v.animate { SPINNER[frame_i] } else { '◷' };
        let mut spans = vec![(format!("{spin} {}", format_duration(d)), t.accent)];
        if let Some((cpu, rss)) = v.usage {
            // CPU turns warm, then hot, as it climbs.
            let heat = if cpu >= 300.0 {
                t.normal[1]
            } else if cpu >= 100.0 {
                t.normal[3]
            } else {
                t.muted
            };
            spans.push((format!("  {cpu:.0}%"), heat));
            spans.push((format!("  {}", format_bytes(rss)), t.muted));
        }
        right.push(Segment::plain(spans));
    } else if let Some((code, d)) = v.last {
        let ok = code.unwrap_or(0) == 0;
        let fresh = v.finished_ago.is_some_and(|a| a.as_secs_f32() < RESULT_CHIP_SECS);
        let text = match code {
            Some(c) if c != 0 => format!("{ICON_FAIL} exit {c}  {}", format_duration(d)),
            _ => format!("{ICON_OK} {}", format_duration(d)),
        };
        let c = if ok { t.normal[2] } else { t.normal[1] };
        // Just finished: a tinted chip. Later: quiet text.
        right.push(if fresh {
            Segment { spans: vec![(text, c)], bg: Some(t.surface.mix(c, 0.16)), hit: None }
        } else {
            Segment::plain(vec![(text, c.mix(t.muted, 0.35))])
        });
    }

    // ── Agents: one dot each, in the middle ──────────────────────────
    let mut agents = None;
    if !v.agent_dots.is_empty() {
        let mut spans = Vec::new();
        for (i, (label, working, attention)) in v.agent_dots.iter().enumerate() {
            let (working, attention) = (*working, *attention);
            if i > 0 {
                spans.push(("  ".into(), t.muted));
            }
            let (glyph, c) = if working {
                // Breathes while it works.
                let pulse = 0.55 + 0.45 * (v.anim * 3.2 + i as f32).sin().abs();
                ("●", t.surface.mix(t.normal[2], pulse))
            } else if attention {
                ("◆", t.ssh)
            } else {
                ("◆", t.ssh.mix(t.muted, 0.3))
            };
            spans.push((format!("{glyph} {label}"), c));
        }
        agents = Some(Segment::clickable(spans, StatusHit::Agents));
    }

    let pad = 10.0 * s;
    let gap = 18.0 * s;
    let mut x = g.x + pad;
    let mut left_end = x;
    for seg in &left {
        let w = seg.width(g);
        if x + w > g.x + g.w * 0.62 {
            break; // keep room for the rest
        }
        emit(frame, t, g, seg, x, hover, &mut hits);
        x += w + gap;
        left_end = x;
    }
    let mut rx = g.x + g.w - pad;
    for seg in &right {
        let w = seg.width(g);
        if rx - w < left_end {
            break;
        }
        rx -= w;
        emit(frame, t, g, seg, rx, hover, &mut hits);
        rx -= gap;
    }
    if let Some(seg) = agents {
        // Centred when there's room, otherwise just after the left side.
        let w = seg.width(g);
        let cx = (g.x + g.w / 2.0 - w / 2.0).max(left_end);
        if cx + w < rx {
            emit(frame, t, g, &seg, cx, hover, &mut hits);
        }
    }
    hits
}

/// The bar's top edge: a hairline at rest, a sweeping comet while a command
/// runs, a green or red flash when one ends.
fn activity_line(frame: &mut Frame, t: &Theme, g: &BarGeom, v: &StatusView) {
    let s = g.scale;
    let h = (1.5 * s).round().max(1.0);
    frame.under.push(Rect::new(g.x, g.y, g.w, h, rgb(t.border)));
    if v.animate {
        // A comet: bright head, fading tail, looping left to right.
        let len = g.w * 0.22;
        let head = g.x + ((v.anim * 0.42).fract() * 1.3 - 0.1) * g.w;
        let slices = 24;
        for i in 0..slices {
            let f = i as f32 / slices as f32;
            let x = head - len * f;
            let c = t.normal[6].mix(t.accent, f);
            let a = ((1.0 - f).powf(1.6) * 255.0) as u8;
            frame.under.push(Rect { x, y: g.y, w: len / slices as f32 + 1.0, h: h + s * 0.5, color: [c.0, c.1, c.2, a] });
        }
        // A faint glow under the head.
        let gc = t.normal[6];
        frame.under.push(Rect { x: head - len * 0.3, y: g.y + h, w: len * 0.3, h: 3.0 * s, color: [gc.0, gc.1, gc.2, 28] });
    }
    if let (Some(ago), Some((code, _))) = (v.finished_ago, v.last) {
        let p = ago.as_secs_f32() / FLASH_SECS;
        if p < 1.0 {
            let c = if code.unwrap_or(0) == 0 { t.normal[2] } else { t.normal[1] };
            let a = ((1.0 - p).powi(2) * 255.0) as u8;
            frame.under.push(Rect { x: g.x, y: g.y, w: g.w, h: h + s * 0.5, color: [c.0, c.1, c.2, a] });
            frame.under.push(Rect { x: g.x, y: g.y + h, w: g.w, h: 5.0 * s, color: [c.0, c.1, c.2, a / 7] });
        }
    }
}

fn emit(frame: &mut Frame, t: &Theme, g: &BarGeom, seg: &Segment, x: f32, hover: Option<StatusHit>, hits: &mut Vec<(StatusHit, f32, f32)>) {
    let s = g.scale;
    if let Some(h) = seg.hit {
        let w = seg.width(g);
        hits.push((h, x, x + w));
        if hover == Some(h) {
            // A quiet highlight says "this does something".
            let ph = g.h - 8.0 * s;
            frame.under.push(Rect {
                x: x - 5.0 * s,
                y: g.y + (g.h - ph) / 2.0 + s * 0.5,
                w: w + 10.0 * s,
                h: ph,
                color: [t.elevated.0, t.elevated.1, t.elevated.2, 255],
            });
        }
    }
    let lh = g.font_px * 1.3;
    let mut tx = x;
    if let Some(bg) = seg.bg {
        let w = seg.width(g);
        let ph = g.h - 8.0 * s;
        frame.under.push(Rect::new(x, g.y + (g.h - ph) / 2.0 + s * 0.5, w, ph, rgb(bg)));
        tx += 7.0 * s;
    }
    let _ = t;
    frame.lines.push(TextLine {
        x: tx,
        y: g.y + (g.h - lh) / 2.0 + s * 0.5,
        font: FontKind::Mono,
        size: g.font_px,
        line_height: lh,
        spans: seg
            .spans
            .iter()
            .map(|(text, c)| Span { text: text.clone(), color: rgb(*c), bold: seg.bg.is_some(), italic: false })
            .collect(),
        clip: Some([g.x as i32, g.y as i32, (g.x + g.w) as i32, (g.y + g.h) as i32]),
        cell_width: Some(g.char_w),
    });
}

fn rgb(c: Rgb) -> [u8; 3] {
    [c.0, c.1, c.2]
}

/// Local wall-clock time as HH:MM.
pub fn clock() -> String {
    // SAFETY: time(NULL) only returns the current time; it writes nothing.
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    // SAFETY: libc::tm is plain integers and a nullable pointer; all-zero is valid.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers refer to live locals; localtime_r is the thread-safe variant.
    unsafe { libc::localtime_r(&now, &mut tm) };
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(format_duration(Duration::from_millis(320)), "320ms");
        assert_eq!(format_duration(Duration::from_millis(4200)), "4.2s");
        assert_eq!(format_duration(Duration::from_secs(192)), "3m 12s");
        assert_eq!(format_duration(Duration::from_secs(3840)), "1h 4m");
    }
}

#[cfg(test)]
mod legend_tests {
    use super::*;

    #[test]
    fn breadcrumbs_keep_the_tail() {
        let c = crumbs("~/Desktop/Kraken/crates", 3);
        assert_eq!(c.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["~", "Desktop", "Kraken", "crates"]);
        assert!(c.last().unwrap().1 && !c[0].1);
        let deep = crumbs("/usr/local/share/very/deep", 3);
        assert_eq!(deep[0].0, "…");
        assert_eq!(deep.last().unwrap().0, "deep");
        assert_eq!(crumbs("~", 3), vec![("~".to_string(), true)]);
    }

    #[test]
    fn bytes_read_naturally() {
        assert_eq!(format_bytes(412 * 1024 * 1024), "412 MB");
        assert_eq!(format_bytes(1536 * 1024 * 1024), "1.5 GB");
    }
}
