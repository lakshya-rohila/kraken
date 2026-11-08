//! What happens when Kraken panics.
//!
//! A panic on the main thread (inside a winit callback) aborts the whole app,
//! taking every tab with it. The hook installed here runs first and:
//!
//! 1. writes the most recent session snapshot to disk, so relaunching brings
//!    back your windows, tabs and splits;
//! 2. saves a crash report (message, location, backtrace) to
//!    `~/Library/Logs/Kraken/`;
//! 3. leaves a marker so the next launch can say what happened.
//!
//! Panics on worker threads are caught by [`crate::tasks`] and never get here.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// The first panic gets the report; later ones (a panic while unwinding out
/// of an AppKit callback panics again) are appended to it.
static PANICKED: AtomicBool = AtomicBool::new(false);
static REPORT: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The latest session JSON, kept fresh by the app every tick.
static LAST_SESSION: Mutex<String> = Mutex::new(String::new());

fn logs_dir() -> Option<PathBuf> {
    dirs_home().map(|h| h.join("Library/Logs/Kraken"))
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn marker() -> Option<PathBuf> {
    logs_dir().map(|d| d.join("last-crash"))
}

/// Remember the session so the panic hook can save it.
pub fn remember_session(json: &str) {
    // Never block: the hook may run while another thread holds this lock.
    if let Ok(mut s) = LAST_SESSION.try_lock()
        && *s != json
    {
        json.clone_into(&mut s);
    }
}

pub fn install() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".into());
        let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())).unwrap_or_default();

        // 1. Session first: it is what the user loses otherwise.
        if let Ok(json) = LAST_SESSION.try_lock()
            && !json.is_empty()
        {
            kraken_mux::session::save(&json);
        }

        // 2. Crash report (the first panic is the interesting one).
        let backtrace = std::backtrace::Backtrace::force_capture();
        if PANICKED.swap(true, Ordering::SeqCst) {
            if let Ok(report) = REPORT.try_lock()
                && let Some(path) = report.as_ref()
            {
                use std::io::Write;
                if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(path) {
                    let _ = write!(f, "\n--- then, on thread {thread} at {location}: {payload}\n");
                }
            }
        } else if let Some(dir) = logs_dir()
            && std::fs::create_dir_all(&dir).is_ok()
        {
            let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let path = dir.join(format!("crash-{secs}-{}.log", std::process::id()));
            let report = format!(
                "Kraken {} crashed\nthread: {thread}\nat: {location}\nmessage: {payload}\nmacOS: {}\n\nbacktrace:\n{backtrace}\n",
                env!("CARGO_PKG_VERSION"),
                os_version(),
            );
            if std::fs::write(&path, report).is_ok() {
                if let Some(m) = marker() {
                    let _ = std::fs::write(m, path.to_string_lossy().as_bytes());
                }
                if let Ok(mut r) = REPORT.try_lock() {
                    *r = Some(path);
                }
            }
        }
        default_hook(info);
    }));
}

/// The crash report from the previous run, if it crashed (asked once).
pub fn take_previous() -> Option<PathBuf> {
    let m = marker()?;
    let path = std::fs::read_to_string(&m).ok()?;
    let _ = std::fs::remove_file(&m);
    Some(PathBuf::from(path.trim()))
}

/// A blocking native alert for errors Kraken can't recover from (no GPU…).
pub fn fatal_alert(message: &str) {
    log::error!("{message}");
    let _ = std::process::Command::new("osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display alert \"Kraken can't start\" message (item 1 of argv) as critical",
            "-e",
            "end run",
            message,
        ])
        .status();
}

fn os_version() -> String {
    std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}
