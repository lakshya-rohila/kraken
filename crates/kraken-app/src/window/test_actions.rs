//! `KRAKEN_ACTIONS` scripting: action ids plus test-only input helpers.

use super::*;

impl TermWindow {
    /// One step of a `KRAKEN_ACTIONS` script. Any action id (`split-right`,
    /// `focus:left`, `set-theme:ember`…) runs through [`Self::perform`], the
    /// same path as keys, menus and the palette; the verbs below are test-only
    /// helpers that simulate input.
    pub(super) fn run_test_action(&mut self, ctx: &mut Ctx, action: &str) {
        if let Some(a) = Action::parse(action) {
            self.perform(ctx, a);
            return;
        }
        let _: std::io::Result<()> = match action {
            a if a.starts_with("find:") => {
                self.perform(ctx, Action::Find);
                if let Some(st) = self.search.as_mut() {
                    st.query = a[5..].to_string();
                    st.edited();
                }
                Ok(())
            }
            a if a.starts_with("palette:") => {
                self.perform(ctx, Action::Palette);
                if let Some(st) = self.palette.as_mut() {
                    st.query = a[8..].to_string();
                    st.refilter();
                }
                Ok(())
            }
            "palette-down" => {
                if let Some(st) = self.palette.as_mut() {
                    st.move_by(1);
                    self.preview = match st.selected_action() {
                        Some(Action::SetTheme(id)) => Theme::builtin(id),
                        _ => None,
                    };
                }
                Ok(())
            }
            "prompt-up" => {
                self.jump_prompt(true);
                Ok(())
            }
            a if a.starts_with("toast:") => {
                self.show_toast(&a[6..], false);
                Ok(())
            }
            a if a.starts_with("drop:") => {
                // drop:swap | drop:left | drop:newtab — moves the focused pane.
                if let (Some(src), Some(other)) =
                    (self.focus_id(), self.visible.iter().map(|(id, _)| *id).find(|id| Some(*id) != self.focus_id()))
                {
                    let target = match &a[5..] {
                        "swap" => DropTarget::Swap(other),
                        "left" => DropTarget::Edge(other, Direction::Left),
                        "up" => DropTarget::Edge(other, Direction::Up),
                        _ => DropTarget::NewTab,
                    };
                    self.drop_pane(ctx, src, target);
                }
                Ok(())
            }
            a if a.starts_with("dragsim:") => {
                // dragsim:left|swap — pretend to drag the focused pane over another one.
                let other = self.visible.iter().find(|(id, _)| Some(*id) != self.focus_id()).map(|(_, r)| *r);
                if let (Some(id), Some(r)) = (self.focus_id(), other) {
                    let fx = if &a[8..] == "left" { 0.1 } else { 0.5 };
                    self.cursor_px = (r.x + r.w * fx, r.y + r.h * 0.5);
                    self.drag = Some(Drag::Pane { id, from: (0.0, 0.0), active: true });
                }
                Ok(())
            }
            "sidebar" => {
                self.perform(ctx, Action::ToggleSidebar);
                if let Some(sb) = self.sidebar.as_mut() {
                    // Open the first folder so the tree shows some depth.
                    if let Some(i) = sb.entries.iter().position(|e| e.is_dir && e.name == "crates") {
                        sb.selected = i;
                        sb.toggle();
                    }
                }
                Ok(())
            }
            a if a.starts_with("findfile:") => {
                self.perform(ctx, Action::FindFile);
                if let Some(st) = self.palette.as_mut() {
                    st.query = a[9..].to_string();
                    st.refilter();
                }
                Ok(())
            }
            a if a.starts_with("cmdhover:") => {
                // cmdhover:row,col — pretend ⌘ is held with the mouse over that cell.
                let mut it = a[9..].split(',').filter_map(|n| n.parse::<f32>().ok());
                if let (Some(row), Some(col), Some(p)) = (it.next(), it.next(), self.focused_pane()) {
                    let l = p.layout;
                    self.cursor_px = (l.x + (col + 0.5) * l.cell_w, l.y + (row + 0.5) * l.cell_h);
                    self.modifiers = Modifiers::from(winit::keyboard::ModifiersState::SUPER);
                    self.update_link_hover();
                }
                Ok(())
            }
            a if a.starts_with("rename:") => {
                self.perform(ctx, Action::RenameTab);
                if let Some((_, text)) = self.renaming.as_mut() {
                    *text = a[7..].to_string();
                }
                Ok(())
            }
            "rename-save" => {
                if let Some((tab, text)) = self.renaming.take() {
                    self.tabs[tab].name = Some(text);
                }
                Ok(())
            }
            a if a.starts_with("color:") => {
                self.perform(ctx, Action::SetTabColor(a[6..].parse().ok()));
                Ok(())
            }
            a if a.starts_with("cardclick:") => {
                // cardclick:card,choice — press an answer button on a card.
                let mut it = a[10..].split(',').filter_map(|n| n.parse::<usize>().ok());
                if let (Some(i), Some(c)) = (it.next(), it.next()) {
                    self.click_card(ctx, CardHit::Choice(i, c));
                }
                Ok(())
            }
            "agents" => {
                self.tick_agents(ctx);
                self.perform(ctx, Action::Agents);
                Ok(())
            }
            "sheet" => {
                self.sheet = Some((Instant::now(), false));
                Ok(())
            }
            // Synthetic clipboard contents through the real paste path (the
            // system clipboard is never touched by tests).
            "paste-test-image" => {
                let (w, h) = (320usize, 200usize);
                let rgba = (0..w * h).flat_map(|i| [(i % w * 255 / w) as u8, (i / w * 255 / h) as u8, 180, 255]).collect();
                if let Some(id) = self.focus_id() {
                    self.paste_clip(id, crate::clipboard::Clip::Image { width: w, height: h, rgba });
                }
                Ok(())
            }
            "paste-test-files" => {
                if let Some(id) = self.focus_id() {
                    let files = vec![PathBuf::from("/tmp/My Screenshot.png"), PathBuf::from("/tmp/notes.txt")];
                    self.paste_clip(id, crate::clipboard::Clip::Files(files));
                }
                Ok(())
            }
            "minimize" => {
                self.window.set_minimized(true);
                Ok(())
            }
            "git-popover" => {
                self.status_click(ctx, kraken_ui::statusbar::StatusHit::Branch);
                Ok(())
            }
            "tick" => {
                self.tick(ctx);
                Ok(())
            }
            a if a.starts_with("wait:") => {
                // The shell runs on its own thread, so blocking here just lets it catch up.
                std::thread::sleep(Duration::from_millis(a[5..].parse().unwrap_or(500)));
                Ok(())
            }
            a if a.starts_with("input:") => {
                self.write_focused(a[6..].replace("\\n", "\r").into_bytes());
                Ok(())
            }
            a => {
                log::warn!("unknown test action {a:?}");
                Ok(())
            }
        };
    }

    // ── Geometry ─────────────────────────────────────────────────────────
}

impl TermWindow {
    /// `KRAKEN_LATER="1500=git-popover;3000=tab-overview"` runs actions that
    /// many milliseconds after the first frame (once the shell has reported
    /// its folder, git info has loaded, …), for screenshots and tests.
    pub(super) fn later_hook(&mut self, ctx: &mut Ctx) {
        let Ok(spec) = std::env::var("KRAKEN_LATER") else { return };
        static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        static DONE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let start = *START.get_or_init(Instant::now);
        let steps: Vec<(u64, &str)> =
            spec.split(';').filter_map(|s| s.split_once('=')).filter_map(|(ms, a)| Some((ms.trim().parse().ok()?, a.trim()))).collect();
        let done = DONE.load(std::sync::atomic::Ordering::Relaxed);
        if let Some(&(due, action)) = steps.get(done) {
            if start.elapsed() >= Duration::from_millis(due) {
                DONE.store(done + 1, std::sync::atomic::Ordering::Relaxed);
                self.run_test_action(ctx, action);
            }
            self.window.request_redraw();
        }
    }
}
