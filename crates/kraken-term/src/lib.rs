//! Terminal sessions: a shell running in a PTY, parsed by `alacritty_terminal`.
//!
//! The PTY is read on a dedicated I/O thread which feeds bytes into the
//! parser and updates the shared [`Term`] grid. The UI thread locks the grid
//! only briefly to take a snapshot for rendering (see [`view`]).

pub mod images;
pub mod shell;
pub mod status;
pub mod view;

use alacritty_terminal::event::{Event, EventListener, Notify, OnResize, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, Notifier};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config as TermConfig, Term};
use alacritty_terminal::tty;
use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;

pub use alacritty_terminal;

/// Forwards terminal events (title changes, bell, new output…) to the UI.
#[derive(Clone)]
pub struct EventProxy(Arc<dyn Fn(Event) + Send + Sync>);

impl EventProxy {
    pub fn new(f: impl Fn(Event) + Send + Sync + 'static) -> Self {
        EventProxy(Arc::new(f))
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        (self.0)(event)
    }
}

/// Grid dimensions in cells plus the cell size in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridSize {
    pub cols: usize,
    pub rows: usize,
    pub cell_w: f32,
    pub cell_h: f32,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

impl From<GridSize> for WindowSize {
    fn from(s: GridSize) -> Self {
        WindowSize { num_lines: s.rows as u16, num_cols: s.cols as u16, cell_width: s.cell_w as u16, cell_height: s.cell_h as u16 }
    }
}

pub type SharedTerm = Arc<FairMutex<Term<EventProxy>>>;

/// The emulator settings that come from kraken.toml.
fn term_config(config: &kraken_config::Config) -> TermConfig {
    use alacritty_terminal::vte::ansi::{CursorShape, CursorStyle};
    use kraken_config::{CursorBlink, CursorShapeSetting};
    let shape = match config.cursor.style {
        CursorShapeSetting::Block => CursorShape::Block,
        CursorShapeSetting::Beam => CursorShape::Beam,
        CursorShapeSetting::Underline => CursorShape::Underline,
    };
    TermConfig {
        scrolling_history: config.scrollback,
        default_cursor_style: CursorStyle { shape, blinking: config.cursor.blink == CursorBlink::On },
        // Apps can ask for the Kitty keyboard protocol (Shift+Enter, releases…).
        kitty_keyboard: true,
        ..Default::default()
    }
}

pub struct Session {
    pub term: SharedTerm,
    /// Inline images shown in this pane.
    pub images: images::SharedImages,
    notifier: Notifier,
    pub size: GridSize,
    /// PID of the shell process.
    pub shell_pid: u32,
}

impl Session {
    /// Apply changed settings (scrollback size, default cursor) to a running pane.
    pub fn apply_config(&self, config: &kraken_config::Config) {
        self.term.lock().set_options(term_config(config));
    }

    /// Should this pane's cursor blink right now? `blink = "app"` follows
    /// what the program asked for (DECSCUSR), the others force it.
    pub fn cursor_blinks(&self, config: &kraken_config::Config) -> bool {
        match config.cursor.blink {
            kraken_config::CursorBlink::Off => false,
            kraken_config::CursorBlink::On => true,
            kraken_config::CursorBlink::App => self.term.lock().cursor_style().blinking,
        }
    }

    pub fn spawn(
        config: &kraken_config::Config,
        size: GridSize,
        proxy: EventProxy,
        shell_events: shell::ShellListener,
        id: u64,
        working_directory: Option<PathBuf>,
    ) -> std::io::Result<Session> {
        tty::setup_env();
        let mut opts = tty::Options {
            shell: config.shell.clone().map(|s| tty::Shell::new(s, vec!["-l".into()])),
            working_directory: working_directory.or_else(dirs_home),
            ..Default::default()
        };
        opts.env.insert("TERM".into(), "xterm-256color".into());
        opts.env.insert("COLORTERM".into(), "truecolor".into());
        opts.env.insert("TERM_PROGRAM".into(), "Kraken".into());
        opts.env.insert("TERM_PROGRAM_VERSION".into(), env!("CARGO_PKG_VERSION").into());
        // Apps launched from Finder get no locale; without one, zsh mangles UTF-8.
        if std::env::var_os("LANG").is_none() {
            opts.env.insert("LANG".into(), "en_US.UTF-8".into());
        }

        if config.shell_integration {
            let shell = config.shell.clone().or_else(|| std::env::var("SHELL").ok()).unwrap_or_default();
            if let Some(integration) = shell::integration(&shell) {
                opts.env.extend(integration.env);
                if let Some(args) = integration.args {
                    opts.shell = Some(tty::Shell::new(shell, args));
                }
            }
        }

        let pty = tty::new(&opts, size.into(), id)?;
        let shell_pid = pty.child().id();
        let images = images::ImageStore::new(images::Metrics { cell_w: size.cell_w, cell_h: size.cell_h, cols: size.cols });
        let pty = shell::ScanPty::new(pty, shell_events, Some(images.clone()))?;

        let term = Arc::new(FairMutex::new(Term::new(term_config(config), &size, proxy.clone())));
        let event_loop = EventLoop::new(term.clone(), proxy, pty, false, false)?;
        let notifier = Notifier(event_loop.channel());
        event_loop.spawn();

        Ok(Session { term, images, notifier, size, shell_pid })
    }

