//! Input routing: window events, keys (overlays first, then the
//! keymap, then the shell), mouse, scrolling, and running [`Action`]s.

use super::*;

impl TermWindow {
    pub(super) fn agents_key(&mut self, ctx: &mut Ctx, ev: &KeyEvent) -> WinAction {
        let rows = self.agent_rows();
        let Some((_, sel)) = self.agents_panel.as_mut() else { return WinAction::None };
        match &ev.logical_key {
            Key::Named(NamedKey::ArrowUp) => *sel = sel.saturating_sub(1),
            Key::Named(NamedKey::ArrowDown) => *sel = (*sel + 1).min(rows.len().saturating_sub(1)),
            Key::Named(NamedKey::Enter) => {
                if let Some(r) = rows.get(*sel) {
                    self.select_tab(ctx, r.tab);
                    self.set_focus(ctx, r.pane);
                    if let Some(a) = self.panes.get_mut(&r.pane).and_then(|p| p.agent.as_mut()) {
                        a.attention = false;
                    }
                }
                self.agents_panel = None;
            }
            _ => self.agents_panel = None, // Esc or anything else closes
        }
        self.window.request_redraw();
        WinAction::None
    }

    /// Open a link (URL, file, folder) the way you'd expect.
    pub(super) fn open_link(&mut self, ctx: &mut Ctx, link: Link) {
        match link {
            Link::Url(u) => {
                open::open_url(&u);
                self.show_toast(&format!("Opening {}", truncate(&u, 60)), false);
            }
            Link::File { path, .. } if path.is_dir() => {
                if let Some(sb) = self.sidebar.as_mut() {
                    sb.pinned = true;
                    sb.set_root(path.clone());
                    self.show_toast(&format!("Browsing {}", path.display()), false);
                } else {
                    open::open_url(&path.to_string_lossy());
                }
            }
            Link::File { path, line, col } => {
                self.perform(ctx, Action::OpenFile(path, line, col));
            }
        }
    }

