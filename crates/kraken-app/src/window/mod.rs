//! A Kraken window: tabs, each holding a tree of split panes, each pane
//! running its own shell.

use crate::actions::Action;
use crate::input;
use crate::open;
use crate::palette::{self, PaletteGeom, PaletteState};
use crate::{App, UserEvent};
use kraken_config::{Config, OptionAsMeta, Rgb, Theme};
use kraken_mux::layout::{self, Dir, Direction, Divider, Node, PaneId, R};
use kraken_mux::session::{NodeSession, TabSession, WindowSession};
use kraken_render::{FontKind, Frame, Rect, Shared, Span, TextLine, WindowRenderer};
use kraken_term::alacritty_terminal::event::Event as TermEvent;
use kraken_term::alacritty_terminal::grid::{Dimensions, Scroll};
use kraken_term::alacritty_terminal::index::{Column, Point, Side};
use kraken_term::alacritty_terminal::selection::{Selection, SelectionType};
use kraken_term::alacritty_terminal::term::TermMode;
use kraken_term::alacritty_terminal::vte::ansi::{ClearMode, Handler};
use kraken_term::shell::ShellEvent;
use kraken_term::status::{self, GitInfo};
use kraken_term::view::{self, Layout};
use kraken_term::{EventProxy, GridSize, Session};
use kraken_ui::agents::{self, Agent, AgentState};
use kraken_ui::anim::{self, ease_out_back, ease_out_cubic};
use kraken_ui::cards::{self, Card, CardHit};
use kraken_ui::links::{self, Link};
use kraken_ui::search::{self, SearchState};
use kraken_ui::sheet::{self, SheetGeom};
use kraken_ui::sidebar::{self, Sidebar, SidebarGeom};
use kraken_ui::splash::{self, SplashGeom};
use kraken_ui::statusbar::{self, BarGeom, STATUS_PT, StatusView};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, Ime, KeyEvent, Modifiers, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersKeyState, NamedKey};
use winit::platform::macos::{OptionAsAlt, WindowAttributesExtMacOS};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::{CursorIcon, UserAttentionType, Window};

mod a11y;
mod chrome;
mod input_router;
mod mux;
mod overlays;
mod pane;
pub mod perf;
mod test_actions;

/// Height of the transparent title bar (traffic lights live here), in points.
const TITLEBAR_PT: f32 = 30.0;
/// Height of the tab strip, shown once there are 2+ tabs.
const TABBAR_PT: f32 = 34.0;
const MAX_TAB_PT: f32 = 230.0;
/// Extra grab area around a divider, in points.
const DIVIDER_GRAB_PT: f32 = 4.0;
const MIN_FONT_PT: f32 = 6.0;
const MAX_FONT_PT: f32 = 72.0;

/// Borrowed app state that window handlers need.
pub struct Ctx<'a> {
    pub shared: &'a mut Shared,
    pub cfg: &'a Config,
    pub clipboard: &'a mut Option<arboard::Clipboard>,
    pub next_id: &'a mut u64,
    pub keymap: &'a crate::keys::Keymap,
}

impl Ctx<'_> {
    fn new_id(&mut self) -> PaneId {
        *self.next_id += 1;
        *self.next_id
    }
}