    /// Send bytes to the shell (keyboard input, paste, replies).
    pub fn write(&self, bytes: impl Into<Cow<'static, [u8]>>) {
        self.notifier.notify(bytes);
    }

    /// Working directory of whatever runs in the foreground (follows `cd`).
    pub fn cwd(&self) -> Option<PathBuf> {
        let pid = self.foreground().map(|f| f.pid).unwrap_or(self.shell_pid);
        process_cwd(pid).or_else(|| process_cwd(self.shell_pid))
    }

    /// The process group currently in the foreground of this terminal:
    /// the shell at a prompt, or e.g. `vim`, `ssh`, `claude` while they run.
    pub fn foreground(&self) -> Option<ForegroundProcess> {
        foreground_process(self.shell_pid)
    }

    /// Before the pane gets narrower while a prompt is showing: cut the
    /// prompt's lines to the new width so they don't re-wrap. Otherwise the
    /// shell's prompt redraw lands a line off and leaves a duplicate behind.
    pub fn trim_prompt_for_narrowing(&self, new_cols: usize) {
        use alacritty_terminal::index::{Column, Line};
        use alacritty_terminal::term::cell::{Cell, Flags};
        let mut term = self.term.lock();
        let cols = term.columns();
        if new_cols >= cols {
            return;
        }
        let top = term.topmost_line();
        let cursor_line = term.grid().cursor.point.line;
        let is_prompt = |term: &alacritty_terminal::Term<EventProxy>, line: Line| {
            let row = &term.grid()[line];
            (0..cols).any(|c| row[Column(c)].hyperlink().is_some_and(|h| h.uri() == shell::PROMPT_TAG))
        };
        if !is_prompt(&term, cursor_line) {
            return;
        }
        let mut start = cursor_line;
        while start > top && is_prompt(&term, start - 1) {
            start -= 1;
        }
        let mut line = start;
        while line <= cursor_line {
            let row = &mut term.grid_mut()[line];
            for c in new_cols..cols {
                row[Column(c)] = Cell::default();
            }
            row[Column(cols - 1)].flags.remove(Flags::WRAPLINE);
            line += 1;
        }
    }

