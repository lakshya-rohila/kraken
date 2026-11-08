//! The welcome screen: something rises from the abyss.
//!
//! ```text
//! 0.00  abyss · marine snow drifting · a faint heartbeat glow far below
//! 0.60  two eyes open in the dark, deep down · blink · pupils dart, then lock on you
//! 1.35  the body rises toward you, lit from below · tentacles unfurl and writhe
//! 2.45  roar: eyes flare · pressure shockwave · shake · KRAKEN slams in, white-hot
//! 2.80  hold: the body breathes · one letter flickers · the subtitle types itself
//! 4.00  it inks: pixel ink blooms out of the head and swallows the screen
//! 4.35  the ink dissolves from the edges inward · the eyes are the last to go
//! ```
//!
//! Everything is drawn as rectangles and mono text, coloured from the theme.
//! Any key or click skips it; `animation_speed = 0` turns it off.

use crate::anim::{ease_out_back, ease_out_cubic};
use kraken_config::{Rgb, Theme};
use kraken_mux::layout::R;
use kraken_render::{FontKind, Frame, Rect, Span, TextLine};

// Beats, in seconds at speed 1.
const EYES_OPEN: f32 = 0.6;
const BLINK: f32 = 1.12;
const RISE_START: f32 = 1.35;
const ROAR: f32 = 2.45;
const INK_START: f32 = 4.0;
const INK_COVER: f32 = 4.35;
pub const TOTAL: f32 = 4.9;

/// The mantle. Every line is symmetric, so centring it per line keeps it
/// aligned. `●` marks the eye sockets; the eyes themselves are drawn apart.
const HEAD: &[&str] = &[
    "▄▄████████▄▄",
    "▄████████████████▄",
    "██████████████████████",
    "████▀▀  ▀████▀  ▀▀████",
    "███   ●  ████  ●   ███",
    "████▄▄  ▄████▄  ▄▄████",
    "▀██████████████████▀",
    "▀████████████████▀",
];
const EYE_ROW: usize = 4;

const TENTACLES: usize = 8;
const TENTACLE_ROWS: usize = 8;

/// "KRAKEN" in a block-letter font, built letter by letter so columns line up.
fn title_rows() -> Vec<String> {
    let k = ["██╗  ██╗", "██║ ██╔╝", "█████╔╝ ", "██╔═██╗ ", "██║  ██╗", "╚═╝  ╚═╝"];
    let r = ["██████╗ ", "██╔══██╗", "██████╔╝", "██╔══██╗", "██║  ██║", "╚═╝  ╚═╝"];
    let a = [" █████╗ ", "██╔══██╗", "███████║", "██╔══██║", "██║  ██║", "╚═╝  ╚═╝"];
    let e = ["███████╗", "██╔════╝", "█████╗  ", "██╔══╝  ", "███████╗", "╚══════╝"];
    let n = ["███╗   ██╗", "████╗  ██║", "██╔██╗ ██║", "██║╚██╗██║", "██║ ╚████║", "╚═╝  ╚═══╝"];
    (0..6).map(|i| [k[i], r[i], a[i], k[i], e[i], n[i]].join("")).collect()
}
/// Columns of the "E" in the title: the letter whose neon is failing.
const FLICKER_COLS: std::ops::Range<usize> = 40..48;

const SUBTITLE: &str = "one creature  ·  many arms";
const HINT: &str = "press any key  ·  ⌘/ for shortcuts";

/// Deterministic noise in 0..1 (no RNG state: every frame agrees).
fn hash(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77) ^ 0xC2B2_AE3D;
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// 0 → 1 over `[a, b]`.
fn span(t: f32, a: f32, b: f32) -> f32 {
    clamp01((t - a) / (b - a))
}

fn smooth(x: f32) -> f32 {
    let x = clamp01(x);
    x * x * (3.0 - 2.0 * x)
}

/// A grid of coloured characters we paint into, then turn into text lines.
struct Canvas {
    cols: usize,
    rows: Vec<Vec<(char, Rgb)>>,
}

