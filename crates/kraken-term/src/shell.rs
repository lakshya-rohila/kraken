//! The PTY output stream, between the shell and the terminal emulator.
//!
//! `alacritty_terminal` ignores some sequences we care about, so we wrap its
//! PTY with a reader that rewrites the stream on the way through:
//! - OSC 133 (prompt/command marks), OSC 7 (cwd), OSC 9/777 (notifications)
//!   are reported as [`ShellEvent`]s and still passed on;
//! - images (Kitty graphics APC, iTerm2 OSC 1337, sixel DCS) are decoded and
//!   replaced with placeholder characters (see [`crate::images`]).
//!
//! Everything else passes through byte for byte.

use alacritty_terminal::event::{OnResize, WindowSize};
use alacritty_terminal::tty::{ChildEvent, EventedPty, EventedReadWrite, Pty};
use polling::{Event, PollMode, Poller};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::images::{self, KittyState, SharedImages};

#[derive(Debug, Clone, PartialEq)]
pub enum ShellEvent {
    /// The shell is about to draw a prompt.
    Prompt,
    /// The user ran a command.
    CommandStart,
    /// A command finished.
    CommandEnd {
        exit: Option<i32>,
    },
    Cwd(PathBuf),
    /// A program asked for a desktop notification (OSC 9 / OSC 777).
    Notify {
        title: Option<String>,
        body: String,
    },
    /// Bytes to send back to the program (e.g. image protocol replies).
    Reply(Vec<u8>),
}

pub type ShellListener = Arc<dyn Fn(ShellEvent, Instant) + Send + Sync>;

#[derive(Default, Clone, Copy, PartialEq)]
enum State {
    #[default]
    Ground,
    Esc,
    /// Inside OSC (`ESC ]`), APC (`ESC _`) or DCS (`ESC P`).
    Seq(Kind),
    /// Saw ESC inside a sequence: `\` ends it, anything else aborts it.
    SeqEsc(Kind),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Osc,
    Apc,
    Dcs,
}

impl Kind {
    fn intro(self) -> u8 {
        match self {
            Kind::Osc => b']',
            Kind::Apc => b'_',
            Kind::Dcs => b'P',
        }
    }
}

/// Images can be large; anything bigger than this is dropped.
const MAX_SEQ: usize = 64 * 1024 * 1024;

/// Incremental stream rewriter; survives sequences split across reads.
pub struct Rewriter {
    state: State,
    buf: Vec<u8>,
    overflow: bool,
    images: Option<SharedImages>,
    kitty: KittyState,
    /// Between "prompt starts" and "command starts" we tag cells with a
    /// private hyperlink, so the prompt's exact lines are known later.
    prompt_tag_open: bool,
}

/// URI of the invisible tag on prompt cells (never shown as a link).
pub const PROMPT_TAG: &str = "kraken:prompt";

impl Default for Rewriter {
    fn default() -> Self {
        Rewriter::new(None)
    }
}

impl Rewriter {
    pub fn new(images: Option<SharedImages>) -> Self {
        Rewriter { state: State::Ground, buf: Vec::new(), overflow: false, images, kitty: KittyState::default(), prompt_tag_open: false }
    }