/// What the app should do after a window handled an event.
pub enum WinAction {
    None,
    /// Write `key = value` (TOML) to the settings file and apply it.
    SetConfig(&'static str, String),
    ReloadConfig,
    CloseWindow,
    NewWindow(Option<PathBuf>),
    Quit,
}

/// What we know about a pane's shell, for the status bar.
#[derive(Default)]
struct PaneStatus {
    /// Foreground program when it isn't the shell itself (e.g. "claude", "vim").
    program: Option<String>,
    /// `user@host` while ssh/mosh runs in the foreground.
    ssh: Option<String>,
    cwd: Option<PathBuf>,
    git_root: Option<PathBuf>,
    running_since: Option<Instant>,
    last: Option<(Option<i32>, Duration)>,
    /// When the last command finished (the status bar flashes its result).
    last_at: Option<Instant>,
    /// CPU % and memory of the running program and its children.
    usage: Option<(f32, u64)>,
    /// Previous CPU-time sample, to turn totals into a percentage.
    cpu_sample: Option<(Instant, u64)>,
}

struct GitEntry {
    info: Option<GitInfo>,
    fetched: Instant,
    pending: bool,
}

const SHELLS: &[&str] = &["zsh", "bash", "fish", "sh", "login", "nu", "tcsh"];
const GIT_STALE: Duration = Duration::from_secs(4);

pub struct Pane {
    st: PaneStatus,
    /// A coding agent running in this pane, if any.
    agent: Option<Agent>,
    /// When output last arrived (agents print constantly while working).
    last_output: Instant,
    /// Where each shell prompt started, as absolute line numbers from the
    /// top of history (from shell-integration marks).
    prompts: Vec<usize>,
    /// When the pane appeared, and which way it grows out (split panes).
    born: Instant,
    grow: Option<Dir>,
    session: Session,
    title: String,
    /// New output arrived while the pane was in a background tab.
    activity: bool,
    bell: bool,
    scroll_accum: f64,
    mouse_held: Option<u8>,
    last_report_cell: Option<(usize, usize)>,
    layout: Layout,
}

/// The branch popover's state: which repository, where it was opened, and
/// the changed files once `git status` has answered.
struct GitPop {
    root: PathBuf,
    anchor_x: f32,
    changes: Option<Vec<kraken_term::status::GitChange>>,
}

struct Overview {
    opened: Instant,
    /// Set when closing; the cards shrink away, then it's gone.
    closing: Option<Instant>,
    selected: usize,
}

struct Tab {
    root: Node,
    focus: PaneId,
    zoom: bool,
    born: Instant,
    /// A name you gave the tab; `None` = automatic (folder / program).
    name: Option<String>,
    /// Tab colour (ANSI index; 99 = theme orange). SSH tabs go orange on their own.
    color: Option<usize>,
}

/// Where a dragged pane would land.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DropTarget {
    /// Beside a pane, on one of its edges.
    Edge(PaneId, Direction),
    /// Trade places with a pane.
    Swap(PaneId),
    /// Into another tab.
    Tab(usize),
    /// Into a brand-new tab.
    NewTab,
}

/// How often a selection dragged past a pane's edge scrolls another step.
const AUTOSCROLL_STEP: Duration = Duration::from_millis(50);

enum Drag {
    /// Moving a pane: `active` once the mouse has travelled a few pixels.
    Pane {
        id: PaneId,
        from: (f32, f32),
        active: bool,
    },
    Divider(Divider),
    Select(PaneId),
    Tab(usize),
}

struct TabHit {
    index: usize,
    rect: R,
    close: Option<R>,
}

