//! Keyboard and mouse → bytes for the shell (xterm conventions).

use alacritty_terminal::term::TermMode;
use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// xterm modifier parameter: 1 + shift(1) + alt(2) + ctrl(4).
fn mod_param(mods: ModifiersState, meta: bool) -> u8 {
    1 + mods.shift_key() as u8 + (meta as u8) * 2 + mods.control_key() as u8 * 4
}

/// Encode a key press. `meta` is true when Option should act as Meta for this press.
/// Returns `None` for keys that produce nothing (bare modifiers, Cmd shortcuts).
pub fn encode_key(ev: &KeyEvent, mods: ModifiersState, mode: TermMode, meta: bool) -> Option<Vec<u8>> {
    if ev.state != ElementState::Pressed || mods.super_key() {
        return None;
    }
    let app_cursor = mode.contains(TermMode::APP_CURSOR);
    let ctrl = mods.control_key();
    let shift = mods.shift_key();
    let m = mod_param(mods, meta);
    let esc = |s: &str| Some(format!("\x1b{s}").into_bytes());

    // Cursor-style keys: CSI <x> or SS3 <x> without modifiers, CSI 1;<m><x> with.
    let cursor_key = |c: char| {
        if m > 1 {
            esc(&format!("[1;{m}{c}"))
        } else if app_cursor {
            esc(&format!("O{c}"))
        } else {
            esc(&format!("[{c}"))
        }
    };
    // Tilde keys: CSI <n>~ or CSI <n>;<m>~.
    let tilde = |n: u8| if m > 1 { esc(&format!("[{n};{m}~")) } else { esc(&format!("[{n}~")) };
    let f_ss3 = |c: char| if m > 1 { esc(&format!("[1;{m}{c}")) } else { esc(&format!("O{c}")) };

    if let Key::Named(named) = &ev.logical_key {
        return match named {
            NamedKey::Enter if shift || meta => esc("\r"), // newline in coding agents
            NamedKey::Enter => Some(b"\r".to_vec()),
            NamedKey::Tab if shift => esc("[Z"),
            NamedKey::Tab => Some(b"\t".to_vec()),
            NamedKey::Backspace if ctrl => Some(vec![0x08]),
            NamedKey::Backspace if meta => esc("\x7f"), // delete word
            NamedKey::Backspace => Some(vec![0x7f]),
            NamedKey::Escape => Some(vec![0x1b]),
            NamedKey::Space if ctrl => Some(vec![0]),
            NamedKey::Space if meta => esc(" "),
            NamedKey::Space => Some(b" ".to_vec()),
            // macOS natural editing: Option+←/→ jumps words.
            NamedKey::ArrowLeft if meta && !ctrl && !shift => esc("b"),
            NamedKey::ArrowRight if meta && !ctrl && !shift => esc("f"),
            NamedKey::ArrowUp => cursor_key('A'),
            NamedKey::ArrowDown => cursor_key('B'),
            NamedKey::ArrowRight => cursor_key('C'),
            NamedKey::ArrowLeft => cursor_key('D'),
            NamedKey::Home => cursor_key('H'),
            NamedKey::End => cursor_key('F'),
            NamedKey::Insert => tilde(2),
            NamedKey::Delete => tilde(3),
            NamedKey::PageUp => tilde(5),
            NamedKey::PageDown => tilde(6),
            NamedKey::F1 => f_ss3('P'),
            NamedKey::F2 => f_ss3('Q'),
            NamedKey::F3 => f_ss3('R'),
            NamedKey::F4 => f_ss3('S'),
            NamedKey::F5 => tilde(15),
            NamedKey::F6 => tilde(17),
            NamedKey::F7 => tilde(18),
            NamedKey::F8 => tilde(19),
            NamedKey::F9 => tilde(20),
            NamedKey::F10 => tilde(21),
            NamedKey::F11 => tilde(23),
            NamedKey::F12 => tilde(24),
            _ => None,
        };
    }

    let Key::Character(chars) = &ev.logical_key else { return None };

    if ctrl {
        let c = chars.chars().next()?.to_ascii_lowercase();
        let code = match c {
            'a'..='z' => c as u8 - b'a' + 1,
            '@' | '2' | ' ' => 0,
            '[' | '3' => 0x1b,
            '\\' | '4' => 0x1c,
            ']' | '5' => 0x1d,
            '^' | '6' => 0x1e,
            '_' | '-' | '7' | '/' => 0x1f,
            '8' | '?' => 0x7f,
            _ => return ev.text.as_ref().map(|t| t.as_bytes().to_vec()),
        };
        return Some(if meta { vec![0x1b, code] } else { vec![code] });
    }

    if meta {
        // With Option-as-Alt, logical_key is the un-optioned character.
        let mut out = vec![0x1b];
        out.extend_from_slice(chars.as_bytes());
        return Some(out);
    }

    ev.text.as_ref().map(|t| t.as_bytes().to_vec())
}

