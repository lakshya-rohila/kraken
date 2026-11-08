//! One pane: its title, shell-integration and terminal events, git
//! status, prompts, and selection/clipboard.

use super::*;

impl TermWindow {
    pub(super) fn pane_title(&self, id: PaneId) -> String {
        let Some(p) = self.panes.get(&id) else { return String::new() };
        if let Some(host) = &p.st.ssh {
            return format!("⇄ {host}");
        }
        if !p.title.is_empty() {
            // Shells usually set "user@host:path"; the path is the useful part.
            return match p.title.split_once(':') {
                Some((who, path)) if who.contains('@') && !path.is_empty() => path.trim().to_string(),
                _ => p.title.clone(),
            };
        }
        p.session
            .cwd()
            .map(|c| {
                let home = std::env::var("HOME").unwrap_or_default();
                let s = c.to_string_lossy().to_string();
                if !home.is_empty() && s.starts_with(&home) { format!("~{}", &s[home.len()..]) } else { s }
            })
            .unwrap_or_else(|| "shell".into())
    }

    // ── Drawing ──────────────────────────────────────────────────────────

    /// The last lines of a pane's screen, as plain text (for reading questions).
    pub(super) fn screen_tail(&self, id: PaneId, n: usize) -> Vec<String> {
        let Some(p) = self.panes.get(&id) else { return Vec::new() };
        let term = p.session.term.lock();
        let rows = term.screen_lines();
        let cols = term.columns();
        let text_of = |r: usize| {
            let row = &term.grid()[kraken_term::alacritty_terminal::index::Line(r as i32)];
            let text: String = (0..cols).map(|c| row[Column(c)].c).map(|c| if c == '\0' { ' ' } else { c }).collect();
            text.trim_end().to_string()
        };
        // End at the last line with text on it (a fresh pane's content sits at the top).
        let last = (0..rows).rev().find(|&r| !text_of(r).trim().is_empty()).unwrap_or(0);
        (last.saturating_sub(n - 1)..=last).map(text_of).collect()
    }

    /// Run `git status` for `root` on a background thread (rate-limited).
    pub(super) fn request_git(&mut self, root: PathBuf, force: bool) {
        let entry =
            self.git.entry(root.clone()).or_insert(GitEntry { info: None, fetched: Instant::now() - GIT_STALE * 2, pending: false });
        if entry.pending || (!force && entry.fetched.elapsed() < GIT_STALE) {
            return;
        }
        entry.pending = true;
        let proxy = self.proxy.clone();
        let wid = self.window.id();
        crate::tasks::spawn("git-status", move || {
            let info = status::git_status(&root);
            let _ = proxy.send_event(UserEvent::Git(wid, root, info));
        });
    }

    pub fn handle_git(&mut self, root: PathBuf, info: Option<GitInfo>) {
        let e = self.git.entry(root).or_insert(GitEntry { info: None, fetched: Instant::now(), pending: false });
        let changed = e.info != info;
        e.info = info;
        e.fetched = Instant::now();
        e.pending = false;
        if changed {
            self.window.request_redraw();
        }
    }

    pub fn handle_shell_event(&mut self, ctx: &mut Ctx, id: PaneId, ev: ShellEvent, at: Instant) {
        let visible = self.visible.iter().any(|(p, _)| *p == id);
        let window_focused = self.focused;
        let title = self.pane_title(id);
        let Some(p) = self.panes.get_mut(&id) else { return };
        match ev {
            ShellEvent::CommandStart => p.st.running_since = Some(at),
            ShellEvent::CommandEnd { exit } => {
                if let Some(start) = p.st.running_since.take() {
                    let took = at.saturating_duration_since(start);
                    p.st.last = Some((exit, took));
                    p.st.last_at = Some(at);
                    p.st.usage = None;
                    p.st.cpu_sample = None;
                    let limit = ctx.cfg.notify_after_secs;
                    if limit > 0 && took.as_secs() >= limit && (!visible || !window_focused) {
                        let body = match exit {
                            Some(0) | None => format!("✓ {title} finished in {}", statusbar::format_duration(took)),
                            Some(c) => format!("✗ {title} failed (exit {c}) after {}", statusbar::format_duration(took)),
                        };
                        notify("Kraken", &body);
                        p.bell = !visible;
                        self.window.request_user_attention(Some(UserAttentionType::Informational));
                    }
                }
                // Commands often change files or branches: refresh right away.
                self.tick(ctx);
                if let Some(root) = self.panes.get(&id).and_then(|p| p.st.git_root.clone()) {
                    self.request_git(root, true);
                }
            }
            ShellEvent::Cwd(dir) => {
                p.st.git_root = status::git_root(&dir);
                p.st.cwd = Some(dir);
            }
            ShellEvent::Reply(bytes) => p.session.write(bytes),
            ShellEvent::Prompt => {
                let term = p.session.term.lock();
                let abs = term.grid().history_size() + term.grid().cursor.point.line.0.max(0) as usize;
                drop(term);
                if p.prompts.last() != Some(&abs) {
                    p.prompts.push(abs);
                }
            }
            ShellEvent::Notify { title: t, body } => {
                // Agents use this to say "done" or "I need you". Only interrupt
                // when you're not already looking at that pane.
                if p.agent.is_some() {
                    self.raise_card(id);
                    self.window.request_redraw();
                    return;
                }
                if !visible || !window_focused {
                    notify(t.as_deref().unwrap_or("Kraken"), &body);
                    p.bell = !visible;
                    self.window.request_user_attention(Some(UserAttentionType::Informational));
                }
            }
        }
        self.window.request_redraw();
    }