pub struct TermWindow {
    pub window: Arc<Window>,
    renderer: WindowRenderer,
    proxy: EventLoopProxy<UserEvent>,
    scale: f32,
    font_pt: f32,
    cell: (f32, f32),
    panes: HashMap<PaneId, Pane>,
    tabs: Vec<Tab>,
    active: usize,
    focused: bool,
    /// Start of the cursor's blink cycle; typing restarts it (cursor shown).
    blink_epoch: Instant,
    modifiers: Modifiers,
    cursor_px: (f32, f32),
    cursor_icon: CursorIcon,
    last_click: Option<(Instant, PaneId, usize, usize, u8)>,
    /// Last autoscroll step while a selection is dragged past a pane's edge.
    autoscroll_at: Instant,
    preedit: bool,
    drag: Option<Drag>,
    /// Geometry from the last layout pass, for hit-testing.
    visible: Vec<(PaneId, R)>,
    dividers: Vec<Divider>,
    tab_hits: Vec<TabHit>,
    plus_hit: Option<R>,
    git: HashMap<PathBuf, GitEntry>,
    // Animation state.
    last_frame: Instant,
    /// Frame timing (`KRAKEN_PERF=1`).
    perf: Option<perf::Perf>,
    /// A frame has been presented (the window is visibly up).
    presented: bool,
    /// Focus crossfade: the pane focused in the last frame, the one before
    /// it, and when it changed.
    focus_drawn: Option<PaneId>,
    focus_prev: Option<PaneId>,
    focus_at: Instant,
    /// Clickable status-bar segments from the last frame (hit, x from, x to).
    status_hits: Vec<(statusbar::StatusHit, f32, f32)>,
    status_hover: Option<statusbar::StatusHit>,
    /// The branch popover (click the branch in the status bar).
    git_pop: Option<GitPop>,
    git_pop_hits: Vec<(R, usize)>,
    git_pop_hover: Option<usize>,
    /// Tab overview (⌘⇧\): open, closing, and which card is chosen.
    overview: Option<Overview>,
    overview_hits: Vec<R>,
    /// Covered, minimized or hidden: draw nothing until it's visible again.
    occluded: bool,
    /// What the once-a-second tick last showed; it only redraws on a change.
    tick_shown: String,
    /// VoiceOver bridge (`None` only in tests).
    a11y: Option<a11y::A11y>,
    /// Active-tab underline: current (x, width), eased toward the active tab.
    indicator: Option<(f32, f32)>,
    indicator_moving: bool,
    splash: Option<Instant>,
    /// Shortcut sheet: (when it started opening/closing, closing?).
    sheet: Option<(Instant, bool)>,
    search: Option<SearchState>,
    palette: Option<PaletteState>,
    /// Palette open/close animation: (start, closing?).
    palette_anim: (Instant, bool),
    /// Theme shown while browsing themes in the palette.
    preview: Option<Theme>,
    /// Short message above the status bar: (text, shown at, is error).
    toast: Option<(String, Instant, bool)>,
    base_font_pt: f32,
    /// Pane under the mouse (shows the drag grip).
    hover: Option<PaneId>,
    transparent: bool,
    sidebar: Option<Sidebar>,
    /// The link under the mouse while ⌘ is held: (pane, row, first col, last col, link).
    link_hover: Option<(PaneId, usize, usize, usize, Link)>,
    /// Files are being dragged over the window from Finder.
    file_hover: bool,
    /// Text being composed in an input method (Chinese, Japanese, Hindi…).
    preedit_text: String,
    /// Renaming a tab: (tab index, text typed so far).
    renaming: Option<(usize, String)>,
    /// Agents dashboard: (opened at, selected row).
    agents_panel: Option<(Instant, usize)>,
    /// "Agent needs you" cards, newest last.
    cards: Vec<Card>,
    card_hits: Vec<(R, CardHit)>,
    /// Focused pane's cursor cell, for IME placement.
    cursor_rect: Option<Rect>,
    frame: Frame,
}

fn rgb(c: Rgb) -> [u8; 3] {
    [c.0, c.1, c.2]
}

