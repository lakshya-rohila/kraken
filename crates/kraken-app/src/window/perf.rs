//! `KRAKEN_PERF=1`: once a second, print what frames cost, plus the
//! startup time and memory — so performance claims are measured, not guessed.
//!
//! ```text
//! [perf] startup 142 ms to first frame
//! [perf] 60 fps · build 0.4/1.1 ms · render 2.1/3.0 ms · key→present 4.2/6.8 ms (12 keys) · memory 72 MB
//! ```
//! (`avg/max`; "key→present" is from the key event reaching Kraken to the
//! frame showing it being handed to the compositor: Kraken's share of input
//! latency, without the keyboard, OS and display.)

use std::time::{Duration, Instant};

/// When the process started (set first thing in `main`).
pub static PROCESS_START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// `KRAKEN_PERF=1`: print how long after process start `label` happened.
pub fn mark(label: &str) {
    if std::env::var_os("KRAKEN_PERF").is_some()
        && let Some(start) = PROCESS_START.get()
    {
        eprintln!("[perf]   {:>4} ms  {label}", start.elapsed().as_millis());
    }
}

#[derive(Default)]
struct Stat {
    total: Duration,
    max: Duration,
    n: u32,
}

impl Stat {
    fn add(&mut self, d: Duration) {
        self.total += d;
        self.max = self.max.max(d);
        self.n += 1;
    }
    fn show(&self) -> String {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        if self.n == 0 { "-".into() } else { format!("{:.1}/{:.1} ms", ms(self.total / self.n), ms(self.max)) }
    }
}

pub struct Perf {
    window_start: Instant,
    build: Stat,
    prepare: Stat,
    render: Stat,
    shaped: usize,
    latency: Stat,
    /// The oldest key press not yet on screen.
    pending_key: Option<Instant>,
    first_frame_done: bool,
}

impl Perf {
    pub fn from_env() -> Option<Perf> {
        std::env::var_os("KRAKEN_PERF").map(|_| Perf {
            window_start: Instant::now(),
            build: Stat::default(),
            prepare: Stat::default(),
            render: Stat::default(),
            shaped: 0,
            latency: Stat::default(),
            pending_key: None,
            first_frame_done: false,
        })
    }

    pub fn key_pressed(&mut self) {
        self.pending_key.get_or_insert_with(Instant::now);
    }

    /// Record one frame: draw-list build, renderer CPU prep (shaping, atlas,
    /// uploads), and the whole render call (prep + waiting for vsync + submit).
    pub fn frame(&mut self, build: Duration, prepare: Duration, render: Duration, shaped: usize) {
        self.shaped += shaped;
        let now = Instant::now();
        if !self.first_frame_done {
            self.first_frame_done = true;
            if let Some(start) = PROCESS_START.get() {
                eprintln!("[perf] startup {} ms to first frame", (now - *start).as_millis());
            }
        }
        self.build.add(build);
        self.prepare.add(prepare);
        self.render.add(render);
        if let Some(at) = self.pending_key.take() {
            self.latency.add(now - at);
        }
        if now - self.window_start >= Duration::from_secs(1) {
            let fps = self.build.n as f64 / (now - self.window_start).as_secs_f64();
            eprintln!(
                "[perf] {fps:.0} fps · build {} · prep {} ({:.0} lines shaped/frame) · render+vsync {} · key→present {} ({} keys) · memory {}",
                self.build.show(),
                self.prepare.show(),
                self.shaped as f64 / self.build.n.max(1) as f64,
                self.render.show(),
                self.latency.show(),
                self.latency.n,
                footprint_mb().map_or("?".into(), |m| format!("{m} MB")),
            );
            self.window_start = now;
            self.build = Stat::default();
            self.prepare = Stat::default();
            self.shaped = 0;
            self.render = Stat::default();
            self.latency = Stat::default();
        }
    }
}

/// This process's physical memory footprint — what Activity Monitor shows.
/// (RSS also counts shared framework pages, overstating it ~2×.)
fn footprint_mb() -> Option<u64> {
    // SAFETY: rusage_info_v2 is plain integers, so all-zero is valid.
    let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a live rusage_info_v2, matching the RUSAGE_INFO_V2 flavor.
    let ok =
        unsafe { libc::proc_pid_rusage(std::process::id() as i32, libc::RUSAGE_INFO_V2, &mut info as *mut _ as *mut libc::rusage_info_t) };
    (ok == 0).then_some(info.ri_phys_footprint / (1024 * 1024))
}