    pub fn resize(&mut self, size: GridSize) {
        if size == self.size {
            return;
        }
        self.size = size;
        if let Ok(mut im) = self.images.lock() {
            im.metrics = images::Metrics { cell_w: size.cell_w, cell_h: size.cell_h, cols: size.cols };
        }
        self.term.lock().resize(size);
        self.notifier.on_resize(size.into());
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.notifier.0.send(alacritty_terminal::event_loop::Msg::Shutdown);
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForegroundProcess {
    pub pid: u32,
    pub name: String,
}

/// Find the foreground process group of the terminal that `pid` belongs to.
/// On macOS the shell is started via `login`, whose BSD info records the
/// controlling terminal's foreground process group (`e_tpgid`). A group's
/// id is its leader's pid.
#[cfg(target_os = "macos")]
pub fn foreground_process(pid: u32) -> Option<ForegroundProcess> {
    // `login` runs as root, so its details are hidden from us; its child (the
    // user's shell) is ours and carries the same terminal information.
    let info = bsd_info(pid).or_else(|| child_pids(pid).into_iter().find_map(bsd_info))?;
    let fg = if info.e_tpgid != 0 { info.e_tpgid } else { info.pbi_pid };
    let fg_info = bsd_info(fg)?;
    let raw = if fg_info.pbi_name[0] != 0 { &fg_info.pbi_name[..] } else { &fg_info.pbi_comm[..] };
    let bytes: Vec<u8> = raw.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    Some(ForegroundProcess { pid: fg, name: String::from_utf8_lossy(&bytes).trim_start_matches('-').to_string() })
}

/// CPU time (nanoseconds) and resident memory (bytes) of a process and its
/// children and grandchildren (`cargo` → `rustc`, `npm` → `node`…).
#[cfg(target_os = "macos")]
pub fn process_tree_usage(pid: u32) -> Option<(u64, u64)> {
    let mut pids = vec![pid];
    for p in child_pids(pid) {
        pids.push(p);
        pids.extend(child_pids(p));
    }
    let mut found = false;
    let (mut cpu, mut rss) = (0u64, 0u64);
    for p in pids {
        if let Some((c, r)) = task_usage(p) {
            found = true;
            cpu += c;
            rss += r;
        }
    }
    found.then_some((cpu, rss))
}

#[cfg(not(target_os = "macos"))]
pub fn process_tree_usage(_pid: u32) -> Option<(u64, u64)> {
    None
}

#[cfg(target_os = "macos")]
fn task_usage(pid: u32) -> Option<(u64, u64)> {
    // SAFETY: proc_taskinfo is plain integers; all-zero is valid.
    let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_taskinfo>() as i32;
    // SAFETY: `info` is a live, correctly sized out-parameter for PROC_PIDTASKINFO.
    let n = unsafe { libc::proc_pidinfo(pid as i32, libc::PROC_PIDTASKINFO, 0, &mut info as *mut _ as *mut libc::c_void, size) };
    if n != size {
        return None;
    }
    // Task times are in mach ticks (≈41.7 ns on Apple Silicon, 1 ns on Intel).
    static TIMEBASE: std::sync::OnceLock<(u64, u64)> = std::sync::OnceLock::new();
    let (numer, denom) = *TIMEBASE.get_or_init(|| {
        #[repr(C)]
        struct Timebase {
            numer: u32,
            denom: u32,
        }
        // libSystem's stable C API (libc only offers a deprecated binding).
        unsafe extern "C" {
            fn mach_timebase_info(info: *mut Timebase) -> i32;
        }
        let mut tb = Timebase { numer: 1, denom: 1 };
        // SAFETY: `tb` matches `mach_timebase_info_data_t` and is a live out-parameter.
        unsafe { mach_timebase_info(&mut tb) };
        (tb.numer.max(1) as u64, tb.denom.max(1) as u64)
    });
    let ticks = info.pti_total_user + info.pti_total_system;
    Some(((ticks as u128 * numer as u128 / denom as u128) as u64, info.pti_resident_size))
}

#[cfg(target_os = "macos")]
fn child_pids(pid: u32) -> Vec<u32> {
    let mut buf = [0i32; 64];
    // SAFETY: the buffer pointer and its byte length describe `buf`, which outlives the call.
    let n = unsafe { libc::proc_listchildpids(pid as i32, buf.as_mut_ptr() as *mut libc::c_void, std::mem::size_of_val(&buf) as i32) };
    buf.iter().take(n.max(0) as usize).filter(|&&p| p > 0).map(|&p| p as u32).collect()
}

#[cfg(target_os = "macos")]
fn bsd_info(pid: u32) -> Option<libc::proc_bsdinfo> {
    // SAFETY: proc_bsdinfo is a C struct of integers and char arrays; all-zero is valid.
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
    // SAFETY: `info` is a live, correctly sized out-parameter for PROC_PIDTBSDINFO.
    let n = unsafe { libc::proc_pidinfo(pid as i32, libc::PROC_PIDTBSDINFO, 0, &mut info as *mut _ as *mut libc::c_void, size) };
    (n == size).then_some(info)
}

#[cfg(not(target_os = "macos"))]
pub fn foreground_process(_pid: u32) -> Option<ForegroundProcess> {
    None
}

/// Working directory of a process, via `proc_pidinfo` (macOS).
#[cfg(target_os = "macos")]
pub fn process_cwd(pid: u32) -> Option<PathBuf> {
    use std::ffi::CStr;
    // SAFETY: proc_vnodepathinfo is a C struct of integers and char arrays; all-zero is valid.
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as i32;
    // SAFETY: `info` is a live, correctly sized out-parameter for PROC_PIDVNODEPATHINFO.
    let n = unsafe { libc::proc_pidinfo(pid as i32, libc::PROC_PIDVNODEPATHINFO, 0, &mut info as *mut _ as *mut libc::c_void, size) };
    if n != size {
        return None;
    }
    // Bounded: stops at the buffer's end even if the kernel left no NUL.
    let raw: Vec<u8> = info.pvi_cdir.vip_path.iter().flatten().map(|&c| c as u8).collect();
    let path = CStr::from_bytes_until_nul(&raw).ok()?;
    Some(PathBuf::from(path.to_string_lossy().into_owned()))
}

#[cfg(not(target_os = "macos"))]
pub fn process_cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn cwd_of_self() {
        let me = super::process_cwd(std::process::id()).unwrap();
        assert_eq!(me, std::env::current_dir().unwrap());
    }
}

#[cfg(all(test, target_os = "macos"))]
mod proc_tests {
    #[test]
    fn usage_counts_real_cpu_time() {
        let pid = std::process::id();
        let (cpu0, rss) = super::process_tree_usage(pid).expect("usage of this process");
        assert!(rss > 1024 * 1024, "resident size {rss}");
        // Burn ~150 ms of CPU; the measured time must grow by about that much
        // (catches mach-tick vs nanosecond mix-ups, which are ~40× off).
        let t = std::time::Instant::now();
        let mut x = 0u64;
        while t.elapsed() < std::time::Duration::from_millis(150) {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
        }
        std::hint::black_box(x);
        let (cpu1, _) = super::process_tree_usage(pid).unwrap();
        let ms = (cpu1 - cpu0) as f64 / 1e6;
        assert!((100.0..1000.0).contains(&ms), "measured {ms} ms of CPU for ~150 ms of work");
    }

    #[test]
    fn process_cwd_reads_our_own_directory() {
        let cwd = super::process_cwd(std::process::id()).expect("cwd of this process");
        assert_eq!(cwd.canonicalize().unwrap(), std::env::current_dir().unwrap().canonicalize().unwrap());
    }
}