impl TermWindow {
    pub fn create(
        app: &mut App,
        event_loop: &ActiveEventLoop,
        cwd: Option<PathBuf>,
        restore: Option<&WindowSession>,
    ) -> Result<TermWindow, Box<dyn std::error::Error>> {
        let cfg = &app.config;
        let option_as_alt = match cfg.option_as_meta {
            OptionAsMeta::None => OptionAsAlt::None,
            OptionAsMeta::Left => OptionAsAlt::OnlyLeft,
            OptionAsMeta::Right => OptionAsAlt::OnlyRight,
            OptionAsMeta::Both => OptionAsAlt::Both,
        };
        let attrs = Window::default_attributes()
            .with_title("Kraken")
            .with_inner_size(LogicalSize::new(cfg.window_width, cfg.window_height))
            .with_min_inner_size(LogicalSize::new(360.0, 200.0))
            .with_titlebar_transparent(true)
            .with_title_hidden(true)
            .with_fullsize_content_view(true)
            .with_option_as_alt(option_as_alt)
            .with_transparent(cfg.opacity < 1.0)
            .with_visible(false);
        let attrs = match restore {
            Some(r) => {
                let a = attrs.with_inner_size(LogicalSize::new(r.width.max(360.0), r.height.max(200.0)));
                match (r.x, r.y) {
                    (Some(x), Some(y)) => a.with_position(PhysicalPosition::new(x, y)),
                    _ => a,
                }
            }
            None => attrs,
        };
        perf::mark("creating window");
        let window = Arc::new(event_loop.create_window(attrs)?);
        perf::mark("window created");
        // VoiceOver: the adapter must exist before the window is first shown.
        let a11y = Some(a11y::A11y::new(event_loop, &window, app.proxy.clone()));
        window.set_ime_allowed(true);

        if app.shared.is_none() {
            let instance =
                wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(Box::new(event_loop.owned_display_handle())));
            let surface = instance.create_surface(window.clone())?;
            let shared = pollster::block_on(Shared::new(instance, &surface, &cfg.font.family, cfg.font.ligatures))?;
            drop(surface);
            app.shared = Some(shared);
        }
        let first_window = app.windows.is_empty();
        let App { shared, config, clipboard, next_id, proxy, keymap, .. } = app;
        let shared = shared.as_mut().ok_or("renderer not initialised")?;
        perf::mark("window + GPU ready");
        let surface = shared.instance.create_surface(window.clone())?;
        let size = window.inner_size();
        let renderer = WindowRenderer::new(shared, surface, size.width, size.height);
        let scale = window.scale_factor() as f32;
        let font_pt = config.font.size;
        let cell = shared.cell_size(font_pt * scale, config.font.line_height);

        let mut tw = TermWindow {
            window,
            renderer,
            proxy: proxy.clone(),
            scale,
            font_pt,
            cell,
            panes: HashMap::new(),
            tabs: Vec::new(),
            active: 0,
            focused: true,
            blink_epoch: Instant::now(),
            modifiers: Modifiers::default(),
            cursor_px: (0.0, 0.0),
            cursor_icon: CursorIcon::Default,
            last_click: None,
            autoscroll_at: Instant::now(),
            preedit: false,
            drag: None,
            visible: Vec::new(),
            dividers: Vec::new(),
            tab_hits: Vec::new(),
            plus_hit: None,
            git: HashMap::new(),
            last_frame: Instant::now(),
            perf: perf::Perf::from_env(),
            presented: false,
            focus_drawn: None,
            focus_prev: None,
            focus_at: Instant::now(),
            status_hits: Vec::new(),
            status_hover: None,
            git_pop: None,
            git_pop_hits: Vec::new(),
            git_pop_hover: None,
            overview: None,
            overview_hits: Vec::new(),
            occluded: false,
            tick_shown: String::new(),
            a11y,
            indicator: None,
            indicator_moving: false,
            splash: None,
            sheet: None,
            search: None,
            palette: None,
            palette_anim: (Instant::now(), false),
            preview: None,
            toast: None,
            base_font_pt: font_pt,
            hover: None,
            transparent: false,
            sidebar: None,
            link_hover: None,
            file_hover: false,
            preedit_text: String::new(),
            renaming: None,
            agents_panel: None,
            cards: Vec::new(),
            card_hits: Vec::new(),
            cursor_rect: None,
            frame: Frame::default(),
        };
        let mut ctx = Ctx { shared, cfg: config, clipboard, next_id, keymap };
        tw.apply_window_style(&mut ctx);
        perf::mark("renderer created");
        match restore {
            Some(r) => tw.restore_tabs(&mut ctx, r),
            None => tw.new_tab(&mut ctx, cwd)?,
        }
        perf::mark("shell spawned");
        if tw.tabs.is_empty() {
            tw.new_tab(&mut ctx, None)?;
        }
        if first_window && ctx.cfg.welcome && std::env::var_os("KRAKEN_NO_WELCOME").is_none() {
            tw.splash = Some(Instant::now());
        }

