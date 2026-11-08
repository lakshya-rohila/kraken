//! Emulation-core throughput, without a GPU or a PTY:
//!
//! - **parse**: bytes → Kraken's OSC/APC scanner → alacritty's parser → grid
//!   (what `cat bigfile` costs before anything is drawn), in MB/s;
//! - **frame**: grid → draw list (`view::build`) for a full 200×60 screen, in
//!   µs per frame — the budget at 120 Hz is 8333 µs for *everything*.
//!
//! `cargo bench -p kraken-term --bench throughput` (release build).
//! Set `KRAKEN_BENCH_MB` to change the input size (default 64).

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use kraken_render::Frame;
use kraken_term::shell::Rewriter;
use kraken_term::view::{self, Layout};
use std::hint::black_box;
use std::time::Instant;

const COLS: usize = 200;
const ROWS: usize = 60;

struct Size;
impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        ROWS
    }
    fn screen_lines(&self) -> usize {
        ROWS
    }
    fn columns(&self) -> usize {
        COLS
    }
}

fn workload(kind: &str, bytes: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes + 4096);
    let mut i = 0u64;
    while out.len() < bytes {
        i += 1;
        let line = match kind {
            // Like `cat` of source code.
            "ascii" => format!("    let value_{i} = compute(&input[{i}..], {}) + offset; // comment text here\r\n", i % 97),
            // Like `ls --color`, compiler diagnostics, `git log --graph`.
            "sgr" => format!(
                "\x1b[1;34mdir_{i}\x1b[0m  \x1b[32mfile.rs\x1b[0m \x1b[38;5;{}m{i:>8}\x1b[0m \x1b[38;2;{};{};200mtruecolor\x1b[0m \x1b[4:3;58;5;1mwarn\x1b[0m\r\n",
                i % 256,
                i % 255,
                (i * 7) % 255
            ),
            // CJK, emoji and combining marks.
            "unicode" => format!("中文字符 {i} 🦑🐙 kraken é\u{301} — ünïcødé ✓ ✗ → ← 日本語テキスト\r\n"),
            // A full-screen app redrawing (cursor addressing, like htop/vim).
            "tui" => {
                let row = i % ROWS as u64 + 1;
                format!("\x1b[{row};1H\x1b[7m CPU {:>3}% \x1b[0m\x1b[K {}", i % 100, "▇".repeat((i % 60) as usize))
            }
            _ => unreachable!(),
        };
        out.extend_from_slice(line.as_bytes());
    }
    out
}

fn main() {
    let mb: usize = std::env::var("KRAKEN_BENCH_MB").ok().and_then(|v| v.parse().ok()).unwrap_or(64);
    let theme = kraken_config::Theme::default();
    let layout = Layout { x: 0.0, y: 0.0, cell_w: 9.0, cell_h: 19.0, font_px: 14.0, focused: true, cursor_visible: true };
    println!("{:<8} {:>12} {:>12} {:>14}", "workload", "parse MB/s", "+scan MB/s", "frame µs");
    for kind in ["ascii", "sgr", "unicode", "tui"] {
        let input = workload(kind, mb * 1024 * 1024);
        let mb_f = input.len() as f64 / (1024.0 * 1024.0);

        // Parse only (alacritty's parser + grid).
        let mut term = Term::new(Config { scrolling_history: 10_000, ..Config::default() }, &Size, VoidListener);
        let mut parser: Processor = Processor::new();
        let t = Instant::now();
        for chunk in input.chunks(64 * 1024) {
            parser.advance(&mut term, chunk);
        }
        let parse = mb_f / t.elapsed().as_secs_f64();

        // Kraken's scanner in front, as on the real PTY path.
        let mut term = Term::new(Config { scrolling_history: 10_000, ..Config::default() }, &Size, VoidListener);
        let mut parser: Processor = Processor::new();
        let mut rw = Rewriter::new(None);
        let mut buf = Vec::with_capacity(64 * 1024);
        let t = Instant::now();
        for chunk in input.chunks(64 * 1024) {
            buf.clear();
            rw.feed(chunk, &mut buf, |_| {});
            parser.advance(&mut term, &buf);
        }
        let scan = mb_f / t.elapsed().as_secs_f64();

        // Draw-list build for the final full screen.
        let mut frame = Frame::default();
        let mut marks = Vec::new();
        let n = 300;
        let t = Instant::now();
        for _ in 0..n {
            frame.clear_all();
            marks.clear();
            black_box(view::build(&term, &theme, &layout, &mut frame, &mut marks));
        }
        let frame_us = t.elapsed().as_secs_f64() * 1e6 / n as f64;
        println!("{kind:<8} {parse:>12.0} {scan:>12.0} {frame_us:>14.0}");
    }
}
