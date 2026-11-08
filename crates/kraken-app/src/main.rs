//! Kraken — a GPU-accelerated terminal for macOS.

mod actions;
mod clipboard;
mod crash;
mod input;
mod keys;
mod menu;
mod open;
mod palette;
mod tasks;
mod watch;
mod window;

use kraken_config::Config;
use kraken_mux::layout::PaneId;
use kraken_render::Shared;
use kraken_term::alacritty_terminal::event::Event as TermEvent;
use kraken_term::shell::ShellEvent;
use kraken_term::status::GitInfo;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use window::{Ctx, TermWindow, WinAction};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::WindowId;

pub enum UserEvent {
    /// An event from the shell running in a pane.
    Term(WindowId, PaneId, TermEvent),
    /// A shell-integration mark (command start/end, cwd, notification).
    Shell(WindowId, PaneId, ShellEvent, Instant),
    /// A background `git status` finished.
    Git(WindowId, PathBuf, Option<GitInfo>),
    /// The changed-file list for the branch popover arrived.
    GitChanges(WindowId, PathBuf, Option<Vec<kraken_term::status::GitChange>>),
    /// Once a second: refresh status bars.
    Tick,
    /// A menu-bar item was chosen (its id).
    Menu(String),
    /// The settings file was saved (from the file watcher).
    ConfigChanged,
    /// VoiceOver / AccessKit wants something from a window.
    A11y(accesskit_winit::Event),
    /// A pasted image finished saving: type its path into that pane
    /// (`Ok((text, toast))`), or say what went wrong.
    PasteReady(WindowId, PaneId, Result<(String, String), String>),
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(e: accesskit_winit::Event) -> Self {
        UserEvent::A11y(e)
    }
}

pub struct App {
    pub config: Config,
    pub proxy: EventLoopProxy<UserEvent>,
    pub shared: Option<Shared>,
    pub windows: HashMap<WindowId, TermWindow>,
    pub clipboard: Option<arboard::Clipboard>,
    /// Pane ids are unique across all windows.
    pub next_id: u64,
    menu: Option<menu::MenuBar>,
    /// The window menu commands go to.
    last_focused: Option<WindowId>,
    /// A settings error found at launch, shown once the first window opens.
    startup_error: Option<String>,
    /// Last session JSON written, to skip identical saves.
    last_session: String,
    ticks: u64,
    /// Settings file modification time when last loaded: editors fire
    /// several file events per save, and only a real change reloads.
    config_mtime: Option<std::time::SystemTime>,
    /// Watches the settings folder; dropping it stops live reload.
    _config_watcher: Option<notify::RecommendedWatcher>,
    /// Keybindings: defaults plus the settings file's `[keys]`.
    pub keymap: keys::Keymap,
}