        // Debug/test hooks.
        if let Ok(text) = std::env::var("KRAKEN_INPUT") {
            tw.write_focused(text.replace("\\n", "\r").into_bytes());
        }
        if let Ok(actions) = std::env::var("KRAKEN_ACTIONS") {
            for a in actions.split('|') {
                tw.run_test_action(&mut ctx, a);
            }
        }

        // Draw once before showing, so the window never flashes white.
        tw.redraw(&mut ctx);
        tw.window.set_visible(true);
        Ok(tw)
    }

    fn titlebar_h(&self) -> f32 {
        TITLEBAR_PT * self.scale
    }

    fn tabbar_h(&self) -> f32 {
        if self.tabs.len() > 1 { TABBAR_PT * self.scale } else { 0.0 }
    }

    fn status_h(&self) -> f32 {
        STATUS_PT * self.scale
    }

    fn inset(&self, cfg: &Config) -> f32 {
        (cfg.padding * self.scale * 0.75).round()
    }

    fn grid_for(&self, r: R, cfg: &Config) -> GridSize {
        let inset = self.inset(cfg);
        let cols = ((r.w - inset * 2.0) / self.cell.0).floor().max(2.0) as usize;
        let rows = ((r.h - inset * 2.0) / self.cell.1).floor().max(1.0) as usize;
        GridSize { cols, rows, cell_w: self.cell.0, cell_h: self.cell.1 }
    }

    /// Recompute pane rectangles for the active tab and resize their grids.
    fn relayout(&mut self, ctx: &mut Ctx) {
        let (w, h) = (self.renderer.config.width as f32, self.renderer.config.height as f32);
        let top = self.titlebar_h() + self.tabbar_h();
        let side = if self.sidebar.is_some() { (sidebar::WIDTH_PT * self.scale).min(w * 0.45).round() } else { 0.0 };
        let area = R { x: side, y: top, w: (w - side).max(1.0), h: (h - top - self.status_h()).max(1.0) };
        let gap = self.scale.max(1.0).round();
        let mut rects = Vec::new();
        let mut dividers = Vec::new();
        if let Some(tab) = self.tabs.get(self.active) {
            if tab.zoom {
                rects.push((tab.focus, area));
            } else {
                tab.root.layout(area, gap, &mut rects, &mut dividers);
            }
        }
        let inset = self.inset(ctx.cfg);
        let focus = self.tabs.get(self.active).map(|t| t.focus);
        for &(id, r) in &rects {
            let grid = self.grid_for(r, ctx.cfg);
            if let Some(p) = self.panes.get_mut(&id) {
                // At a prompt (not mid-command): keep the prompt from re-wrapping.
                if grid.cols < p.session.size.cols && p.st.running_since.is_none() {
                    p.session.trim_prompt_for_narrowing(grid.cols);
                }
                p.layout = Layout {
                    x: r.x + inset,
                    y: r.y + inset,
                    cell_w: self.cell.0,
                    cell_h: self.cell.1,
                    font_px: self.font_pt * self.scale,
                    focused: self.focused && Some(id) == focus,
                    cursor_visible: true,
                };
                p.session.resize(grid);
            }
        }
        self.visible = rects;
        self.dividers = dividers;
        self.settle_cards();
        self.window.request_redraw();
    }

    fn apply_size(&mut self, ctx: &mut Ctx) {
        let size = self.window.inner_size();
        self.renderer.resize(ctx.shared, size.width, size.height);
        self.cell = ctx.shared.cell_size(self.font_pt * self.scale, ctx.cfg.font.line_height);
        self.relayout(ctx);
    }

    fn pane_at(&self, x: f32, y: f32) -> Option<PaneId> {
        self.visible.iter().find(|(_, r)| r.contains(x, y)).map(|(id, _)| *id)
    }

    fn divider_at(&self, x: f32, y: f32) -> Option<&Divider> {
        let g = DIVIDER_GRAB_PT * self.scale;
        self.dividers.iter().find(|d| {
            let r = d.rect;
            R { x: r.x - g, y: r.y - g, w: r.w + 2.0 * g, h: r.h + 2.0 * g }.contains(x, y)
        })
    }

    // ── Panes and tabs ───────────────────────────────────────────────────

    fn focus_id(&self) -> Option<PaneId> {
        self.tabs.get(self.active).map(|t| t.focus)
    }

    fn focused_pane(&self) -> Option<&Pane> {
        self.focus_id().and_then(|id| self.panes.get(&id))
    }

    fn write_focused(&self, bytes: Vec<u8>) {
        if let Some(p) = self.focused_pane() {
            p.session.write(bytes);
        }
    }

    fn focused_cwd(&self) -> Option<PathBuf> {
        self.focused_pane()?.session.cwd()
    }

    /// Opacity and blur from settings.
    fn apply_window_style(&mut self, ctx: &mut Ctx) {
        let want = ctx.cfg.opacity < 1.0;
        if want != self.transparent {
            self.transparent = want && self.renderer.set_transparent(ctx.shared, true);
            if !want {
                self.renderer.set_transparent(ctx.shared, false);
            }
            self.window.set_transparent(self.transparent);
        }
        self.window.set_blur(self.transparent && ctx.cfg.blur);
    }

    /// Settings changed on disk: pick up the new base font size, then redraw.
    pub fn config_changed(&mut self, ctx: &mut Ctx) {
        if (ctx.cfg.font.size - self.base_font_pt).abs() > f32::EPSILON {
            self.base_font_pt = ctx.cfg.font.size;
            self.font_pt = ctx.cfg.font.size;
        }
        self.apply_window_style(ctx);
        self.apply_size(ctx);
        for p in self.panes.values() {
            p.session.apply_config(ctx.cfg);
        }
    }

    fn set_cursor_icon(&mut self, icon: CursorIcon) {
        if self.cursor_icon != icon {
            self.cursor_icon = icon;
            self.window.set_cursor(icon);
        }
    }
}