/// Encode a mouse report. `button` is 0/1/2 for left/middle/right, 3 for
/// release in legacy mode, 64/65 for wheel up/down. `motion` adds 32.
pub fn encode_mouse(
    button: u8,
    pressed: bool,
    motion: bool,
    col: usize,
    row: usize,
    mods: ModifiersState,
    mode: TermMode,
) -> Option<Vec<u8>> {
    let mut b = button as u32;
    if motion {
        b += 32;
    }
    if mods.shift_key() {
        b += 4;
    }
    if mods.alt_key() {
        b += 8;
    }
    if mods.control_key() {
        b += 16;
    }
    if mode.contains(TermMode::SGR_MOUSE) {
        let c = if pressed { 'M' } else { 'm' };
        return Some(format!("\x1b[<{b};{};{}{c}", col + 1, row + 1).into_bytes());
    }
    // Legacy X10/normal encoding: releases are reported as button 3.
    if !pressed {
        b = (b & !3) | 3;
    }
    let (x, y) = (col + 1 + 32, row + 1 + 32);
    if mode.contains(TermMode::UTF8_MOUSE) {
        let mut s = String::from("\x1b[M");
        for v in [b + 32, x as u32, y as u32] {
            s.push(char::from_u32(v)?);
        }
        return Some(s.into_bytes());
    }
    if x > 255 || y > 255 {
        return None;
    }
    Some(vec![0x1b, b'[', b'M', (b + 32) as u8, x as u8, y as u8])
}

pub fn mouse_button_code(b: MouseButton) -> Option<u8> {
    match b {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        _ => None,
    }
}

/// Prepare clipboard text for pasting into the shell.
pub fn paste_bytes(text: &str, mode: TermMode) -> Vec<u8> {
    if mode.contains(TermMode::BRACKETED_PASTE) {
        // Strip any embedded end marker so pasted text can't escape the bracket.
        let clean = text.replace("\x1b[201~", "");
        format!("\x1b[200~{clean}\x1b[201~").into_bytes()
    } else {
        text.replace("\r\n", "\r").replace('\n', "\r").into_bytes()
    }
}

// ── Kitty keyboard protocol ──────────────────────────────────────────────
// https://sw.kovidgoyal.net/kitty/keyboard-protocol/
// Apps (Claude Code, neovim, helix…) turn this on to tell apart keys that
// legacy terminals send identically: Shift+Enter vs Enter, Ctrl+I vs Tab,
// Esc vs Alt+…, and to receive key releases.

/// Modifier parameter: 1 + shift(1) + alt(2) + ctrl(4) + super(8).
fn kitty_mods(mods: ModifiersState, meta: bool) -> u32 {
    1 + mods.shift_key() as u32 + (meta as u32) * 2 + mods.control_key() as u32 * 4 + mods.super_key() as u32 * 8
}