impl Canvas {
    fn new(cols: usize, rows: usize, bg: Rgb) -> Self {
        Canvas { cols, rows: vec![vec![(' ', bg); cols]; rows] }
    }
    fn put(&mut self, row: usize, col: i32, ch: char, c: Rgb) {
        if col >= 0 && (col as usize) < self.cols && row < self.rows.len() {
            self.rows[row][col as usize] = (ch, c);
        }
    }
    fn text(&mut self, row: usize, col: i32, s: &str, color: impl Fn(usize, char) -> Rgb) {
        for (i, ch) in s.chars().enumerate() {
            if ch != ' ' {
                self.put(row, col + i as i32, ch, color(i, ch));
            }
        }
    }
}

/// Three-stop gradient: cyan → violet → coral.
fn gradient(t: &Theme, x: f32) -> Rgb {
    let (cyan, violet, coral) = (t.normal[6], t.accent, t.normal[1]);
    if x < 0.5 { cyan.mix(violet, x * 2.0) } else { violet.mix(coral, (x - 0.5) * 2.0) }
}

pub struct SplashGeom {
    pub area: R,
    pub cell_w: f32,
    pub cell_h: f32,
    pub font_px: f32,
}

fn rect(frame: &mut Frame, x: f32, y: f32, w: f32, h: f32, c: Rgb, a: u8) {
    frame.overlay_rects.push(Rect { x, y, w, h, color: [c.0, c.1, c.2, a] });
}

/// A soft ellipse as stacked horizontal strips (pixel-art friendly, no square
/// corners). Several of these with low alpha make a glow.
#[allow(clippy::too_many_arguments)]
fn ellipse(frame: &mut Frame, cx: f32, cy: f32, rx: f32, ry: f32, strip: f32, c: Rgb, alpha: u8) {
    if rx <= 0.0 || ry <= 0.0 || alpha == 0 {
        return;
    }
    let n = ((2.0 * ry / strip).ceil() as i32).clamp(1, 48);
    let h = 2.0 * ry / n as f32;
    for i in 0..n {
        let v = ((i as f32 + 0.5) * h - ry) / ry; // -1..1
        let w = rx * (1.0 - v * v).max(0.0).sqrt();
        rect(frame, (cx - w).round(), (cy - ry + i as f32 * h).round(), (2.0 * w).round(), h.ceil(), c, alpha);
    }
}

/// Which part of the canvas a row belongs to (each has its own visibility).
#[derive(Clone, Copy, PartialEq)]
enum Part {
    Head,
    Gap,
    Title,
    Words,
}