    // ── Events ───────────────────────────────────────────────────────────

    pub fn handle_term_event(&mut self, ctx: &mut Ctx, id: PaneId, ev: TermEvent) -> WinAction {
        let visible = self.visible.iter().any(|(p, _)| *p == id);
        let Some(p) = self.panes.get_mut(&id) else { return WinAction::None };
        match ev {
            TermEvent::Wakeup => {
                p.last_output = Instant::now();
                if let Some(st) = self.search.as_mut().filter(|st| st.pane == id) {
                    st.dirty = true;
                }
                if visible {
                    self.window.request_redraw();
                } else if !p.activity {
                    p.activity = true;
                    self.window.request_redraw();
                }
            }
            TermEvent::Title(t) => {
                p.title = t;
                self.window.request_redraw();
            }
            TermEvent::ResetTitle => {
                p.title.clear();
                self.window.request_redraw();
            }
            TermEvent::ClipboardStore(_, text) => {
                if let Some(cb) = ctx.clipboard.as_mut() {
                    let _ = cb.set_text(text);
                }
            }
            TermEvent::ClipboardLoad(_, format) => {
                let text = ctx.clipboard.as_mut().and_then(|cb| cb.get_text().ok()).unwrap_or_default();
                p.session.write(format(&text).into_bytes());
            }
            TermEvent::ColorRequest(index, format) => {
                let t = &ctx.cfg.theme;
                let c = {
                    let term = p.session.term.lock();
                    term.colors()[index].map(|c| Rgb(c.r, c.g, c.b))
                }
                .unwrap_or(match index {
                    0..=255 => t.indexed(index as u8),
                    256 => t.foreground,
                    257 => t.background,
                    _ => t.cursor,
                });
                let out = format(kraken_term::alacritty_terminal::vte::ansi::Rgb { r: c.0, g: c.1, b: c.2 });
                p.session.write(out.into_bytes());
            }
            TermEvent::PtyWrite(text) => {
                // Identify as a modern (VT220-class, colour) terminal rather than
                // a VT102: some tools (e.g. kitty's icat) wait forever otherwise.
                let text = if text == "\x1b[?6c" { "\x1b[?62;22c".to_string() } else { text };
                p.session.write(text.into_bytes());
            }
            TermEvent::TextAreaSizeRequest(format) => {
                let out = format(p.session.size.into());
                p.session.write(out.into_bytes());
            }
            TermEvent::Bell => {
                if p.agent.is_some() {
                    self.raise_card(id);
                    return WinAction::None;
                }
                if !visible || !self.focused {
                    p.bell = !visible;
                    self.window.request_user_attention(Some(UserAttentionType::Informational));
                    self.window.request_redraw();
                }
            }
            TermEvent::Exit | TermEvent::ChildExit(_) => {
                if self.close_pane(ctx, id) {
                    return WinAction::CloseWindow;
                }
            }
            TermEvent::MouseCursorDirty | TermEvent::CursorBlinkingChange => {}
        }
        WinAction::None
    }

    pub(super) fn scroll_focused_to_bottom(&self) {
        if let Some(p) = self.focused_pane() {
            let mut term = p.session.term.lock();
            if term.grid().display_offset() != 0 {
                term.scroll_display(Scroll::Bottom);
            }
        }
    }