/// Show a macOS notification (through osascript, so no extra permissions).
fn notify(title: &str, body: &str) {
    let (title, body) = (title.to_string(), body.to_string());
    crate::tasks::spawn("notify", move || {
        let _ = std::process::Command::new("osascript")
            .args([
                "-e",
                "on run argv",
                "-e",
                "display notification (item 2 of argv) with title (item 1 of argv) sound name \"Glass\"",
                "-e",
                "end run",
                &title,
                &body,
            ])
            .output();
    });
}

const TOAST_SHOW: Duration = Duration::from_millis(2600);
const TOAST_FADE: Duration = Duration::from_millis(220);

/// Open a file in the user's editor for its type (falls back to TextEdit).
fn open_in_editor(path: &std::path::Path) {
    let path = path.to_path_buf();
    crate::tasks::spawn("open-settings", move || {
        let ok = std::process::Command::new("open").arg(&path).status().is_ok_and(|s| s.success());
        if !ok {
            let _ = std::process::Command::new("open").arg("-t").arg(&path).status();
        }
    });
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else { format!("{}…", s.chars().take(max).collect::<String>()) }
}

/// East Asian wide characters take two cells.
fn is_wide(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x115F | 0x2E80..=0x303E | 0x3041..=0x33FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6 | 0x1F300..=0x1F64F | 0x1F900..=0x1F9FF | 0x20000..=0x3FFFD)
}