/// Key number and the final character for keys with a fixed CSI form.
fn kitty_functional(named: &NamedKey) -> Option<(u32, char)> {
    Some(match named {
        NamedKey::Escape => (27, 'u'),
        NamedKey::Enter => (13, 'u'),
        NamedKey::Tab => (9, 'u'),
        NamedKey::Backspace => (127, 'u'),
        NamedKey::Space => (32, 'u'),
        NamedKey::ArrowUp => (1, 'A'),
        NamedKey::ArrowDown => (1, 'B'),
        NamedKey::ArrowRight => (1, 'C'),
        NamedKey::ArrowLeft => (1, 'D'),
        NamedKey::Home => (1, 'H'),
        NamedKey::End => (1, 'F'),
        NamedKey::F1 => (1, 'P'),
        NamedKey::F2 => (1, 'Q'),
        NamedKey::F4 => (1, 'S'),
        NamedKey::Insert => (2, '~'),
        NamedKey::Delete => (3, '~'),
        NamedKey::PageUp => (5, '~'),
        NamedKey::PageDown => (6, '~'),
        NamedKey::F3 => (13, '~'),
        NamedKey::F5 => (15, '~'),
        NamedKey::F6 => (17, '~'),
        NamedKey::F7 => (18, '~'),
        NamedKey::F8 => (19, '~'),
        NamedKey::F9 => (20, '~'),
        NamedKey::F10 => (21, '~'),
        NamedKey::F11 => (23, '~'),
        NamedKey::F12 => (24, '~'),
        NamedKey::Shift => (57441, 'u'),
        NamedKey::Control => (57442, 'u'),
        NamedKey::Alt => (57443, 'u'),
        NamedKey::Super => (57444, 'u'),
        NamedKey::CapsLock => (57358, 'u'),
        _ => return None,
    })
}

/// A key event, reduced to what the encoder needs (so it can be unit-tested).
pub struct KittyKey<'a> {
    pub named: Option<NamedKey>,
    /// The key's character without modifiers (lowercase), for text keys.
    pub base: Option<char>,
    /// Text the key would type.
    pub text: Option<&'a str>,
    /// 1 press, 2 repeat, 3 release.
    pub event: u8,
}

/// Encode with the Kitty protocol. `None` means "send nothing".
pub fn encode_kitty(k: &KittyKey, mods: ModifiersState, mode: TermMode, meta: bool) -> Option<Vec<u8>> {
    let all_as_esc = mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC);
    let events = mode.contains(TermMode::REPORT_EVENT_TYPES);
    let assoc = mode.contains(TermMode::REPORT_ASSOCIATED_TEXT);
    if k.event == 3 && !events {
        return None;
    }
    let m = kitty_mods(mods, meta);
    let modified = mods.control_key() || meta || mods.super_key();
    let is_modifier_key =
        matches!(k.named, Some(NamedKey::Shift | NamedKey::Control | NamedKey::Alt | NamedKey::Super | NamedKey::CapsLock));
    if is_modifier_key && !all_as_esc {
        return None;
    }

    // Plain typing stays plain text, unless the app wants every key as an escape.
    if !all_as_esc && k.event != 3 {
        if let Some(text) = k.text.filter(|t| !t.is_empty() && !t.chars().any(char::is_control))
            && !modified
            && k.named.is_none_or(|n| n == NamedKey::Space)
        {
            return Some(text.as_bytes().to_vec());
        }
        // Enter / Tab / Backspace without modifiers keep their legacy bytes, so a
        // shell still works if a program crashes with the mode left on.
        if m == 1 {
            match k.named {
                Some(NamedKey::Enter) => return Some(b"\r".to_vec()),
                Some(NamedKey::Tab) => return Some(b"\t".to_vec()),
                Some(NamedKey::Backspace) => return Some(vec![0x7f]),
                _ => {}
            }
        }
    }

    let (code, end) = match (&k.named, k.base) {
        (Some(n), _) => kitty_functional(n)?,
        (None, Some(c)) => (c.to_lowercase().next().unwrap_or(c) as u32, 'u'),
        _ => return None,
    };
    let mut params = String::new();
    let event_suffix = if events && k.event != 1 { format!(":{}", k.event) } else { String::new() };
    let text_param = if assoc && all_as_esc && k.event != 3 {
        k.text.filter(|t| !t.chars().any(char::is_control)).map(|t| t.chars().map(|c| (c as u32).to_string()).collect::<Vec<_>>().join(":"))
    } else {
        None
    };
    if m != 1 || !event_suffix.is_empty() || text_param.is_some() {
        params.push_str(&format!(";{m}{event_suffix}"));
    }
    if let Some(t) = text_param {
        params.push_str(&format!(";{t}"));
    }
    // Arrows etc. without modifiers keep the short form: CSI A, not CSI 1A.
    let key = if end != 'u' && code == 1 && params.is_empty() { String::new() } else { code.to_string() };
    Some(format!("\x1b[{key}{params}{end}").into_bytes())
}