    /// Which link is under the mouse while ⌘ is held.
    pub(super) fn update_link_hover(&mut self) {
        let mods = self.modifiers.state();
        let (x, y) = self.cursor_px;
        let found = (mods.super_key() && !mods.alt_key()).then(|| self.pane_at(x, y)).flatten().and_then(|id| {
            let p = self.panes.get(&id)?;
            let s = p.session.size;
            let (row, col, _) = p.layout.cell_at(x as f64, y as f64, s.cols, s.rows);
            let cwd = p.st.cwd.clone().or_else(|| p.session.cwd());
            let term = p.session.term.lock();
            let line = kraken_term::alacritty_terminal::index::Line(row as i32 - term.grid().display_offset() as i32);
            let grid_row = &term.grid()[line];
            // Programs can mark links explicitly (OSC 8 hyperlinks).
            if let Some(h) = grid_row[Column(col)].hyperlink().filter(|h| h.uri() != kraken_term::shell::PROMPT_TAG) {
                let uri = h.uri().to_string();
                let same = |c: usize| grid_row[Column(c)].hyperlink().is_some_and(|o| o.uri() == uri);
                let (mut c0, mut c1) = (col, col);
                while c0 > 0 && same(c0 - 1) {
                    c0 -= 1;
                }
                while c1 + 1 < s.cols && same(c1 + 1) {
                    c1 += 1;
                }
                let link = links::classify(&uri, cwd.as_deref()).unwrap_or(Link::Url(uri));
                return Some((id, row, c0, c1, link));
            }
            let mut chars = Vec::with_capacity(s.cols);
            let mut cols = Vec::with_capacity(s.cols);
            for c in 0..s.cols {
                let cell = &grid_row[Column(c)];
                if cell.flags.contains(kraken_term::alacritty_terminal::term::cell::Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                chars.push(if cell.c == '\0' { ' ' } else { cell.c });
                cols.push(c);
            }
            drop(term);
            links::find_at(&chars, &cols, col, cwd.as_deref()).map(|(l, c0, c1)| (id, row, c0, c1, l))
        });
        let changed = found.as_ref().map(|f| (f.0, f.1, f.2, f.3)) != self.link_hover.as_ref().map(|f| (f.0, f.1, f.2, f.3));
        self.link_hover = found;
        if changed {
            self.window.request_redraw();
        }
    }

    /// A command chosen from the menu bar. Paste goes into whatever text box
    /// is open (search, palette, rename); everything else runs as usual.
    pub fn menu_action(&mut self, ctx: &mut Ctx, action: Action) -> WinAction {
        if action == Action::Paste {
            let text = ctx.clipboard.as_mut().and_then(|cb| cb.get_text().ok()).unwrap_or_default();
            let first = text.lines().next().unwrap_or("").to_string();
            if let Some((_, buf)) = self.renaming.as_mut() {
                buf.push_str(&first);
                self.window.request_redraw();
                return WinAction::None;
            }
            if let Some(st) = self.palette.as_mut().filter(|_| !self.palette_anim.1) {
                st.query.push_str(&first);
                st.refilter();
                self.window.request_redraw();
                return WinAction::None;
            }
            if let Some(st) = self.search.as_mut() {
                st.query.push_str(&first);
                st.edited();
                self.window.request_redraw();
                return WinAction::None;
            }
        }
        self.perform(ctx, action)
    }

    /// Keys while typing a tab name: ↩ saves, Esc cancels, empty = automatic name.
    pub(super) fn rename_key(&mut self, ev: &KeyEvent) -> WinAction {
        let mods = self.modifiers.state();
        let Some((tab, text)) = self.renaming.as_mut() else { return WinAction::None };
        match &ev.logical_key {
            Key::Named(NamedKey::Escape) => self.renaming = None,
            Key::Named(NamedKey::Enter) => {
                let (tab, name) = (*tab, text.trim().to_string());
                if let Some(t) = self.tabs.get_mut(tab) {
                    t.name = (!name.is_empty()).then_some(name.clone());
                }
                self.renaming = None;
                self.show_toast(if name.is_empty() { "Tab name cleared · showing the folder again" } else { "Tab renamed" }, false);
            }
            Key::Named(NamedKey::Backspace) => {
                if mods.super_key() || mods.alt_key() {
                    text.clear();
                } else {
                    text.pop();
                }
            }
            _ if mods.super_key() || mods.control_key() => {}
            _ => {
                if let Some(t) = ev.text.as_ref().filter(|t| t.chars().all(|c| !c.is_control()))
                    && text.chars().count() < 40
                {
                    text.push_str(t);
                }
            }
        }
        self.window.request_redraw();
        WinAction::None
    }

    /// Keys while the sidebar has focus.
    pub(super) fn sidebar_key(&mut self, ctx: &mut Ctx, ev: &KeyEvent) -> Option<WinAction> {
        let mods = self.modifiers.state();
        if mods.super_key() || mods.control_key() {
            return None; // shortcuts still work
        }
        let geom = self.sidebar_geom()?;
        let sb = self.sidebar.as_mut()?;
        let mut action = None;
        match &ev.logical_key {
            Key::Named(NamedKey::Escape) => sb.focused = false,
            Key::Named(NamedKey::ArrowUp) => sb.move_by(-1),
            Key::Named(NamedKey::ArrowDown) => sb.move_by(1),
            Key::Named(NamedKey::PageUp) => sb.move_by(-(geom.visible_rows() as i32)),
            Key::Named(NamedKey::PageDown) => sb.move_by(geom.visible_rows() as i32),
            Key::Named(NamedKey::ArrowRight) => sb.expand(),
            Key::Named(NamedKey::ArrowLeft) => sb.collapse(),
            Key::Named(NamedKey::Backspace) => sb.go_up(),
            Key::Named(NamedKey::Space) => {
                if let Some(e) = sb.selected_entry() {
                    open::quick_look(&e.path);
                }
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(e) = sb.selected_entry().cloned() {
                    if mods.shift_key() {
                        action = Some(Action::InsertPath(e.path));
                    } else if e.is_dir {
                        sb.toggle();
                    } else if mods.alt_key() {
                        action = Some(Action::EditInTerminal(e.path, None));
                    } else {
                        action = Some(Action::OpenFile(e.path, None, None));
                    }
                }
            }
            _ => {}
        }
        if let Some(sb) = self.sidebar.as_mut() {
            sidebar::clamp_scroll(sb, &geom);
        }
        self.window.request_redraw();
        Some(action.map_or(WinAction::None, |a| self.perform(ctx, a)))
    }

    pub fn handle_window_event(&mut self, ctx: &mut Ctx, event: WindowEvent) -> WinAction {
        if let Some(a) = self.a11y.as_mut() {
            a.process_event(&self.window, &event);
        }
        match event {
            WindowEvent::CloseRequested => return WinAction::CloseWindow,
            WindowEvent::RedrawRequested if !self.occluded => self.redraw(ctx),
            WindowEvent::RedrawRequested => {}
            WindowEvent::Resized(_) => self.apply_size(ctx),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale = scale_factor as f32;
                self.apply_size(ctx);
            }
            WindowEvent::Occluded(hidden) => {
                // Nobody can see it: stop drawing. Output still flows into the
                // terminal; one fresh frame is drawn when it's visible again.
                self.occluded = hidden;
                if !hidden {
                    self.window.request_redraw();
                }
            }
            WindowEvent::Focused(f) => {
                self.focused = f;
                // Coming back: refresh the branch info that was paused while away.
                if f && let Some(root) = self.focused_pane().and_then(|p| p.st.git_root.clone()) {
                    self.request_git(root, false);
                }
                self.blink_epoch = Instant::now();
                if let Some(p) = self.focused_pane()
                    && p.session.term.lock().mode().contains(TermMode::FOCUS_IN_OUT)
                {
                    p.session.write(if f { b"\x1b[I".to_vec() } else { b"\x1b[O".to_vec() });
                }
                self.relayout(ctx);
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m;
                self.update_link_hover();
            }
            WindowEvent::HoveredFile(_) => {
                self.file_hover = true;
                self.window.request_redraw();
            }
            WindowEvent::HoveredFileCancelled => {
                self.file_hover = false;
                self.window.request_redraw();
            }
            WindowEvent::DroppedFile(path) => {
                // Paste the path (quoted) into the pane it was dropped on.
                self.file_hover = false;
                if let Some(id) = self.pane_at(self.cursor_px.0, self.cursor_px.1) {
                    self.set_focus(ctx, id);
                }
                // As a paste (bracketed when the app asked), like other terminals:
                // agents then treat a dropped image as an attachment.
                if let Some(id) = self.focus_id() {
                    self.paste_into(id, &format!("{} ", open::shell_quote(&path)));
                }
                self.window.focus_window();
                self.window.request_redraw();
            }
            WindowEvent::Ime(ime) => match ime {
                Ime::Preedit(text, _) => {
                    self.preedit = !text.is_empty();
                    self.preedit_text = text;
                    self.window.request_redraw();
                }
                Ime::Commit(text) => {
                    self.preedit = false;
                    self.preedit_text.clear();
                    self.scroll_focused_to_bottom();
                    self.write_focused(text.into_bytes());
                }
                _ => {}
            },
            WindowEvent::KeyboardInput { event, .. } => return self.on_key(ctx, event),
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_px = (position.x as f32, position.y as f32);
                self.on_mouse_move(ctx);
            }
            WindowEvent::MouseInput { state, button, .. } => return self.on_mouse_button(ctx, state, button),
            WindowEvent::MouseWheel { delta, .. } => {
                if let (Some(g), Some(sb)) = (self.sidebar_geom(), self.sidebar.as_mut())
                    && g.rect.contains(self.cursor_px.0, self.cursor_px.1)
                {
                    let lines = match delta {
                        MouseScrollDelta::LineDelta(_, y) => (y * 3.0) as i32,
                        MouseScrollDelta::PixelDelta(p) => (p.y / g.row_h() as f64) as i32,
                    };
                    let max = sb.entries.len().saturating_sub(g.visible_rows());
                    sb.scroll = (sb.scroll as i32 - lines).clamp(0, max as i32) as usize;
                    self.window.request_redraw();
                } else {
                    self.on_scroll(delta);
                }
            }
            _ => {}
        }
        WinAction::None
    }

    /// Option counts as Meta only for the side(s) chosen in config.
    pub(super) fn meta_active(&self, cfg: &Config) -> bool {
        let l = self.modifiers.lalt_state() == ModifiersKeyState::Pressed;
        let r = self.modifiers.ralt_state() == ModifiersKeyState::Pressed;
        let any = self.modifiers.state().alt_key();
        match cfg.option_as_meta {
            OptionAsMeta::None => false,
            OptionAsMeta::Both => any,
            // Some keyboards don't report sides; treat unknown as left.
            OptionAsMeta::Left => l || (any && !r),
            OptionAsMeta::Right => r,
        }
    }

    /// Send a key to the focused pane with the Kitty protocol, if the app
    /// there asked for it. Returns true when handled.
    pub(super) fn kitty_key(&self, ctx: &Ctx, ev: &KeyEvent) -> bool {
        let mods = self.modifiers.state();
        if mods.super_key() || self.palette.is_some() || self.search.is_some() || self.sidebar.as_ref().is_some_and(|s| s.focused) {
            return false;
        }
        let Some(p) = self.focused_pane() else { return false };
        let mode = *p.session.term.lock().mode();
        if !mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL) {
            return false;
        }
        let named = match &ev.logical_key {
            Key::Named(n) => Some(*n),
            _ => None,
        };
        let base = match ev.key_without_modifiers() {
            Key::Character(c) => c.chars().next(),
            _ => None,
        };
        let event = match (ev.state, ev.repeat) {
            (ElementState::Released, _) => 3,
            (_, true) => 2,
            _ => 1,
        };
        let k = input::KittyKey { named, base, text: ev.text.as_deref(), event };
        let meta = self.meta_active(ctx.cfg);
        if let Some(bytes) = input::encode_kitty(&k, mods, mode, meta) {
            if event != 3 {
                let mut term = p.session.term.lock();
                term.selection = None;
                if term.grid().display_offset() != 0 {
                    term.scroll_display(Scroll::Bottom);
                }
            }
            p.session.write(bytes);
        }
        true
    }

    pub(super) fn on_key(&mut self, ctx: &mut Ctx, ev: KeyEvent) -> WinAction {
        if ev.state != ElementState::Pressed {
            self.kitty_key(ctx, &ev); // releases, only if an app asked
            return WinAction::None;
        }
        self.blink_epoch = Instant::now();
        if matches!(
            ev.logical_key,
            Key::Named(
                NamedKey::Shift | NamedKey::Control | NamedKey::Alt | NamedKey::Super | NamedKey::Meta | NamedKey::CapsLock | NamedKey::Fn
            )
        ) {
            self.kitty_key(ctx, &ev);
            return WinAction::None;
        }
        if let Some(perf) = self.perf.as_mut() {
            perf.key_pressed();
        }
        // Any key skips the welcome screen (and still does its normal job).
        if self.splash.take().is_some() {
            self.window.request_redraw();
        }
        // Any key closes the branch popover (Esc does nothing else).
        if self.git_pop.take().is_some() {
            self.window.request_redraw();
            if ev.logical_key == Key::Named(NamedKey::Escape) {
                return WinAction::None;
            }
        }
        // Any key closes the shortcut sheet.
        if self.sheet.is_some_and(|(_, closing)| !closing) {
            self.sheet = Some((Instant::now(), true));
            self.window.request_redraw();
            return WinAction::None;
        }
        if self.renaming.is_some() {
            return self.rename_key(&ev);
        }
        if self.agents_panel.is_some() && !(self.modifiers.state().super_key() && self.modifiers.state().shift_key()) {
            return self.agents_key(ctx, &ev);
        }
        if self.overview.as_ref().is_some_and(|o| o.closing.is_none()) {
            return self.overview_key(ctx, &ev);
        }
        if self.palette.is_some() && !self.palette_anim.1 {
            return self.palette_key(ctx, &ev);
        }
        if self.sidebar.as_ref().is_some_and(|s| s.focused)
            && let Some(result) = self.sidebar_key(ctx, &ev)
        {
            return result;
        }
        let mods = self.modifiers.state();
        if self.search.is_some()
            && let Some(result) = self.search_key(ctx, &ev)
        {
            return result;
        }
        if let Some(action) = ctx.keymap.lookup(&ev, mods) {
            return self.perform(ctx, action);
        }
        if mods.super_key() {
            // Line editing that sends bytes rather than running a command.
            match ev.key_without_modifiers() {
                Key::Named(NamedKey::ArrowLeft) => self.write_focused(vec![0x01]), // line start
                Key::Named(NamedKey::ArrowRight) => self.write_focused(vec![0x05]), // line end
                Key::Named(NamedKey::Backspace) => self.write_focused(vec![0x15]), // kill to start
                _ => {}
            }
            return WinAction::None;
        }

        let Some(p) = self.focused_pane() else { return WinAction::None };
        let mode = *p.session.term.lock().mode();

        // Shift+PageUp/PageDown/Home/End scroll the scrollback (outside full-screen apps).
        if mods.shift_key() && !mode.contains(TermMode::ALT_SCREEN) {
            let scroll = match ev.logical_key {
                Key::Named(NamedKey::PageUp) => Some(Scroll::PageUp),
                Key::Named(NamedKey::PageDown) => Some(Scroll::PageDown),
                Key::Named(NamedKey::Home) => Some(Scroll::Top),
                Key::Named(NamedKey::End) => Some(Scroll::Bottom),
                _ => None,
            };
            if let Some(s) = scroll {
                p.session.term.lock().scroll_display(s);
                self.window.request_redraw();
                return WinAction::None;
            }
        }

        if self.preedit {
            return WinAction::None; // the input method owns this key
        }
        if self.kitty_key(ctx, &ev) {
            return WinAction::None;
        }
        let meta = self.meta_active(ctx.cfg);
        if let Some(bytes) = input::encode_key(&ev, mods, mode, meta) {
            {
                let mut term = p.session.term.lock();
                term.selection = None;
                if term.grid().display_offset() != 0 {
                    term.scroll_display(Scroll::Bottom);
                }
            }
            p.session.write(bytes);
        }
        WinAction::None
    }

    /// Run a command (from a shortcut or the palette).
    pub(super) fn perform(&mut self, ctx: &mut Ctx, action: Action) -> WinAction {
        let report = |r: std::io::Result<()>| {
            if let Err(e) = r {
                log::error!("could not start shell: {e}");
            }
        };
        let n = self.tabs.len();
        match action {
            Action::NewTab => report(self.new_tab(ctx, None)),
            Action::NewWindow => return WinAction::NewWindow(self.focused_cwd()),
            Action::Split(dir) => report(self.split(ctx, dir)),
            Action::ClosePane => {
                if let Some(id) = self.focus_id()
                    && self.close_pane(ctx, id)
                {
                    return WinAction::CloseWindow;
                }
            }
            Action::ZoomPane => self.toggle_zoom(ctx),
            Action::NextTab => self.select_tab(ctx, (self.active + 1) % n),
            Action::PrevTab => self.select_tab(ctx, (self.active + n - 1) % n),
            Action::GoToTab(i) => self.select_tab(ctx, if i == usize::MAX { n - 1 } else { i }),
            Action::Focus(d) => self.focus_dir(ctx, d),
            Action::Resize(dir, delta) => self.resize_split(ctx, dir, delta),
            Action::Find => {
                if let Some(id) = self.focus_id() {
                    // Reopening keeps the last query, like most apps.
                    let mut st = self.search.take().unwrap_or_else(|| SearchState::new(id));
                    st.pane = id;
                    st.edited();
                    self.search = Some(st);
                }
            }
            Action::Copy => match self.copy_selection(ctx) {
                Some(n) => self.show_toast(&format!("Copied {n} character{}", if n == 1 { "" } else { "s" }), false),
                None => self.show_toast("Nothing selected to copy", false),
            },
            Action::Paste => self.paste_clipboard(ctx),
            Action::SelectAll => {
                if let Some(p) = self.focused_pane() {
                    let mut term = p.session.term.lock();
                    let (top, bottom, last) = (term.topmost_line(), term.bottommost_line(), term.last_column());
                    let mut sel = Selection::new(SelectionType::Simple, Point::new(top, Column(0)), Side::Left);
                    sel.update(Point::new(bottom, last), Side::Right);
                    term.selection = Some(sel);
                }
            }
            Action::Clear => {
                if let Some(id) = self.focus_id()
                    && let Some(p) = self.panes.get_mut(&id)
                {
                    p.session.term.lock().clear_screen(ClearMode::Saved);
                    p.prompts.clear();
                    p.session.write(vec![0x0c]); // let the shell redraw its prompt
                }
            }
            Action::PrevPrompt => self.jump_prompt(true),
            Action::NextPrompt => self.jump_prompt(false),
            Action::ScrollTop => {
                if let Some(p) = self.focused_pane() {
                    p.session.term.lock().scroll_display(Scroll::Top);
                }
            }
            Action::ScrollBottom => self.scroll_focused_to_bottom(),
            Action::FontBigger => self.change_font(ctx, self.font_pt + 1.0),
            Action::FontSmaller => self.change_font(ctx, self.font_pt - 1.0),
            Action::FontReset => self.change_font(ctx, self.base_font_pt),
            Action::Shortcuts => self.sheet = Some((Instant::now(), false)),
            Action::Palette => {
                self.palette = Some(PaletteState::new(ctx.keymap));
                self.palette_anim = (Instant::now(), false);
            }
            Action::Summon => self.splash = Some(Instant::now()),
            Action::OpenSettings => match Config::ensure_file() {
                Ok(path) => {
                    open_in_editor(&path);
                    self.show_toast("Settings opened · changes apply when you save", false);
                }
                Err(e) => self.show_toast(&format!("Couldn't create settings file: {e}"), true),
            },
            Action::ReloadSettings => return WinAction::ReloadConfig,
            Action::SetTheme(id) => return WinAction::SetConfig("theme", format!("\"{id}\"")),
            Action::SetAnimation(v) => return WinAction::SetConfig("animation_speed", format!("{v:.1}")),
            Action::MovePane(d) => {
                if let (Some(cur), Some(tab)) = (self.focus_id(), self.tabs.get_mut(self.active))
                    && let Some(n) = layout::neighbor(&self.visible, cur, d)
                {
                    tab.root.swap(cur, n);
                    self.relayout(ctx);
                }
            }
            Action::PaneToNewTab => {
                if let Some(id) = self.focus_id() {
                    self.drop_pane(ctx, id, DropTarget::NewTab);
                }
            }
            Action::SetOpacity(v) => return WinAction::SetConfig("opacity", format!("{v:.2}")),
            Action::SetBlur(on) => return WinAction::SetConfig("blur", on.to_string()),
            Action::SetTabColor(c) => {
                if let Some(t) = self.tabs.get_mut(self.active) {
                    t.color = c;
                }
            }
            Action::TabOverview => match &mut self.overview {
                Some(o) if o.closing.is_none() => o.closing = Some(Instant::now()),
                _ => {
                    self.agents_panel = None;
                    self.overview = Some(Overview { opened: Instant::now(), closing: None, selected: self.active });
                }
            },
            Action::Agents => {
                self.agents_panel = if self.agents_panel.is_some() { None } else { Some((Instant::now(), 0)) };
            }
            Action::RenameTab => {
                let current = self.tabs.get(self.active).and_then(|t| t.name.clone()).unwrap_or_default();
                self.renaming = Some((self.active, current));
            }
            Action::ToggleSidebar => {
                match &mut self.sidebar {
                    // Open but in the background: bring focus to it first.
                    Some(sb) if !sb.focused => sb.focused = true,
                    Some(_) => self.sidebar = None,
                    None => {
                        let root = self.pane_project().unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()));
                        self.sidebar = Some(Sidebar::new(root));
                        self.refresh_sidebar();
                    }
                }
                self.relayout(ctx);
            }
            Action::FindFile => {
                let root = self
                    .sidebar
                    .as_ref()
                    .filter(|s| s.pinned)
                    .map(|s| s.root.clone())
                    .or_else(|| self.pane_project())
                    .unwrap_or_else(|| PathBuf::from(std::env::var("HOME").unwrap_or_default()));
                let files = sidebar::walk_files(&root, 20_000);
                self.palette = Some(PaletteState::files(&root, files));
                self.palette_anim = (Instant::now(), false);
            }
            Action::OpenFile(path, line, col) => {
                let editor = open::resolve(&ctx.cfg.editor);
                if editor == open::Editor::Terminal {
                    return self.perform(ctx, Action::EditInTerminal(path, line));
                }
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let app = match &editor {
                    open::Editor::Cli { bin, .. } => bin.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    _ => "default app".into(),
                };
                open::open_file(&editor, &path, line, col);
                let at = line.map(|l| format!(":{l}")).unwrap_or_default();
                self.show_toast(&format!("Opening {name}{at} in {app}"), false);
            }
            Action::EditInTerminal(path, line) => {
                let dir = path.parent().map(std::path::Path::to_path_buf);
                if self.split_in(ctx, Dir::Horizontal, dir).is_ok() {
                    self.write_focused(open::terminal_edit_command(&path, line).into_bytes());
                    if let Some(sb) = self.sidebar.as_mut() {
                        sb.focused = false;
                    }
                }
            }
            Action::InsertPath(path) => {
                if let Some(sb) = self.sidebar.as_mut() {
                    sb.focused = false;
                }
                self.write_focused(format!("{} ", open::shell_quote(&path)).into_bytes());
            }
            Action::Quit => return WinAction::Quit,
        }
        self.window.request_redraw();
        WinAction::None
    }

    /// Keys while the search box is open. `None` = not handled here (lets
    /// shortcuts like ⌘T keep working).
    pub(super) fn search_key(&mut self, ctx: &mut Ctx, ev: &KeyEvent) -> Option<WinAction> {
        let mods = self.modifiers.state();
        if ev.logical_key == Key::Named(NamedKey::Escape) {
            self.search = None;
            self.window.request_redraw();
            return Some(WinAction::None);
        }
        let st = self.search.as_mut()?;
        let step = |st: &mut SearchState, panes: &HashMap<PaneId, Pane>, up: bool| {
            if let Some(p) = panes.get(&st.pane) {
                let mut term = p.session.term.lock();
                if st.dirty {
                    st.recompute(&mut term);
                }
                st.step(&mut term, up);
            }
        };
        let handled = match &ev.logical_key {
            Key::Named(NamedKey::Enter) => {
                step(st, &self.panes, !mods.shift_key());
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                step(st, &self.panes, true);
                true
            }
            Key::Named(NamedKey::ArrowDown) => {
                step(st, &self.panes, false);
                true
            }
            Key::Named(NamedKey::Backspace) => {
                if mods.super_key() || mods.alt_key() {
                    st.query.clear();
                } else {
                    st.query.pop();
                }
                st.edited();
                true
            }
            _ => false,
        };
        if handled {
            self.window.request_redraw();
            return Some(WinAction::None);
        }
        if mods.super_key() {
            let Key::Character(c) = ev.key_without_modifiers() else { return None };
            match c.as_str() {
                "g" => step(st, &self.panes, !mods.shift_key()),
                "f" => {}
                "r" => {
                    st.regex_mode = !st.regex_mode;
                    st.edited();
                }
                "v" => {
                    if let Some(text) = ctx.clipboard.as_mut().and_then(|cb| cb.get_text().ok()) {
                        st.query.push_str(text.lines().next().unwrap_or(""));
                        st.edited();
                    }
                }
                _ => return None,
            }
            self.window.request_redraw();
            return Some(WinAction::None);
        }
        if mods.control_key() {
            return None;
        }
        if let Some(text) = ev.text.as_ref().filter(|t| t.chars().all(|c| !c.is_control())) {
            st.query.push_str(text);
            st.edited();
        }
        self.window.request_redraw();
        Some(WinAction::None)
    }

    pub(super) fn close_palette(&mut self) {
        self.palette_anim = (Instant::now(), true);
        self.preview = None;
        self.window.request_redraw();
    }

    pub(super) fn palette_key(&mut self, ctx: &mut Ctx, ev: &KeyEvent) -> WinAction {
        let mods = self.modifiers.state();
        let Some(st) = self.palette.as_mut() else { return WinAction::None };
        match &ev.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.close_palette();
                return WinAction::None;
            }
            Key::Named(NamedKey::Enter) => {
                let action = st.selected_action().map(|a| match a {
                    Action::OpenFile(p, ..) if mods.alt_key() => Action::EditInTerminal(p, None),
                    Action::OpenFile(p, ..) if mods.shift_key() => Action::InsertPath(p),
                    a => a,
                });
                self.close_palette();
                return action.map_or(WinAction::None, |a| self.perform(ctx, a));
            }
            Key::Named(NamedKey::ArrowUp) => st.move_by(-1),
            Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::Tab) => st.move_by(if mods.shift_key() { -1 } else { 1 }),
            Key::Named(NamedKey::Backspace) => {
                if mods.super_key() || mods.alt_key() {
                    st.query.clear();
                } else {
                    st.query.pop();
                }
                st.refilter();
            }
            _ if mods.control_key() => match ev.key_without_modifiers() {
                Key::Character(c) if c == "p" || c == "k" => st.move_by(-1),
                Key::Character(c) if c == "n" || c == "j" => st.move_by(1),
                _ => {}
            },
            _ if mods.super_key() => match ev.key_without_modifiers() {
                Key::Character(c) if c == "p" => {
                    self.close_palette();
                    return WinAction::None;
                }
                Key::Character(c) if c == "v" => {
                    if let Some(text) = ctx.clipboard.as_mut().and_then(|cb| cb.get_text().ok()) {
                        st.query.push_str(text.lines().next().unwrap_or(""));
                        st.refilter();
                    }
                }
                _ => {}
            },
            _ => {
                if let Some(text) = ev.text.as_ref().filter(|t| t.chars().all(|c| !c.is_control())) {
                    st.query.push_str(text);
                    st.refilter();
                }
            }
        }
        // Browsing themes previews them live.
        self.preview = match st.selected_action() {
            Some(Action::SetTheme(id)) => Theme::builtin(id).map(|mut th| {
                ctx.cfg.colors.apply(&mut th);
                th
            }),
            _ => None,
        };
        self.window.request_redraw();
        WinAction::None
    }

    pub(super) fn on_mouse_move(&mut self, ctx: &mut Ctx) {
        let (x, y) = self.cursor_px;
        // In the tab overview, the card under the mouse is the chosen one.
        if let Some(o) = self.overview.as_mut().filter(|o| o.closing.is_none()) {
            if let Some(i) = self.overview_hits.iter().position(|r| r.contains(x, y))
                && i != o.selected
            {
                o.selected = i;
                self.window.request_redraw();
            }
            self.set_cursor_icon(CursorIcon::Pointer);
            return;
        }
        // Hover feedback for the clickable status bar and the branch popover.
        if self.drag.is_none() {
            let pop_hover = self.git_pop_hits.iter().find(|(r, _)| r.contains(x, y)).map(|(_, i)| *i);
            let status_hover = self.status_hit_at(x, y);
            if pop_hover != self.git_pop_hover || status_hover != self.status_hover {
                self.git_pop_hover = pop_hover;
                self.status_hover = status_hover;
                self.window.request_redraw();
            }
            if pop_hover.is_some() || status_hover.is_some() {
                self.set_cursor_icon(CursorIcon::Pointer);
                return;
            }
        }
        match &self.drag {
            Some(Drag::Divider(d)) => {
                let d = d.clone();
                let ratio = match d.dir {
                    Dir::Horizontal => (x - d.parent.x) / d.parent.w,
                    Dir::Vertical => (y - d.parent.y) / d.parent.h,
                }
                .clamp(0.1, 0.9);
                if let Some(tab) = self.tabs.get_mut(self.active)
                    && let Some(Node::Split { ratio: r, .. }) = tab.root.at_path_mut(&d.path)
                {
                    *r = ratio;
                }
                self.relayout(ctx);
                return;
            }
            Some(Drag::Pane { id, from, active }) => {
                let (id, from) = (*id, *from);
                let moved = (x - from.0).hypot(y - from.1) > 6.0 * self.scale;
                if *active || moved {
                    self.drag = Some(Drag::Pane { id, from, active: true });
                    self.set_cursor_icon(CursorIcon::Grabbing);
                    self.window.request_redraw();
                }
                return;
            }
            Some(Drag::Tab(from)) => {
                let from = *from;
                let over = self.tab_hits.iter().find(|h| x >= h.rect.x && x < h.rect.right()).map(|h| h.index);
                if let Some(to) = over.filter(|&to| to != from) {
                    self.tabs.swap(from, to);
                    self.active = to;
                    self.drag = Some(Drag::Tab(to));
                    self.window.request_redraw();
                }
                return;
            }
            Some(Drag::Select(id)) => {
                let id = *id;
                self.extend_selection(id);
                self.select_autoscroll();
                self.window.request_redraw();
                return;
            }
            None => {}
        }

        self.update_link_hover();

        // Hover: which pane shows its drag grip.
        let hover = self.pane_at(x, y);
        if hover != self.hover {
            self.hover = hover;
            self.window.request_redraw();
        }

        // Hover: cursor shape.
        let on_grip = hover.and_then(|h| self.grip_rect(h)).is_some_and(|g| g.contains(x, y));
        let icon = if on_grip {
            CursorIcon::Grab
        } else if let Some(d) = self.divider_at(x, y) {
            if d.dir == Dir::Horizontal { CursorIcon::ColResize } else { CursorIcon::RowResize }
        } else if self.pane_at(x, y).is_some() {
            CursorIcon::Text
        } else {
            CursorIcon::Default
        };
        self.set_cursor_icon(icon);

        // Motion reporting to mouse-aware apps in the focused pane.
        let Some(id) = self.focus_id() else { return };
        let Some(mode) = self.reporting(id) else { return };
        let Some((row, col, _)) = self.cell_in(id) else { return };
        let mods = self.modifiers.state();
        let Some(p) = self.panes.get_mut(&id) else { return };
        let wants = mode.contains(TermMode::MOUSE_MOTION) || (mode.contains(TermMode::MOUSE_DRAG) && p.mouse_held.is_some());
        if wants && p.last_report_cell != Some((row, col)) {
            p.last_report_cell = Some((row, col));
            let button = p.mouse_held.unwrap_or(3);
            if let Some(b) = input::encode_mouse(button, true, true, col, row, mods, mode) {
                p.session.write(b);
            }
        }
    }

    pub(super) fn on_mouse_button(&mut self, ctx: &mut Ctx, state: ElementState, button: MouseButton) -> WinAction {
        let (x, y) = self.cursor_px;
        let pressed = state == ElementState::Pressed;
        if pressed
            && button == MouseButton::Left
            && let Some(&(_, hit)) = self.card_hits.iter().find(|(r, _)| r.contains(x, y))
        {
            self.click_card(ctx, hit);
            return WinAction::None;
        }
        // The tab overview takes clicks: a card opens that tab, elsewhere closes.
        if pressed && self.overview.as_ref().is_some_and(|o| o.closing.is_none()) {
            let picked = self.overview_hits.iter().position(|r| r.contains(x, y));
            self.close_overview(ctx, picked);
            return WinAction::None;
        }
        // The branch popover takes the next click: a file opens, anything else closes it.
        if pressed && self.git_pop.is_some() {
            if self.status_hit_at(x, y) == Some(kraken_ui::statusbar::StatusHit::Branch) {
                self.git_pop = None; // clicking the branch again toggles it shut
                self.window.request_redraw();
            } else {
                self.git_pop_click(ctx, x, y);
            }
            return WinAction::None;
        }
        if pressed
            && button == MouseButton::Left
            && self.drag.is_none()
            && let Some(hit) = self.status_hit_at(x, y)
        {
            return self.status_click(ctx, hit);
        }
        if pressed {
            if self.splash.take().is_some() {
                self.window.request_redraw();
                return WinAction::None;
            }
            if self.sheet.is_some_and(|(_, closing)| !closing) {
                self.sheet = Some((Instant::now(), true));
                self.window.request_redraw();
                return WinAction::None;
            }
            if self.agents_panel.take().is_some() {
                self.window.request_redraw();
                return WinAction::None;
            }
            if self.palette.is_some() && !self.palette_anim.1 {
                self.close_palette();
                return WinAction::None;
            }
        }

        if !pressed {
            if let Some(Drag::Select(_)) = self.drag
                && ctx.cfg.copy_on_select
            {
                self.copy_selection(ctx);
            }
            if let Some(Drag::Pane { id, active: true, .. }) = self.drag {
                self.drag = None;
                if let Some(target) = self.drop_target(id) {
                    self.drop_pane(ctx, id, target);
                }
                self.set_cursor_icon(CursorIcon::Default);
                self.window.request_redraw();
                return WinAction::None;
            }
            let was_dragging = self.drag.take().is_some();
            if let Some(id) = self.focus_id()
                && let (Some(mode), Some(code)) = (self.reporting(id), input::mouse_button_code(button))
                && (!was_dragging || self.panes.get(&id).is_some_and(|p| p.mouse_held.is_some()))
            {
                let (row, col, _) = self.cell_in(id).unwrap_or((0, 0, Side::Left));
                let mods = self.modifiers.state();
                let Some(p) = self.panes.get_mut(&id) else { return WinAction::None };
                p.mouse_held = None;
                if let Some(b) = input::encode_mouse(code, false, false, col, row, mods, mode) {
                    p.session.write(b);
                }
            }
            return WinAction::None;
        }

        // Chrome: title bar and tab strip.
        let chrome = self.titlebar_h() + self.tabbar_h();
        if y < chrome {
            if button != MouseButton::Left {
                return WinAction::None;
            }
            if self.plus_hit.is_some_and(|r| r.contains(x, y)) {
                if let Err(e) = self.new_tab(ctx, None) {
                    log::error!("could not start shell: {e}");
                }
                return WinAction::None;
            }
            let hit = self.tab_hits.iter().find(|h| h.rect.contains(x, y)).map(|h| (h.index, h.close));
            match hit {
                Some((i, Some(close))) if close.contains(x, y) => {
                    if self.close_tab(ctx, i) {
                        return WinAction::CloseWindow;
                    }
                }
                Some((i, _)) => {
                    // Double-click: rename. (Tab clicks use a pseudo pane id.)
                    let now = Instant::now();
                    let tab_click = u64::MAX - 1 - i as u64;
                    let double = self.last_click.is_some_and(|(t, pid, ..)| pid == tab_click && now - t < Duration::from_millis(400));
                    self.last_click = Some((now, tab_click, 0, 0, 1));
                    self.select_tab(ctx, i);
                    if double {
                        self.perform(ctx, Action::RenameTab);
                    } else {
                        self.drag = Some(Drag::Tab(i));
                    }
                }
                None => {
                    let _ = self.window.drag_window();
                }
            }
            return WinAction::None;
        }

        if button == MouseButton::Left
            && let Some(d) = self.divider_at(x, y)
        {
            self.drag = Some(Drag::Divider(d.clone()));
            return WinAction::None;
        }

        if let Some(g) = self.sidebar_geom()
            && g.rect.contains(x, y)
        {
            if button != MouseButton::Left {
                return WinAction::None;
            }
            let now = Instant::now();
            let row = self.sidebar.as_ref().and_then(|sb| g.row_at(sb, y));
            let Some(sb) = self.sidebar.as_mut() else { return WinAction::None };
            sb.focused = true;
            if y < g.rect.y + g.header_h() {
                sb.go_up(); // click the header: go up a folder
            } else if let Some(i) = row {
                let double =
                    sb.selected == i && self.last_click.is_some_and(|(t, pid, ..)| pid == u64::MAX && now - t < Duration::from_millis(400));
                sb.selected = i;
                self.last_click = Some((now, u64::MAX, 0, 0, 1));
                if !sb.toggle()
                    && double
                    && let Some(e) = sb.selected_entry().cloned()
                {
                    self.window.request_redraw();
                    return self.perform(ctx, Action::OpenFile(e.path, None, None));
                }
            }
            self.window.request_redraw();
            return WinAction::None;
        }

        let Some(id) = self.pane_at(x, y) else { return WinAction::None };
        if let Some(sb) = self.sidebar.as_mut() {
            sb.focused = false;
        }
        let m = self.modifiers.state();
        if button == MouseButton::Left
            && m.super_key()
            && !m.alt_key()
            && let Some((_, _, _, _, link)) = self.link_hover.clone()
        {
            self.open_link(ctx, link);
            return WinAction::None;
        }
        if button == MouseButton::Left && (self.grip_rect(id).is_some_and(|g| g.contains(x, y)) || (m.super_key() && m.alt_key())) {
            self.set_focus(ctx, id);
            self.drag = Some(Drag::Pane { id, from: (x, y), active: false });
            return WinAction::None;
        }
        self.set_focus(ctx, id);
        let Some((row, col, side)) = self.cell_in(id) else { return WinAction::None };

        if let Some(mode) = self.reporting(id) {
            if let Some(code) = input::mouse_button_code(button) {
                let mods = self.modifiers.state();
                let Some(p) = self.panes.get_mut(&id) else { return WinAction::None };
                p.mouse_held = Some(code);
                if let Some(b) = input::encode_mouse(code, true, false, col, row, mods, mode) {
                    p.session.write(b);
                }
            }
            return WinAction::None;
        }
        if button != MouseButton::Left {
            return WinAction::None;
        }

        // Count clicks: double = word, triple = line.
        let now = Instant::now();
        let count = match self.last_click {
            Some((t, pid, r, c, n)) if now - t < Duration::from_millis(400) && pid == id && r == row && c == col => n % 3 + 1,
            _ => 1,
        };
        self.last_click = Some((now, id, row, col, count));

        let mods = self.modifiers.state();
        let p = &self.panes[&id];
        let mut term = p.session.term.lock();
        let point = view::viewport_to_point(term.grid().display_offset(), row, col);
        if mods.shift_key() && count == 1 && term.selection.is_some() {
            if let Some(sel) = term.selection.as_mut() {
                sel.update(point, side);
            }
        } else {
            let ty = match count {
                2 => SelectionType::Semantic,
                3 => SelectionType::Lines,
                _ if mods.alt_key() => SelectionType::Block,
                _ => SelectionType::Simple,
            };
            term.selection = Some(Selection::new(ty, point, side));
        }
        drop(term);
        self.drag = Some(Drag::Select(id));
        self.window.request_redraw();
        WinAction::None
    }

    pub(super) fn on_scroll(&mut self, delta: MouseScrollDelta) {
        let (x, y) = self.cursor_px;
        let Some(id) = self.pane_at(x, y).or(self.focus_id()) else { return };
        let cell_h = self.cell.1 as f64;
        let mode = self.reporting(id);
        let cell = self.cell_in(id);
        let mods = self.modifiers.state();
        let Some(p) = self.panes.get_mut(&id) else { return };

        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => y as f64 * 3.0,
            MouseScrollDelta::PixelDelta(d) => d.y / cell_h,
        };
        p.scroll_accum += lines;
        let n = p.scroll_accum.trunc() as i32;
        if n == 0 {
            return;
        }
        p.scroll_accum -= n as f64;

        if let Some(mode) = mode {
            let (row, col, _) = cell.unwrap_or((0, 0, Side::Left));
            let code = if n > 0 { 64 } else { 65 };
            for _ in 0..n.unsigned_abs() {
                if let Some(b) = input::encode_mouse(code, true, false, col, row, mods, mode) {
                    p.session.write(b);
                }
            }
            return;
        }

        let term_mode = *p.session.term.lock().mode();
        if term_mode.contains(TermMode::ALT_SCREEN) && term_mode.contains(TermMode::ALTERNATE_SCROLL) {
            // Full-screen apps without mouse support (less, man) get arrow keys.
            let app = term_mode.contains(TermMode::APP_CURSOR);
            let seq: &[u8] = match (n > 0, app) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            p.session.write(seq.repeat(n.unsigned_abs() as usize));
            return;
        }

        p.session.term.lock().scroll_display(Scroll::Delta(n));
        self.window.request_redraw();
    }
}