    /// Copy the selection; returns how many characters, or `None` if
    /// nothing was selected.
    pub(super) fn copy_selection(&self, ctx: &mut Ctx) -> Option<usize> {
        let p = self.focused_pane()?;
        let text = p.session.term.lock().selection_to_string()?;
        if text.is_empty() {
            return None;
        }
        let n = text.chars().count();
        ctx.clipboard.as_mut()?.set_text(text).ok()?;
        Some(n)
    }

    /// ⌘V: files paste as paths, text as text, an image is saved and its
    /// path pasted (agents attach it). Ctrl+V is left to the app.
    pub(super) fn paste_clipboard(&mut self, ctx: &mut Ctx) {
        use crate::clipboard::{self, Clip};
        let Some(id) = self.focus_id() else { return };
        let clip = ctx.clipboard.as_mut().map_or(Clip::Empty, clipboard::read);
        self.paste_clip(id, clip);
    }

    /// Paste what the clipboard held into pane `id`.
    pub(super) fn paste_clip(&mut self, id: PaneId, clip: crate::clipboard::Clip) {
        use crate::clipboard::{self, Clip};
        match clip {
            Clip::Text(text) => self.paste_into(id, &text),
            Clip::Files(files) => {
                self.paste_into(id, &clipboard::paths_text(&files));
                let n = files.len();
                self.show_toast(&format!("Pasted {n} file path{}", if n == 1 { "" } else { "s" }), false);
            }
            Clip::Image { width, height, rgba } => {
                // Encoding happens off the main thread, so typing never stalls.
                let (proxy, wid) = (self.proxy.clone(), self.window.id());
                crate::tasks::spawn("paste-image", move || {
                    let result = clipboard::save_png(&clipboard::pastes_dir(), width, height, rgba)
                        .map(|path| (clipboard::paths_text(std::slice::from_ref(&path)), format!("Pasted image · {width}×{height}")));
                    let _ = proxy.send_event(UserEvent::PasteReady(wid, id, result));
                });
            }
            Clip::Empty => self.show_toast("The clipboard is empty", false),
        }
    }

    /// Type `text` into pane `id` as a paste (bracketed when the app asked).
    pub(super) fn paste_into(&mut self, id: PaneId, text: &str) {
        let Some(p) = self.panes.get(&id) else { return };
        let bytes = {
            let mut term = p.session.term.lock();
            term.selection = None;
            if term.grid().display_offset() != 0 {
                term.scroll_display(Scroll::Bottom);
            }
            input::paste_bytes(text, *term.mode())
        };
        p.session.write(bytes);
        self.window.request_redraw();
    }

    /// A pasted image was saved (or couldn't be).
    pub fn paste_ready(&mut self, id: PaneId, result: Result<(String, String), String>) {
        match result {
            Ok((text, toast)) => {
                self.paste_into(id, &text);
                self.show_toast(&toast, false);
            }
            Err(e) => self.show_toast(&format!("Couldn't paste the image: {e}"), true),
        }
    }

    /// Scroll so the previous/next shell prompt sits at the top of the pane.
    pub(super) fn jump_prompt(&mut self, up: bool) {
        let Some(p) = self.focused_pane() else { return };
        let mut term = p.session.term.lock();
        let hist = term.grid().history_size();
        let off = term.grid().display_offset();
        if p.prompts.is_empty() {
            // No shell integration: fall back to top/bottom.
            term.scroll_display(if up { Scroll::Top } else { Scroll::Bottom });
            return;
        }
        let top = hist - off;
        let target = if up { p.prompts.iter().rev().find(|&&l| l < top).copied() } else { p.prompts.iter().find(|&&l| l > top).copied() };
        let new_off = match target {
            Some(l) => hist.saturating_sub(l).min(hist),
            None if up => off,
            None => 0,
        };
        term.scroll_display(Scroll::Delta(new_off as i32 - off as i32));
    }

    /// (row, col, side) of the cell under the mouse inside pane `id`.
    pub(super) fn cell_in(&self, id: PaneId) -> Option<(usize, usize, Side)> {
        let p = self.panes.get(&id)?;
        let s = p.session.size;
        let (row, col, right) = p.layout.cell_at(self.cursor_px.0 as f64, self.cursor_px.1 as f64, s.cols, s.rows);
        Some((row, col, if right { Side::Right } else { Side::Left }))
    }

    pub(super) fn reporting(&self, id: PaneId) -> Option<TermMode> {
        let p = self.panes.get(&id)?;
        let mode = *p.session.term.lock().mode();
        (mode.intersects(TermMode::MOUSE_MODE) && !self.modifiers.state().shift_key()).then_some(mode)
    }
}