/// Draw the welcome screen at `secs` seconds (already speed-scaled).
/// Returns false once it is over.
pub fn draw(frame: &mut Frame, t: &Theme, g: &SplashGeom, secs: f32) -> bool {
    if secs >= TOTAL {
        return false;
    }
    let a = g.area;
    let abyss = t.background.mix(Rgb(0, 0, 0), 0.55);
    let ink = Rgb(4, 2, 10).mix(t.accent, 0.06);
    // Blood-orange, hotter than any theme colour: it should feel like a warning.
    let eye_hot = Rgb(255, 58, 24).mix(t.normal[1], 0.15);
    let eye_core = Rgb(255, 214, 120);
    let biolume = t.normal[6];
    let unit = (g.cell_w / 9.0).max(1.0); // one "pixel" of the art

    // ── Camera ────────────────────────────────────────────────────────────
    let since_roar = secs - ROAR;
    let shake = if since_roar > 0.0 { g.cell_w * 0.7 * (-since_roar * 6.5).exp() } else { 0.0 };
    let (sx, sy) = (shake * (secs * 93.0).sin(), shake * (secs * 71.0).cos());

    // ── Layout ────────────────────────────────────────────────────────────
    let cols = (a.w / g.cell_w).floor().max(1.0) as usize;
    let title = title_rows();
    let title_w = title[0].chars().count();
    let big_title = cols >= title_w + 4;
    let title_h = if big_title { title.len() } else { 1 };
    let total_rows = HEAD.len() + TENTACLE_ROWS + 1 + title_h + 1 + 1 + 1;
    let art_h = total_rows as f32 * g.cell_h;
    let y0 = (a.y + (a.h - art_h) / 2.0).max(a.y) + sy;
    let x0 = a.x + (a.w - cols as f32 * g.cell_w) / 2.0 + sx;
    let center = cols as f32 / 2.0;
    let centered = |s: &str| (center - s.chars().count() as f32 / 2.0).round() as i32;

    // The creature starts deep (low on screen, unlit) and rises toward you;
    // at the roar it recoils upward a little, then settles.
    let rise = smooth(span(secs, RISE_START, ROAR).powf(0.8));
    let recoil = if since_roar > 0.0 { -g.cell_h * 0.6 * (since_roar * 9.0).sin().max(0.0) * (-since_roar * 5.0).exp() } else { 0.0 };
    let depth = (1.0 - rise) * a.h * 0.38 + recoil;
    // Body light: dark while deep, lit from below as it rises, breathing after.
    let breath = 0.92 + 0.08 * (secs * 2.2).sin();
    let light = smooth(span(secs, RISE_START - 0.05, ROAR - 0.25)) * if since_roar > 0.0 { breath } else { 1.0 };
    let head_cx = x0 + center * g.cell_w;
    let head_cy = y0 + depth + HEAD.len() as f32 * g.cell_h * 0.5;

    let inked = secs >= INK_COVER; // the ink is all that's left on screen

    // ── Backdrop: the abyss ───────────────────────────────────────────────
    if !inked {
        rect(frame, a.x, a.y, a.w, a.h, abyss, 255);

        // A heartbeat glow far below, before anything is visible.
        let beat = |p: f32| (-((p % 1.05) * 9.0)).exp() + 0.6 * (-(((p + 0.78) % 1.05) * 9.0)).exp();
        let glow = (1.0 - rise) * clamp01(secs / 0.5) * (0.35 + 0.65 * beat(secs));
        // Many faint ellipses: each adds a little light, so the edge melts away.
        for i in 0..10 {
            let r = (10 - i) as f32 * g.cell_w * 1.6;
            let alpha = (glow * 6.0) as u8;
            ellipse(frame, head_cx, head_cy + g.cell_h * 2.0, r * 1.3, r * 0.6, unit * 3.0, t.accent.mix(eye_hot, 0.2), alpha);
        }

        // Marine snow: drifting upward, blown outward by the roar's shockwave.
        let push = if since_roar > 0.0 { a.w * 0.35 * ease_out_cubic(clamp01(since_roar / 0.9)) } else { 0.0 };
        for i in 0..120u32 {
            let speed = 0.02 + hash(i, 1) * 0.05;
            let px = a.x + hash(i, 2) * a.w + (secs * 1.3 + i as f32).sin() * unit * 3.0;
            let py = a.y + (hash(i, 3) - secs * speed).rem_euclid(1.0) * a.h;
            let (dx, dy) = (px - head_cx, py - head_cy);
            let d = (dx * dx + dy * dy).sqrt().max(1.0);
            let (px, py) = (px + dx / d * push * (1.0 - (d / a.w).min(1.0)), py + dy / d * push * (1.0 - (d / a.w).min(1.0)));
            let size = unit * (1.0 + (hash(i, 4) * 2.5).floor());
            let twinkle = 0.25 + 0.75 * hash(i, (secs * 3.0) as u32 + 7) * hash(i, 5);
            let c = abyss.mix(biolume.mix(Rgb(220, 230, 255), 0.5), 0.12 + twinkle * 0.25);
            rect(frame, px.round(), py.round(), size, size, c, 255);
        }
    }

    // ── Canvas: head, title, words ────────────────────────────────────────
    let mut canvas = Canvas::new(cols, total_rows, abyss);
    let mut parts = vec![Part::Gap; total_rows];
    let mut eye_cells: Vec<i32> = Vec::new();
    for (i, line) in HEAD.iter().enumerate() {
        parts[i] = Part::Head;
        // Lit from below, like a torch under the chin: the crown stays in shadow.
        let from_below = i as f32 / (HEAD.len() - 1) as f32;
        let lit = light * (0.28 + 0.72 * from_below.powf(0.8));
        let base = t.accent.mix(t.normal[5], i as f32 / HEAD.len() as f32);
        let c = abyss.mix(base, lit);
        let x = centered(line);
        if i == EYE_ROW {
            eye_cells.extend(line.chars().enumerate().filter(|(_, ch)| *ch == '●').map(|(j, _)| x + j as i32));
        }
        // The sockets stay empty: text draws above rectangles, so a glyph
        // here would cover the eyes drawn later.
        canvas.text(i, x, &line.replace('●', " "), |_, _| c);
    }
    let title_row = HEAD.len() + TENTACLE_ROWS + 1;
    let slam = span(secs, ROAR - 0.06, ROAR + 0.22);
    let heat = if since_roar > 0.0 { (-since_roar * 4.0).exp() } else { 0.0 };
    // The shockwave lights the title as it passes over each column.
    let wave_r = since_roar.max(0.0) * a.w * 0.9;
    let flicker_off = since_roar > 0.6 && hash((secs * 16.0) as u32, 99) < 0.14;
    if big_title {
        let tx = centered(&title[0]);
        for (i, line) in title.iter().enumerate() {
            parts[title_row + i] = Part::Title;
            canvas.text(title_row + i, tx, line, |col, ch| {
                let base = gradient(t, col as f32 / title_w as f32);
                let cx = x0 + (tx as f32 + col as f32) * g.cell_w - head_cx;
                let cy = y0 + (title_row + i) as f32 * g.cell_h - head_cy;
                let dist = (cx * cx + cy * cy).sqrt();
                let wave = (1.0 - ((dist - wave_r).abs() / (g.cell_w * 6.0))).max(0.0) * heat.max(0.25);
                let mut c = base.mix(Rgb(255, 255, 255), (heat * 0.85 + wave * 0.6).min(0.95));
                if flicker_off && FLICKER_COLS.contains(&col) {
                    c = c.mix(abyss, 0.78);
                }
                if ch == '█' { c } else { c.mix(abyss, 0.5) }
            });
        }
    } else {
        parts[title_row] = Part::Title;
        let small = "K R A K E N";
        canvas.text(title_row, centered(small), small, |i, _| gradient(t, i as f32 / 10.0).mix(Rgb(255, 255, 255), heat * 0.8));
    }
    let sub_row = title_row + title_h + 1;
    parts[sub_row] = Part::Words;
    parts[sub_row + 1] = Part::Words;
    let typed = ((secs - ROAR - 0.45) * 34.0).max(0.0) as usize;
    let sub: String = SUBTITLE.chars().take(typed).collect();
    let caret = if typed < SUBTITLE.chars().count() && typed > 0 { "▌" } else { "" };
    canvas.text(sub_row, centered(SUBTITLE), &format!("{sub}{caret}"), |_, _| t.foreground.mix(abyss, 0.25));
    if secs > ROAR + 1.15 {
        canvas.text(sub_row + 1, centered(HINT), HINT, |_, _| t.muted.mix(abyss, 0.15));
    }

    // Text fades away as the ink takes the screen.
    let ink_grow = smooth(span(secs, INK_START, INK_COVER));
    let words_vis = 1.0 - ink_grow;

    // ── Tentacles: unfurl, writhe, grasp ──────────────────────────────────
    if !inked && light > 0.0 {
        let tent_top = y0 + depth + HEAD.len() as f32 * g.cell_h - g.cell_h * 0.2;
        let steps = 60;
        for k in 0..TENTACLES {
            let lane = k as f32 - (TENTACLES as f32 - 1.0) / 2.0;
            let unfurl = smooth(span(secs, RISE_START + 0.15 + k as f32 * 0.04, ROAR + 0.15));
            let reach = (steps as f32 * unfurl) as usize;
            // Every few seconds a tentacle clenches, like it's grabbing at you.
            let grasp = (secs * 1.25 + k as f32 * 1.7).sin().max(0.0).powi(8);
            for i in 0..reach {
                let u = i as f32 / (steps - 1) as f32; // 0 at the body, 1 at the tip
                let row = u * TENTACLE_ROWS as f32;
                let phase = secs * 3.3 + k as f32 * 0.9;
                let spread = lane * (1.6 + row * 1.05);
                let writhe = u.powf(1.5) * 3.0 * (phase - u * 4.6).sin() + u * 0.6 * (secs * 11.0 + k as f32 + u * 9.0).sin();
                let curl = u.powi(3) * (1.4 + 3.2 * grasp) * (phase * 0.6 + k as f32).cos();
                let cx = x0 + (center + spread + writhe + curl) * g.cell_w;
                let cy = tent_top + row * g.cell_h - u.powi(3) * grasp * g.cell_h * 1.5;
                let size = g.cell_w * (1.35 - 1.05 * u);
                let base = gradient(t, u * 0.55 + (k as f32 / TENTACLES as f32) * 0.45);
                let c = abyss.mix(base, light * (0.55 + 0.45 * (1.0 - u)));
                rect(frame, cx - size / 2.0, cy, size, size, c, 255);
                if i % 6 == 3 && u < 0.8 {
                    let d = size * 0.34;
                    rect(frame, cx - d / 2.0, cy + size / 2.0 - d / 2.0, d, d, abyss.mix(t.accent, light * 0.25), 255);
                }
            }
        }
    }

    // ── Shockwave: pressure rings tearing outward ─────────────────────────
    if !inked && since_roar > 0.0 && since_roar < 1.0 {
        let fade = (1.0 - since_roar).powf(1.5);
        for (ring, lag, thick) in [(0u32, 0.0f32, 5.0f32), (1, 0.07, 3.0), (2, 0.16, 2.0)] {
            let r = ease_out_cubic(clamp01((since_roar - lag) / 0.9)) * a.w * 0.75;
            if r <= 0.0 {
                continue;
            }
            let n = 220;
            for i in 0..n {
                let ang = i as f32 / n as f32 * std::f32::consts::TAU;
                // Ragged, like water being shoved, not a clean circle.
                let wob = 1.0 + 0.06 * (ang * 7.0 + secs * 20.0 + ring as f32).sin() + 0.05 * hash(i, ring + 11);
                let (px, py) = (head_cx + ang.cos() * r * wob, head_cy + ang.sin() * r * 0.6 * wob);
                let sz = unit * thick * (0.6 + hash(i, ring + 31));
                let alpha = (fade * [230.0, 150.0, 90.0][ring as usize]) as u8;
                let c = biolume.mix(Rgb(255, 255, 255), 0.5 - ring as f32 * 0.2);
                rect(frame, px - sz / 2.0, py - sz / 2.0, sz, sz, c, alpha);
            }
        }
    }

    // ── Emit canvas rows ──────────────────────────────────────────────────
    if !inked {
        for (i, row) in canvas.rows.iter().enumerate() {
            let (vis, dy) = match parts[i] {
                Part::Head => (if light > 0.0 { 1.0 } else { 0.0 }, depth),
                Part::Gap => (0.0, 0.0),
                Part::Title => {
                    // Slams down from above with a little overshoot.
                    let drop = (1.0 - ease_out_back(slam)) * -g.cell_h * 2.5;
                    (if slam > 0.0 { words_vis } else { 0.0 }, drop)
                }
                Part::Words => (words_vis, 0.0),
            };
            if vis <= 0.0 {
                continue;
            }
            let ry = y0 + i as f32 * g.cell_h + dy;
            let mut spans: Vec<Span> = Vec::new();
            for (col, &(ch, c)) in row.iter().enumerate() {
                let c = abyss.mix(c, vis);
                let block = match ch {
                    '█' => Some((0.0, 1.0)),
                    '▀' => Some((0.0, 0.5)),
                    '▄' => Some((0.5, 0.5)),
                    _ => None,
                };
                let ch = if let Some((off, frac)) = block {
                    rect(frame, x0 + col as f32 * g.cell_w, ry + off * g.cell_h, g.cell_w + 0.5, frac * g.cell_h + 0.5, c, 255);
                    ' '
                } else {
                    ch
                };
                let color = [c.0, c.1, c.2];
                match spans.last_mut() {
                    Some(s) if s.color == color => s.text.push(ch),
                    _ => spans.push(Span { text: ch.to_string(), color, bold: false, italic: false }),
                }
            }
            frame.overlay_lines.push(TextLine {
                x: x0,
                y: ry,
                font: FontKind::Mono,
                size: g.font_px,
                line_height: g.cell_h,
                spans,
                clip: Some([a.x as i32, a.y as i32, (a.x + a.w) as i32, (a.y + a.h) as i32]),
                cell_width: Some(g.cell_w),
            });
        }
    }

    // ── Ink: blooms from the head, covers everything, dissolves inward ────
    if secs >= INK_START {
        let bs = (g.cell_w * 0.8).max(6.0);
        let (nx, ny) = ((a.w / bs).ceil() as u32 + 1, (a.h / bs).ceil() as u32 + 1);
        let max_r = (a.w * a.w + a.h * a.h).sqrt();
        let blobs: Vec<(f32, f32, f32)> = (0..12u32)
            .map(|b| {
                let ang = hash(b, 21) * std::f32::consts::TAU;
                let out = ink_grow * a.w * 0.3 * hash(b, 22);
                let r = ink_grow * max_r * (0.35 + 0.5 * hash(b, 23));
                (head_cx + ang.cos() * out, head_cy + ang.sin() * out * 0.7, r)
            })
            .collect();
        let dissolve = span(secs, INK_COVER, TOTAL - 0.05);
        for by in 0..ny {
            for bx in 0..nx {
                let (px, py) = (a.x + bx as f32 * bs, a.y + by as f32 * bs);
                let n = hash(bx, by + 1000);
                if !inked {
                    let edge = n * bs * 3.0;
                    let inside = blobs.iter().any(|&(cx, cy, r)| {
                        let (dx, dy) = (px - cx, py - cy);
                        dx * dx + dy * dy < (r - edge).max(0.0).powi(2)
                    });
                    if !inside {
                        continue;
                    }
                } else {
                    // Clears from the edges toward the eyes, with a dithered front.
                    let (dx, dy) = ((px - head_cx) / a.w, (py - head_cy) / a.h);
                    let centrality = 1.0 - (dx * dx + dy * dy).sqrt().min(0.75) / 0.75;
                    if n * 0.45 + centrality * 0.55 < dissolve * 1.05 {
                        continue;
                    }
                }
                let shade = ink.mix(t.accent, n * 0.05);
                rect(frame, px, py, bs + 0.5, bs + 0.5, shade, 255);
            }
        }
    }

    // ── Eyes: open first, deep in the dark; last to leave ─────────────────
    let opening = smooth(span(secs, EYES_OPEN, EYES_OPEN + 0.35));
    let blink = 1.0 - (1.0 - ((secs - BLINK) / 0.07).abs()).max(0.0); // a quick shut-and-open
    let closing = 1.0 - smooth(span(secs, INK_COVER + 0.28, INK_COVER + 0.42));
    let open = opening * blink * closing;
    if open > 0.0 && eye_cells.len() == 2 {
        let ey = y0 + depth + EYE_ROW as f32 * g.cell_h + g.cell_h * 0.5;
        // They search, then lock on you.
        let look = if secs < RISE_START + 0.4 {
            let p = span(secs, EYES_OPEN + 0.35, RISE_START + 0.4);
            (p * 9.0).sin() * (1.0 - p) * 0.6
        } else {
            0.0
        };
        let flare = if since_roar > 0.0 { (-since_roar * 3.0).exp() } else { 0.0 };
        let pulse = 0.85 + 0.15 * (secs * 5.0).sin();
        for &col in &eye_cells {
            let ex = x0 + (col as f32 + 0.5) * g.cell_w;
            let (ew, eh) = (g.cell_w * 3.1, g.cell_h * 1.05);
            // Glow in the water: soft ellipses, brighter when it roars.
            for i in 0..8 {
                let grow = 3.6 - i as f32 * 0.32;
                let al = (13.0 * pulse * (0.5 + 0.5 * open) * (1.0 + flare * 2.0)).min(255.0) as u8;
                ellipse(frame, ex, ey, ew * grow * 0.5, eh * grow * 0.45 * open.max(0.3), unit * 2.0, eye_hot, al);
            }
            // The eye: an almond built from pixel rows, hotter towards the middle.
            let rows = 9;
            let row_h = (eh / rows as f32).max(1.0);
            for r in 0..rows {
                let v = (r as f32 + 0.5) / rows as f32 * 2.0 - 1.0; // -1 top … 1 bottom
                let lid = 1.0 - v.abs() / open.max(0.001);
                if lid <= 0.0 {
                    continue;
                }
                // Almond: pointed corners, slightly heavier lower lid.
                let w = ew * (1.0 - v * v).max(0.0).powf(0.7) * if v > 0.0 { 1.0 } else { 0.92 };
                let core = (1.0 - v.abs()).powf(2.0);
                let c = eye_hot.mix(eye_core, core * 0.7 + flare * 0.3);
                let y = ey + v * eh / 2.0 - row_h / 2.0;
                rect(frame, (ex - w / 2.0).round(), y.round(), w.round(), row_h.ceil(), c, 255);
            }
            // A slit pupil, narrowing when it roars.
            let pw = g.cell_w * (0.5 - 0.32 * smooth(span(secs, ROAR - 0.1, ROAR + 0.25)));
            let ph = eh * 0.95 * open;
            let px = ex + look * ew * 0.3 - pw / 2.0;
            rect(frame, px.round(), (ey - ph / 2.0).round(), pw.round().max(2.0), ph.round(), Rgb(0, 0, 0), 255);
        }
    }
    // The roar floods the water with the eyes' light for a moment.
    if since_roar > 0.0 && since_roar < 0.2 && !inked {
        let al = (48.0 * (1.0 - since_roar / 0.2).powi(2)) as u8;
        rect(frame, a.x, a.y, a.w, a.h, eye_hot.mix(Rgb(60, 0, 0), 0.4), al);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_at(secs: f32) -> (Frame, bool) {
        let mut f = Frame::default();
        let g = SplashGeom { area: R { x: 0.0, y: 0.0, w: 2200.0, h: 1300.0 }, cell_w: 18.0, cell_h: 38.0, font_px: 28.0 };
        let alive = draw(&mut f, &Theme::default(), &g, secs);
        (f, alive)
    }

    #[test]
    fn runs_every_beat_and_ends() {
        for secs in [0.0, 0.3, 0.8, 1.12, 1.6, 2.2, 2.45, 2.6, 3.2, 4.1, 4.4, 4.7] {
            let (f, alive) = frame_at(secs);
            assert!(alive, "{secs}");
            assert!(!f.overlay_rects.is_empty(), "nothing drawn at {secs}");
            // Stays cheap enough for 120 Hz (one instanced draw).
            assert!(f.overlay_rects.len() < 60_000, "{} rects at {secs}", f.overlay_rects.len());
        }
        assert!(!frame_at(TOTAL).1);
    }

    #[test]
    fn eyes_come_before_the_body_and_title_waits_for_the_roar() {
        // Before it rises: no title text, no body light.
        let (early, _) = frame_at(0.9);
        assert!(early.overlay_lines.iter().all(|l| l.spans.iter().all(|s| s.text.trim().is_empty())));
        // After the roar the wordmark and subtitle are up.
        let (late, _) = frame_at(3.6);
        let text: String = late.overlay_lines.iter().flat_map(|l| l.spans.iter().map(|s| s.text.as_str())).collect();
        assert!(text.contains("one creature"), "{text}");
    }

    #[test]
    fn the_ink_hides_the_shell_then_lets_it_through() {
        let coverage = |secs: f32| {
            let (f, _) = frame_at(secs);
            f.overlay_rects.iter().map(|r| (r.w * r.h) as f64).sum::<f64>()
        };
        let screen = 2200.0 * 1300.0;
        assert!(coverage(4.36) >= screen * 0.95, "ink must cover the screen when the backdrop goes");
        assert!(coverage(4.85) < coverage(4.5), "and then dissolve");
    }
}