    /// Process `input`, appending what the emulator should see to `out`.
    pub fn feed(&mut self, input: &[u8], out: &mut Vec<u8>, mut emit: impl FnMut(ShellEvent)) {
        let mut i = 0;
        while i < input.len() {
            match self.state {
                State::Ground => {
                    // Fast path: copy everything up to the next ESC.
                    match input[i..].iter().position(|&c| c == 0x1b) {
                        Some(p) => {
                            out.extend_from_slice(&input[i..i + p]);
                            i += p + 1;
                            self.state = State::Esc;
                        }
                        None => {
                            out.extend_from_slice(&input[i..]);
                            return;
                        }
                    }
                }
                State::Esc => {
                    let b = input[i];
                    i += 1;
                    let kind = match b {
                        b']' => Some(Kind::Osc),
                        b'_' => Some(Kind::Apc),
                        b'P' => Some(Kind::Dcs),
                        _ => None,
                    };
                    match kind {
                        Some(k) => {
                            self.buf.clear();
                            self.overflow = false;
                            self.state = State::Seq(k);
                        }
                        None if b == 0x1b => out.push(0x1b), // ESC ESC: first one is literal
                        None => {
                            out.extend_from_slice(&[0x1b, b]);
                            self.state = State::Ground;
                        }
                    }
                }
                State::Seq(k) => {
                    // Copy the body in bulk up to the next ESC (or BEL for OSC).
                    let end = input[i..].iter().position(|&c| c == 0x1b || (k == Kind::Osc && c == 0x07));
                    let take = end.unwrap_or(input.len() - i);
                    if self.buf.len() + take > MAX_SEQ {
                        self.overflow = true;
                        self.buf.clear();
                    } else if !self.overflow {
                        self.buf.extend_from_slice(&input[i..i + take]);
                    }
                    i += take;
                    if i < input.len() {
                        let c = input[i];
                        i += 1;
                        if c == 0x07 {
                            self.finish(k, b"\x07", out, &mut emit);
                            self.state = State::Ground;
                        } else {
                            self.state = State::SeqEsc(k);
                        }
                    }
                }
                State::SeqEsc(k) => {
                    if input[i] == b'\\' {
                        i += 1;
                        self.finish(k, b"\x1b\\", out, &mut emit);
                        self.state = State::Ground;
                    } else {
                        // Not a terminator: the sequence was abandoned. Pass on
                        // what we held back, then treat this ESC afresh.
                        if !self.overflow {
                            out.extend_from_slice(&[0x1b, k.intro()]);
                            out.extend_from_slice(&self.buf);
                        }
                        self.state = State::Esc;
                    }
                }
            }
        }
    }

    fn finish(&mut self, kind: Kind, terminator: &[u8], out: &mut Vec<u8>, emit: &mut impl FnMut(ShellEvent)) {
        if self.overflow {
            return;
        }
        let body = std::mem::take(&mut self.buf);
        let passthrough = |out: &mut Vec<u8>, body: &[u8]| {
            out.extend_from_slice(&[0x1b, kind.intro()]);
            out.extend_from_slice(body);
            out.extend_from_slice(terminator);
        };
        match kind {
            Kind::Osc => {
                if body.starts_with(b"1337;File=") {
                    if let Some(images) = &self.images
                        && let Some(bytes) = images::iterm_image(images, &body)
                    {
                        out.extend_from_slice(&bytes);
                    }
                    return;
                }
                let event = parse_osc(&body);
                let close = |out: &mut Vec<u8>, open: &mut bool| {
                    if *open {
                        out.extend_from_slice(b"\x1b]8;;\x1b\\");
                        *open = false;
                    }
                };
                match &event {
                    Some(ShellEvent::Prompt) => {
                        close(out, &mut self.prompt_tag_open);
                        passthrough(out, &body);
                        out.extend_from_slice(format!("\x1b]8;;{PROMPT_TAG}\x1b\\").as_bytes());
                        self.prompt_tag_open = true;
                    }
                    Some(ShellEvent::CommandStart) | Some(ShellEvent::CommandEnd { .. }) => {
                        close(out, &mut self.prompt_tag_open);
                        passthrough(out, &body);
                    }
                    _ => passthrough(out, &body),
                }
                if let Some(e) = event {
                    emit(e);
                }
            }
            Kind::Apc => {
                // Kitty graphics. Other APC strings are ignored by the emulator anyway.
                if let (Some(rest), Some(images)) = (body.strip_prefix(b"G"), &self.images) {
                    let r = self.kitty.handle(images, rest);
                    out.extend_from_slice(&r.output);
                    if let Some(reply) = r.reply {
                        emit(ShellEvent::Reply(reply));
                    }
                }
            }
            Kind::Dcs => {
                if let Some(images) = &self.images
                    && let Some(img) = images::sixel_image(&body)
                {
                    if let Ok(mut s) = images.lock() {
                        out.extend_from_slice(&s.place(img, None, None));
                    }
                    return;
                }
                passthrough(out, &body);
            }
        }
    }
}