#[cfg(test)]
mod kitty_tests {
    use super::*;

    const DISAMBIGUATE: TermMode = TermMode::DISAMBIGUATE_ESC_CODES;

    fn key(named: Option<NamedKey>, base: Option<char>, text: Option<&str>) -> KittyKey<'_> {
        KittyKey { named, base, text, event: 1 }
    }
    fn enc(k: &KittyKey, mods: ModifiersState, mode: TermMode) -> String {
        let meta = mods.alt_key();
        String::from_utf8(encode_kitty(k, mods, mode, meta).unwrap_or_default()).unwrap()
    }

    #[test]
    fn disambiguate() {
        let none = ModifiersState::empty();
        assert_eq!(enc(&key(None, Some('a'), Some("a")), none, DISAMBIGUATE), "a");
        assert_eq!(enc(&key(None, Some('a'), Some("A")), ModifiersState::SHIFT, DISAMBIGUATE), "A");
        assert_eq!(enc(&key(Some(NamedKey::Escape), None, None), none, DISAMBIGUATE), "\x1b[27u");
        assert_eq!(enc(&key(Some(NamedKey::Enter), None, None), none, DISAMBIGUATE), "\r");
        // The reason agents want this: Shift+Enter is distinguishable.
        assert_eq!(enc(&key(Some(NamedKey::Enter), None, None), ModifiersState::SHIFT, DISAMBIGUATE), "\x1b[13;2u");
        assert_eq!(enc(&key(None, Some('c'), None), ModifiersState::CONTROL, DISAMBIGUATE), "\x1b[99;5u");
        assert_eq!(enc(&key(None, Some('i'), None), ModifiersState::CONTROL, DISAMBIGUATE), "\x1b[105;5u");
        assert_eq!(enc(&key(Some(NamedKey::ArrowUp), None, None), none, DISAMBIGUATE), "\x1b[A");
        assert_eq!(enc(&key(Some(NamedKey::ArrowUp), None, None), ModifiersState::CONTROL, DISAMBIGUATE), "\x1b[1;5A");
        assert_eq!(enc(&key(Some(NamedKey::Delete), None, None), none, DISAMBIGUATE), "\x1b[3~");
        assert_eq!(enc(&key(Some(NamedKey::F3), None, None), none, DISAMBIGUATE), "\x1b[13~");
    }

    #[test]
    fn all_keys_events_and_text() {
        let mode = TermMode::DISAMBIGUATE_ESC_CODES
            | TermMode::REPORT_ALL_KEYS_AS_ESC
            | TermMode::REPORT_EVENT_TYPES
            | TermMode::REPORT_ASSOCIATED_TEXT;
        let none = ModifiersState::empty();
        assert_eq!(enc(&key(None, Some('a'), Some("a")), none, mode), "\x1b[97;1;97u");
        assert_eq!(enc(&key(Some(NamedKey::Enter), None, None), none, mode), "\x1b[13u");
        let release = KittyKey { named: None, base: Some('a'), text: None, event: 3 };
        assert_eq!(enc(&release, none, mode), "\x1b[97;1:3u");
        let repeat = KittyKey { named: Some(NamedKey::ArrowLeft), base: None, text: None, event: 2 };
        assert_eq!(enc(&repeat, ModifiersState::SHIFT, mode), "\x1b[1;2:2D");
        assert_eq!(enc(&key(Some(NamedKey::Shift), None, None), ModifiersState::SHIFT, mode), "\x1b[57441;2u");
        // Releases are dropped unless asked for.
        assert_eq!(enc(&release, none, DISAMBIGUATE), "");
    }
}