impl App {
    fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let (config, mut startup_error) = match Config::load_checked() {
            Ok(c) => (c, None),
            Err(e) => (Config::default(), Some(e)),
        };
        let _config_watcher = Config::path().and_then(|p| watch::config(&p, proxy.clone()));
        let (keymap, key_errors) = keys::Keymap::with_overrides(&config.keys);
        if startup_error.is_none() && !key_errors.is_empty() {
            startup_error = Some(key_errors.join(" · "));
        }
        App {
            keymap,
            config,
            startup_error,
            menu: None,
            last_focused: None,
            proxy,
            shared: None,
            windows: HashMap::new(),
            clipboard: arboard::Clipboard::new().ok(),
            next_id: 0,
            config_mtime: config_mtime(),
            _config_watcher,

            last_session: String::new(),
            ticks: 0,
        }
    }

    /// Snapshot the session for the crash handler and, when `write` is set,
    /// save it to disk if it changed.
    fn save_session(&mut self, write: bool) {
        if !self.config.restore_session || self.windows.is_empty() || std::env::var_os("KRAKEN_NO_RESTORE").is_some() {
            return;
        }
        let json = kraken_mux::session::to_json(self.windows.values().map(|w| w.snapshot()).collect());
        crash::remember_session(&json);
        if write && json != self.last_session {
            kraken_mux::session::save(&json);
            self.last_session = json;
        }
    }

    /// Re-read the settings file and apply it everywhere.
    fn reload_config(&mut self, announce: bool) {
        self.config_mtime = config_mtime();
        match Config::load_checked() {
            Ok(cfg) => {
                log::info!("settings reloaded (theme {})", cfg.theme.name);
                let (keymap, key_errors) = keys::Keymap::with_overrides(&cfg.keys);
                self.keymap = keymap;
                if let Some(menu) = &self.menu {
                    menu.refresh(&self.keymap);
                }
                self.config = cfg;
                let App { windows, shared, config, clipboard, next_id, keymap, .. } = self;
                let Some(shared) = shared.as_mut() else { return };
                shared.set_font_families(&config.font.family, config.font.ligatures);
                let mut ctx = Ctx { shared, cfg: config, clipboard, next_id, keymap };
                for w in windows.values_mut() {
                    w.config_changed(&mut ctx);
                    if !key_errors.is_empty() {
                        w.show_toast(&key_errors.join(" · "), true);
                    } else if announce {
                        w.show_toast(&format!("Settings applied · {}", ctx.cfg.theme.name), false);
                    }
                }
            }
            Err(e) => {
                for w in self.windows.values_mut() {
                    w.show_toast(&e, true);
                }
            }
        }
    }

    pub fn open_window(&mut self, event_loop: &ActiveEventLoop, cwd: Option<std::path::PathBuf>) {
        self.open_window_with(event_loop, cwd, None);
    }

    fn open_window_with(
        &mut self,
        event_loop: &ActiveEventLoop,
        cwd: Option<std::path::PathBuf>,
        restore: Option<&kraken_mux::session::WindowSession>,
    ) {
        match TermWindow::create(self, event_loop, cwd, restore) {
            Ok(w) => {
                self.windows.insert(w.window.id(), w);
            }
            Err(e) => {
                log::error!("could not open window: {e}");
                if self.windows.is_empty() {
                    crash::fatal_alert(&format!("{e}\n\nKraken draws with Metal and needs a GPU that supports it."));
                    event_loop.exit();
                }
            }
        }
    }

    pub fn close_window(&mut self, event_loop: &ActiveEventLoop, id: WindowId) {
        self.windows.remove(&id);
        if self.windows.is_empty() {
            event_loop.exit();
        }
    }

    fn apply(&mut self, event_loop: &ActiveEventLoop, id: WindowId, action: WinAction) {
        match action {
            WinAction::None => {}
            WinAction::CloseWindow => self.close_window(event_loop, id),
            WinAction::NewWindow(cwd) => self.open_window(event_loop, cwd),
            WinAction::Quit => event_loop.exit(),
            WinAction::ReloadConfig => self.reload_config(true),
            WinAction::SetConfig(key, value) => match Config::set_key(key, &value) {
                Ok(()) => self.reload_config(true),
                Err(e) => {
                    if let Some(w) = self.windows.get_mut(&id) {
                        w.show_toast(&format!("Couldn't save settings: {e}"), true);
                    }
                }
            },
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        window::perf::mark("app resumed");
        if self.windows.is_empty() {
            let saved =
                (self.config.restore_session && std::env::var_os("KRAKEN_NO_RESTORE").is_none()).then(kraken_mux::session::load).flatten();
            if let Some(saved) = saved {
                for ws in &saved.windows {
                    self.open_window_with(event_loop, None, Some(ws));
                }
                let (tabs, panes) = self.windows.values().fold((0, 0), |(t, p), w| (t + w.tab_count(), p + w.pane_count()));
                let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
                if let Some(w) = self.windows.values_mut().next() {
                    w.show_toast(&format!("Welcome back · restored {} · {}", plural(tabs, "tab"), plural(panes, "pane")), false);
                }
            }
            if self.windows.is_empty() {
                self.open_window(event_loop, None);
            }
            if let Some(report) = crash::take_previous()
                && let Some(w) = self.windows.values_mut().next()
            {
                w.show_toast(&format!("Kraken crashed last time · your tabs were restored · report: {}", report.display()), true);
            }
            if let Some(e) = self.startup_error.take() {
                for w in self.windows.values_mut() {
                    w.show_toast(&e, true);
                }
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        let App { windows, shared, config, clipboard, next_id, keymap, .. } = self;
        let Some(shared) = shared.as_mut() else { return };
        let mut ctx = Ctx { shared, cfg: config, clipboard, next_id, keymap };
        match event {
            UserEvent::Term(wid, pid, ev) => {
                let Some(w) = windows.get_mut(&wid) else { return };
                let action = w.handle_term_event(&mut ctx, pid, ev);
                self.apply(event_loop, wid, action);
            }
            UserEvent::Shell(wid, pid, ev, at) => {
                if let Some(w) = windows.get_mut(&wid) {
                    w.handle_shell_event(&mut ctx, pid, ev, at);
                }
            }
            UserEvent::Git(wid, root, info) => {
                if let Some(w) = windows.get_mut(&wid) {
                    w.handle_git(root, info);
                }
            }
            UserEvent::Menu(item) => {
                let Some(action) = self.menu.as_ref().and_then(|m| m.action(&item)) else { return };
                let target = self.last_focused.filter(|id| windows.contains_key(id)).or_else(|| windows.keys().next().copied());
                if let Some(wid) = target
                    && let Some(w) = windows.get_mut(&wid)
                {
                    let result = w.menu_action(&mut ctx, action);
                    self.apply(event_loop, wid, result);
                } else if action == actions::Action::NewWindow || action == actions::Action::NewTab {
                    self.open_window(event_loop, None);
                }
            }
            UserEvent::GitChanges(wid, root, changes) => {
                if let Some(w) = windows.get_mut(&wid) {
                    w.handle_git_changes(root, changes);
                }
            }
            UserEvent::PasteReady(wid, pid, result) => {
                if let Some(w) = windows.get_mut(&wid) {
                    w.paste_ready(pid, result);
                }
            }
            UserEvent::A11y(e) => {
                if let Some(w) = windows.get_mut(&e.window_id) {
                    w.handle_a11y_event(e.window_event);
                }
            }
            UserEvent::ConfigChanged => {
                if config_mtime() != self.config_mtime {
                    self.reload_config(true);
                }
            }
            UserEvent::Tick => {
                for w in windows.values_mut() {
                    w.tick(&mut ctx);
                }
                // Keep a fresh session on disk, so even a crash loses little.
                self.ticks += 1;
                self.save_session(self.ticks.is_multiple_of(5));
                // Debug hook: crash on purpose to exercise the crash handler.
                if let Some(after) = std::env::var("KRAKEN_PANIC_AFTER").ok().and_then(|s| s.parse::<u64>().ok())
                    && self.ticks >= after
                {
                    panic!("KRAKEN_PANIC_AFTER={after}: test crash");
                }
            }
        }
    }

    /// Quitting with windows open (⌘Q) keeps the session for next time;
    /// closing the last window yourself means "start fresh".
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if std::env::var_os("KRAKEN_NO_RESTORE").is_some() {
            return;
        }
        if self.windows.is_empty() {
            kraken_mux::session::forget();
        } else {
            self.save_session(true);
        }
    }

    /// Pace animations: redraw windows that need a frame now, sleep until the
    /// soonest future frame otherwise, or wait for input when all are idle.
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // The menu bar (~30 ms to build) waits until the first frame is on
        // screen: the keymap handles every shortcut without it meanwhile.
        if self.menu.is_none() && self.windows.values().any(|w| w.has_presented()) {
            self.menu = Some(menu::build(&self.keymap));
            let proxy = self.proxy.clone();
            muda::MenuEvent::set_event_handler(Some(move |e: muda::MenuEvent| {
                let _ = proxy.send_event(UserEvent::Menu(e.id.0));
            }));
            window::perf::mark("menu bar built (after first frame)");
        }
        let now = Instant::now();
        let mut next: Option<Instant> = None;
        for w in self.windows.values() {
            match w.next_frame(&self.config) {
                Some(t) if t <= now => w.window.request_redraw(),
                Some(t) => next = Some(next.map_or(t, |n| n.min(t))),
                None => {}
            }
        }
        event_loop.set_control_flow(next.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let WindowEvent::Focused(true) = event {
            self.last_focused = Some(id);
        }
        let App { windows, shared, config, clipboard, next_id, keymap, .. } = self;
        let (Some(w), Some(shared)) = (windows.get_mut(&id), shared.as_mut()) else { return };
        let mut ctx = Ctx { shared, cfg: config, clipboard, next_id, keymap };
        let action = w.handle_window_event(&mut ctx, event);
        self.apply(event_loop, id, action);
    }
}

fn main() {
    let _ = window::perf::PROCESS_START.set(Instant::now());
    crash::install();
    window::perf::mark("crash handler installed");
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    // Kraken builds its own menu bar (see menu.rs) instead of winit's default.
    window::perf::mark("logger ready");
    use winit::platform::macos::EventLoopBuilderExtMacOS;
    let event_loop = EventLoop::<UserEvent>::with_user_event().with_default_menu(false).build().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    window::perf::mark("event loop created");
    let mut app = App::new(event_loop.create_proxy());
    window::perf::mark("config + keymap loaded");
    // Pasted images older than a week go (agents read them right after the paste).
    tasks::spawn("clean-pastes", || clipboard::clean_old(&clipboard::pastes_dir(), Duration::from_secs(7 * 24 * 3600)));
    let ticker = event_loop.create_proxy();
    tasks::spawn_long("ticker", move || {
        while ticker.send_event(UserEvent::Tick).is_ok() {
            std::thread::sleep(Duration::from_secs(1));
        }
    });
    event_loop.run_app(&mut app).expect("run app");
}

fn config_mtime() -> Option<std::time::SystemTime> {
    Config::path().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok())
}