fn parse_osc(body: &[u8]) -> Option<ShellEvent> {
    let text = String::from_utf8_lossy(body);
    let (code, rest) = text.split_once(';').unwrap_or((&text, ""));
    match code {
        "133" => {
            let mut parts = rest.split(';');
            match parts.next() {
                Some("A") => Some(ShellEvent::Prompt),
                Some("C") => Some(ShellEvent::CommandStart),
                Some("D") => Some(ShellEvent::CommandEnd { exit: parts.next().and_then(|s| s.parse().ok()) }),
                _ => None,
            }
        }
        "7" => parse_file_url(rest).map(ShellEvent::Cwd),
        // OSC 9 is also used by ConEmu for progress ("9;4;…"); skip those.
        "9" if !rest.starts_with("4;") && !rest.is_empty() => Some(ShellEvent::Notify { title: None, body: rest.to_string() }),
        "777" => {
            let mut parts = rest.splitn(3, ';');
            match (parts.next(), parts.next(), parts.next()) {
                (Some("notify"), Some(title), body) => {
                    Some(ShellEvent::Notify { title: Some(title.to_string()), body: body.unwrap_or("").to_string() })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// `file://host/path%20with%20spaces` → `/path with spaces`.
fn parse_file_url(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let path = &rest[rest.find('/')?..];
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) = u8::from_str_radix(&path[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(PathBuf::from(String::from_utf8_lossy(&out).into_owned()))
}

/// Reader that runs PTY output through the [`Rewriter`].
pub struct ScanReader {
    file: File,
    /// Same PTY, for answering protocol queries in stream order.
    reply_to: File,
    rewriter: Rewriter,
    listener: ShellListener,
    raw: Vec<u8>,
    /// Rewritten bytes not yet handed to the emulator.
    pending: Vec<u8>,
    pos: usize,
}

impl Read for ScanReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Never return 0 while the shell is alive (0 means end of file): if a
        // read only fed the middle of an image, read again.
        while self.pos >= self.pending.len() {
            self.pending.clear();
            self.pos = 0;
            let n = self.file.read(&mut self.raw)?;
            if n == 0 {
                return Ok(0);
            }
            let now = Instant::now();
            let raw = &self.raw[..n];
            let mut events = Vec::new();
            self.rewriter.feed(raw, &mut self.pending, |e| events.push(e));
            for e in events {
                match e {
                    // Answer right away: programs often send a query followed by
                    // a "device attributes" request and stop listening when that
                    // answer arrives, so ours must come first.
                    ShellEvent::Reply(bytes) => {
                        use std::io::Write;
                        let _ = self.reply_to.write_all(&bytes);
                    }
                    e => (self.listener)(e, now),
                }
            }
        }
        let n = buf.len().min(self.pending.len() - self.pos);
        buf[..n].copy_from_slice(&self.pending[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// A [`Pty`] whose output passes through the [`Rewriter`].
pub struct ScanPty {
    pty: Pty,
    reader: ScanReader,
}

impl ScanPty {
    pub fn new(mut pty: Pty, listener: ShellListener, images: Option<SharedImages>) -> io::Result<Self> {
        // A second handle to the same PTY: reads through it see the same stream.
        let file = pty.reader().try_clone()?;
        let reply_to = pty.reader().try_clone()?;
        let reader =
            ScanReader { file, reply_to, rewriter: Rewriter::new(images), listener, raw: vec![0; 64 * 1024], pending: Vec::new(), pos: 0 };
        Ok(ScanPty { pty, reader })
    }
}

impl EventedReadWrite for ScanPty {
    type Reader = ScanReader;
    type Writer = File;

    unsafe fn register(&mut self, poll: &Arc<Poller>, interest: Event, mode: PollMode) -> io::Result<()> {
        // SAFETY: forwards the caller's contract (the fd stays registered only
        // while the pty lives) to the wrapped pty unchanged.
        unsafe { self.pty.register(poll, interest, mode) }
    }
    fn reregister(&mut self, poll: &Arc<Poller>, interest: Event, mode: PollMode) -> io::Result<()> {
        self.pty.reregister(poll, interest, mode)
    }
    fn deregister(&mut self, poll: &Arc<Poller>) -> io::Result<()> {
        self.pty.deregister(poll)
    }
    fn reader(&mut self) -> &mut ScanReader {
        &mut self.reader
    }
    fn writer(&mut self) -> &mut File {
        self.pty.writer()
    }
}

impl EventedPty for ScanPty {
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.pty.next_child_event()
    }
}

impl OnResize for ScanPty {
    fn on_resize(&mut self, size: WindowSize) {
        self.pty.on_resize(size)
    }
}

/// Shells Kraken ships integration scripts for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    Zsh,
    Bash,
    Fish,
    Nu,
    Pwsh,
}

impl ShellKind {
    /// Recognise a shell by its executable's file name (`/opt/homebrew/bin/fish`,
    /// `-zsh`, `pwsh.exe`, …).
    pub fn detect(shell: &str) -> Option<ShellKind> {
        let name = shell.trim().rsplit('/').next()?.trim_start_matches('-');
        let name = name.strip_suffix(".exe").unwrap_or(name);
        Some(match name {
            "zsh" => ShellKind::Zsh,
            "bash" => ShellKind::Bash,
            "fish" => ShellKind::Fish,
            "nu" | "nushell" => ShellKind::Nu,
            "pwsh" | "pwsh-preview" | "powershell" => ShellKind::Pwsh,
            _ => return None,
        })
    }

    /// The scripts to install, as (path under the integration dir, contents).
    fn files(self) -> &'static [(&'static str, &'static str)] {
        match self {
            ShellKind::Zsh => &[("zsh/.zshenv", include_str!("../shell/zshenv")), ("zsh/kraken.zsh", include_str!("../shell/kraken.zsh"))],
            ShellKind::Bash => &[("bash/kraken.bash", include_str!("../shell/kraken.bash"))],
            ShellKind::Fish => &[(FISH_CONF, include_str!("../shell/kraken.fish"))],
            ShellKind::Nu => &[(NU_AUTOLOAD, include_str!("../shell/kraken.nu"))],
            ShellKind::Pwsh => &[("pwsh/kraken.ps1", include_str!("../shell/kraken.ps1"))],
        }
    }
}

/// fish and nu find these through `XDG_DATA_DIRS` (see [`plan`]).
const DATA_DIR: &str = "data";
const FISH_CONF: &str = "data/fish/vendor_conf.d/kraken.fish";
const NU_AUTOLOAD: &str = "data/nushell/vendor/autoload/kraken.nu";

/// How to start a shell so it loads Kraken's integration.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Integration {
    /// Extra environment variables for the shell.
    pub env: Vec<(String, String)>,
    /// Arguments to start the shell with *instead of* the usual `-l`, or
    /// `None` to start it as usual.
    pub args: Option<Vec<String>>,
}

/// Install the integration scripts for `shell` and return how to load them.
/// Returns `None` for shells we don't know or if the files can't be written.
pub fn integration(shell: &str) -> Option<Integration> {
    let kind = ShellKind::detect(shell)?;
    let dir = dirs_support()?.join("shell-integration");
    install(kind, &dir).ok()?;
    Some(plan(kind, &dir, |k| std::env::var(k).ok()))
}

/// Write `kind`'s scripts under `dir`, skipping files that are already current.
fn install(kind: ShellKind, dir: &Path) -> io::Result<()> {
    for (name, body) in kind.files() {
        let p = dir.join(name);
        if std::fs::read_to_string(&p).ok().as_deref() != Some(*body) {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&p, body)?;
        }
    }
    Ok(())
}

/// The env and argv that load the scripts installed under `dir`. `var` reads
/// Kraken's own environment (what the shell would otherwise inherit).
///
/// - zsh: `ZDOTDIR` points at our `.zshenv`, which restores the user's
///   `ZDOTDIR` and sources their files, then ours.
/// - bash: `bash --rcfile kraken.bash`; the script sources the login files
///   (`/etc/profile`, `~/.bash_profile`…) itself, since login shells ignore
///   `--rcfile`. Works with macOS's bash 3.2, unlike the `--posix`/`ENV` trick.
/// - fish, nu: our data dir goes first in `XDG_DATA_DIRS`, where fish looks
///   for `vendor_conf.d` and nu for `vendor/autoload`; the script puts the
///   variable back. The user's config loads untouched.
/// - pwsh: `-NoExit -Command ". 'kraken.ps1'"` runs after `$PROFILE`.
fn plan(kind: ShellKind, dir: &Path, var: impl Fn(&str) -> Option<String>) -> Integration {
    let path = |rel: &str| dir.join(rel).to_string_lossy().into_owned();
    let mut env = Vec::new();
    let mut set = |k: &str, v: String| env.push((k.to_string(), v));
    let mut args = None;
    match kind {
        ShellKind::Zsh => {
            set("ZDOTDIR", path("zsh"));
            set("KRAKEN_SHELL_INTEGRATION", path("zsh/kraken.zsh"));
            if let Some(orig) = var("ZDOTDIR") {
                set("KRAKEN_ORIG_ZDOTDIR", orig);
            }
        }
        ShellKind::Bash => {
            // Kraken always starts login shells; the script does what `-l` would.
            set("KRAKEN_BASH_INJECT", "login".into());
            args = Some(vec!["--rcfile".into(), path("bash/kraken.bash")]);
        }
        ShellKind::Fish | ShellKind::Nu => {
            let data = path(DATA_DIR);
            let orig = var("XDG_DATA_DIRS").filter(|v| !v.is_empty());
            // Unset means this default (XDG spec); keep it so nothing else is lost.
            let rest = orig.clone().unwrap_or_else(|| "/usr/local/share:/usr/share".into());
            set("XDG_DATA_DIRS", format!("{data}:{rest}"));
            set("KRAKEN_SHELL_DATA_DIR", data);
            if let Some(orig) = orig {
                set("KRAKEN_ORIG_XDG_DATA_DIRS", orig);
            }
        }
        ShellKind::Pwsh => {
            let script = path("pwsh/kraken.ps1").replace('\'', "''");
            args = Some(vec!["-Login".into(), "-NoExit".into(), "-Command".into(), format!(". '{script}'")]);
        }
    }
    Integration { env, args }
}

fn dirs_support() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/Kraken"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(chunks: &[&[u8]]) -> Vec<ShellEvent> {
        rewrite(chunks).1
    }

    fn rewrite(chunks: &[&[u8]]) -> (Vec<u8>, Vec<ShellEvent>) {
        let mut r = Rewriter::new(Some(images::ImageStore::new(images::Metrics { cell_w: 10.0, cell_h: 20.0, cols: 80 })));
        let (mut out, mut ev) = (vec![], vec![]);
        for c in chunks {
            r.feed(c, &mut out, |e| ev.push(e));
        }
        (out, ev)
    }

    #[test]
    fn ordinary_output_passes_through_unchanged() {
        let input: &[&[u8]] =
            &[b"hi \x1b[31mred\x1b[0m \x1b]0;title\x07 \x1b]8;;http://x\x1b\\link\x1b]8;;\x1b\\ \x1bP+q544e\x1b\\ \x1b\x1b[A"];
        let (out, _) = rewrite(input);
        assert_eq!(out, input[0]);
        // Split anywhere: same result.
        let whole = input[0];
        for cut in 1..whole.len() {
            let (out, _) = rewrite(&[&whole[..cut], &whole[cut..]]);
            assert_eq!(out, whole, "cut at {cut}");
        }
    }

    #[test]
    fn prompts_are_tagged() {
        let (out, ev) = rewrite(&[b"\x1b]133;A\x07$ ls\x1b]133;C\x07out"]);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text, "\x1b]133;A\x07\x1b]8;;kraken:prompt\x1b\\$ ls\x1b]8;;\x1b\\\x1b]133;C\x07out");
        assert_eq!(ev, vec![ShellEvent::Prompt, ShellEvent::CommandStart]);
    }

    #[test]
    fn images_become_placeholders() {
        let (out, ev) = rewrite(&[b"A\x1bPq\"1;1;10;6#1;2;100;0;0#1!10~\x1b\\B"]);
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with('A') && text.ends_with('B'));
        assert_eq!(text.chars().filter_map(images::decode_placeholder).count(), 1);
        assert!(ev.is_empty());
        // Kitty query gets a reply and leaves no trace on screen.
        let (out, ev) = rewrite(&[b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\"]);
        assert!(out.is_empty());
        assert_eq!(ev, vec![ShellEvent::Reply(b"\x1b_Gi=31;OK\x1b\\".to_vec())]);
    }

    #[test]
    fn marks_and_exit_codes() {
        let ev = scan(&[b"hi\x1b]133;C\x07out\x1b]133;D;127\x07\x1b]133;A\x1b\\"]);
        assert_eq!(ev, vec![ShellEvent::CommandStart, ShellEvent::CommandEnd { exit: Some(127) }, ShellEvent::Prompt]);
    }

    #[test]
    fn split_across_reads() {
        let ev = scan(&[b"\x1b]13", b"3;D;0", b"\x07", b"\x1b", b"]7;file://mac/Users/me/My%20Dir\x07"]);
        assert_eq!(ev, vec![ShellEvent::CommandEnd { exit: Some(0) }, ShellEvent::Cwd(PathBuf::from("/Users/me/My Dir"))]);
    }

    #[test]
    fn notifications() {
        let ev = scan(&[b"\x1b]9;Build done\x07\x1b]777;notify;Claude;Needs input\x07\x1b]9;4;1;50\x07"]);
        assert_eq!(
            ev,
            vec![
                ShellEvent::Notify { title: None, body: "Build done".into() },
                ShellEvent::Notify { title: Some("Claude".into()), body: "Needs input".into() },
            ]
        );
    }

    #[test]
    fn ignores_other_escapes() {
        assert!(scan(&[b"\x1b[31mred\x1b[0m\x1b]0;title\x07\x1b]8;;http://x\x07"]).is_empty());
    }

    #[test]
    fn detects_shells_by_file_name() {
        let cases = [
            ("/bin/zsh", Some(ShellKind::Zsh)),
            ("-zsh", Some(ShellKind::Zsh)),
            ("/bin/bash", Some(ShellKind::Bash)),
            ("/opt/homebrew/bin/bash", Some(ShellKind::Bash)),
            ("/opt/homebrew/bin/fish", Some(ShellKind::Fish)),
            ("nu", Some(ShellKind::Nu)),
            ("/usr/local/bin/pwsh", Some(ShellKind::Pwsh)),
            ("pwsh-preview", Some(ShellKind::Pwsh)),
            ("powershell.exe", Some(ShellKind::Pwsh)),
            ("/bin/sh", None),
            ("/usr/bin/tcsh", None),
            ("/opt/zsh/bin/xonsh", None),
            ("/home/fish/bin/elvish", None),
            ("", None),
        ];
        for (shell, kind) in cases {
            assert_eq!(ShellKind::detect(shell), kind, "{shell}");
        }
    }

    fn env_of(i: &Integration) -> std::collections::HashMap<&str, &str> {
        i.env.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
    }

    const DIR: &str = "/S/shell-integration";

    #[test]
    fn zsh_plan() {
        let i = plan(ShellKind::Zsh, Path::new(DIR), |_| None);
        assert_eq!(i.args, None);
        let env = env_of(&i);
        assert_eq!(env["ZDOTDIR"], "/S/shell-integration/zsh");
        assert_eq!(env["KRAKEN_SHELL_INTEGRATION"], "/S/shell-integration/zsh/kraken.zsh");
        assert!(!env.contains_key("KRAKEN_ORIG_ZDOTDIR"));
        let i = plan(ShellKind::Zsh, Path::new(DIR), |k| (k == "ZDOTDIR").then(|| "/Users/me/.config/zsh".into()));
        assert_eq!(env_of(&i)["KRAKEN_ORIG_ZDOTDIR"], "/Users/me/.config/zsh");
    }

    #[test]
    fn bash_plan() {
        let i = plan(ShellKind::Bash, Path::new(DIR), |_| None);
        assert_eq!(i.args, Some(vec!["--rcfile".to_string(), "/S/shell-integration/bash/kraken.bash".to_string()]));
        assert_eq!(i.env, vec![("KRAKEN_BASH_INJECT".to_string(), "login".to_string())]);
    }

    #[test]
    fn fish_and_nu_plan() {
        for kind in [ShellKind::Fish, ShellKind::Nu] {
            let i = plan(kind, Path::new(DIR), |_| None);
            assert_eq!(i.args, None);
            let env = env_of(&i);
            assert_eq!(env["XDG_DATA_DIRS"], "/S/shell-integration/data:/usr/local/share:/usr/share");
            assert_eq!(env["KRAKEN_SHELL_DATA_DIR"], "/S/shell-integration/data");
            assert!(!env.contains_key("KRAKEN_ORIG_XDG_DATA_DIRS"));

            let i = plan(kind, Path::new(DIR), |k| (k == "XDG_DATA_DIRS").then(|| "/a:/b".into()));
            let env = env_of(&i);
            assert_eq!(env["XDG_DATA_DIRS"], "/S/shell-integration/data:/a:/b");
            assert_eq!(env["KRAKEN_ORIG_XDG_DATA_DIRS"], "/a:/b");

            // Set but empty counts as unset.
            let i = plan(kind, Path::new(DIR), |k| (k == "XDG_DATA_DIRS").then(String::new));
            assert!(!env_of(&i).contains_key("KRAKEN_ORIG_XDG_DATA_DIRS"));
        }
    }

    #[test]
    fn pwsh_plan() {
        let i = plan(ShellKind::Pwsh, Path::new("/Users/o'neil/Kraken"), |_| None);
        assert!(i.env.is_empty());
        assert_eq!(
            i.args,
            Some(vec![
                "-Login".to_string(),
                "-NoExit".to_string(),
                "-Command".to_string(),
                ". '/Users/o''neil/Kraken/pwsh/kraken.ps1'".to_string(),
            ])
        );
    }

    #[test]
    fn plans_point_at_installed_files() {
        let dir = std::env::temp_dir().join(format!("kraken-shell-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for kind in [ShellKind::Zsh, ShellKind::Bash, ShellKind::Fish, ShellKind::Nu, ShellKind::Pwsh] {
            install(kind, &dir).unwrap();
            install(kind, &dir).unwrap(); // again: already current
            for (name, body) in kind.files() {
                assert_eq!(std::fs::read_to_string(dir.join(name)).unwrap(), *body);
            }
        }
        assert!(dir.join("zsh/.zshenv").is_file());
        // What the shells look for under the data dir.
        let data = dir.join(DATA_DIR);
        assert!(data.join("fish/vendor_conf.d").join("kraken.fish").is_file());
        assert!(data.join("nushell/vendor/autoload").join("kraken.nu").is_file());
        let bash = plan(ShellKind::Bash, &dir, |_| None).args.unwrap();
        assert!(Path::new(&bash[1]).is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn scripts_emit_the_marks_we_parse() {
        for (name, body) in [ShellKind::Zsh, ShellKind::Bash, ShellKind::Fish, ShellKind::Pwsh].iter().flat_map(|k| k.files()) {
            if name.ends_with(".zshenv") {
                continue;
            }
            for mark in ["133;A", "133;C", "133;D;", "]7;file://"] {
                assert!(body.contains(mark), "{name} lacks {mark}");
            }
        }
        let nu = ShellKind::Nu.files()[0].1;
        assert!(nu.contains("shell_integration.osc133 = true") && nu.contains("shell_integration.osc7 = true"));
    }
}
