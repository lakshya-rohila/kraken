//! Golden draw-list tests.
//!
//! Each case feeds escape sequences into a real `alacritty_terminal::Term`,
//! builds Kraken's draw list with `view::build` (exactly what the renderer
//! receives), and compares a text dump of it with `tests/golden/<case>.txt`.
//! No GPU is needed, so these run anywhere, including CI.
//!
//! The VT cases follow the themes of vttest: cursor movement, scrolling
//! regions, insert/delete, autowrap, tab stops, DECALN, origin mode, erase,
//! the alternate screen, wide characters and attributes.
//!
//! After an intentional change: `UPDATE_GOLDEN=1 cargo test -p kraken-term --test golden`,
//! then review the diff of `tests/golden/` before committing it.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use kraken_render::Frame;
use kraken_term::view::{self, Layout};
use std::fmt::Write as _;
use std::path::PathBuf;

const COLS: usize = 24;
const ROWS: usize = 8;

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

fn render(bytes: &[u8]) -> String {
    let mut term = Term::new(Config::default(), &Size, VoidListener);
    let mut parser: Processor = Processor::new();
    parser.advance(&mut term, bytes);
    let theme = kraken_config::Theme::default();
    let layout = Layout { x: 0.0, y: 0.0, cell_w: 10.0, cell_h: 20.0, font_px: 14.0, focused: true, cursor_visible: true };
    let mut frame = Frame::default();
    let mut marks = Vec::new();
    let cursor = view::build(&term, &theme, &layout, &mut frame, &mut marks);
    dump(&frame, cursor, &theme)
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// A readable, stable dump: the screen text, then every styled span, then
/// rectangles (backgrounds/cursor under the text, decorations over it).
fn dump(frame: &Frame, cursor: Option<kraken_render::Rect>, theme: &kraken_config::Theme) -> String {
    let fg = hex([theme.foreground.0, theme.foreground.1, theme.foreground.2]);
    let mut out = String::from("screen:\n");
    for (row, line) in frame.lines.iter().enumerate() {
        let text: String = line.spans.iter().map(|s| s.text.as_str()).collect();
        let _ = writeln!(out, "{row:02}|{}|", text.trim_end());
    }
    out.push_str("styles:\n");
    for (row, line) in frame.lines.iter().enumerate() {
        for s in &line.spans {
            let color = hex(s.color);
            if s.text.trim().is_empty() || (color == fg && !s.bold && !s.italic) {
                continue;
            }
            let flags = format!("{}{}", if s.bold { " bold" } else { "" }, if s.italic { " italic" } else { "" });
            let _ = writeln!(out, "  r{row} {:?} {color}{flags}", s.text.trim_end());
        }
    }
    let rect = |r: &kraken_render::Rect| {
        format!("{},{} {}x{} {}", r.x.round(), r.y.round(), r.w.round(), r.h.round(), hex([r.color[0], r.color[1], r.color[2]]))
    };
    out.push_str("under:\n");
    for r in &frame.under {
        let _ = writeln!(out, "  {}", rect(r));
    }
    out.push_str("over:\n");
    for r in &frame.over {
        let _ = writeln!(out, "  {}", rect(r));
    }
    let _ = writeln!(out, "cursor: {}", cursor.map(|c| rect(&c)).unwrap_or_else(|| "hidden".into()));
    out
}

fn check(name: &str, bytes: &[u8]) {
    let actual = render(bytes);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(format!("{name}.txt"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected =
        std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("missing {}: run with UPDATE_GOLDEN=1 to create it", path.display()));
    if expected != actual {
        panic!("golden mismatch for {name}\n--- expected ({})\n{expected}\n--- actual\n{actual}", path.display());
    }
}

macro_rules! golden {
    ($($name:ident: $bytes:expr;)*) => {
        $( #[test] fn $name() { check(stringify!($name), $bytes); } )*
    };
}

golden! {
    // Plain text, CRLF, and the cursor ending after it.
    plain_text: b"hello\r\nworld";
    // CUP/CUU/CUD/CUF/CUB: draw a box corner by corner.
    cursor_movement: b"\x1b[2;3H+\x1b[2;10H+\x1b[5;3H+\x1b[5;10H+\x1b[3;3H|\x1b[B\x1b[D|\x1b[3;10H|\x1b[4;10H|\x1b[2;4H\x1b[6C\x1b[1D-";
    // DECSTBM: lines 3–5 scroll, the rest stays put.
    scroll_region: b"top\r\n2\r\nA\r\nB\r\nC\r\nbottom\x1b[3;5r\x1b[5;1H\nD\nE\x1b[r";
    // ICH, DCH, IL, DL.
    insert_delete: b"abcdef\x1b[1;3H\x1b[2@XY\x1b[2;1Hline2\x1b[3;1Hline3\x1b[2;1H\x1b[L\x1b[4;1H\x1b[M\x1b[1;1H\x1b[1P";
    // Autowrap on, then DECAWM off (the last column is overwritten).
    autowrap: b"0123456789abcdefghijklmnOVERFLOW\r\n\x1b[?7lABCDEFGHIJKLMNOPQRSTUVWXYZ!\x1b[?7h";
    // Default tab stops, then a custom stop (HTS) after clearing all (TBC 3).
    tab_stops: b"a\tb\tc\r\n\x1b[3g\x1b[5G\x1bH\r\tX";
    // DECALN fills the screen with E.
    decaln: b"\x1b#8";
    // DECOM: CUP is relative to the scrolling region.
    origin_mode: b"\x1b[3;6r\x1b[?6h\x1b[1;1Hregion-top\x1b[?6l\x1b[r\x1b[1;1Hscreen-top";
    // ED and EL variants.
    erase: b"xxxxxxxxxx\r\nyyyyyyyyyy\r\nzzzzzzzzzz\x1b[2;5H\x1b[K\x1b[1;5H\x1b[1K\x1b[3;3H\x1b[2K\x1b[3;1Hend";
    // The alternate screen leaves the main screen untouched.
    alt_screen: b"main screen\x1b[?1049h\x1b[2J\x1b[Halt content\x1b[?1049l";
    // Wide CJK, emoji and a combining mark.
    wide_chars: "中文 ok\r\n🦑 kraken\r\ne\u{301} accent".as_bytes();
    // 16-colour, 256-colour, truecolor, bold, italic, inverse, dim.
    attributes: b"\x1b[31mred\x1b[0m \x1b[1;38;5;45mbold256\x1b[0m \x1b[3;38;2;255;128;0mital\x1b[0m\r\n\x1b[7minv\x1b[0m \x1b[2mdim\x1b[0m \x1b[44mbg\x1b[0m";
    // Every underline style, SGR 58 colours, strikethrough.
    underlines: b"\x1b[4ma\x1b[0m \x1b[4:2mb\x1b[0m \x1b[4:3mc\x1b[0m \x1b[4:4md\x1b[0m \x1b[4:5me\x1b[0m \x1b[4:3;58;2;255;0;0mf\x1b[0m \x1b[9mg\x1b[0m";
    // DECSCUSR: beam and underline cursors; DECTCEM hides it.
    cursor_beam: b"beam\x1b[6 q";
    cursor_underline: b"under\x1b[4 q";
    cursor_hidden: b"hidden\x1b[?25l";
}
