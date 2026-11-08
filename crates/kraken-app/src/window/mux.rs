//! Windows → tabs → split tree: creating, closing, focusing, zooming,
//! resizing and moving panes, and saving/restoring the layout.

use super::*;

impl TermWindow {
    /// Start a shell for pane `id`, which must already be in the active tab's tree.
    pub(super) fn spawn_pane(&mut self, ctx: &mut Ctx, id: PaneId, cwd: Option<PathBuf>) -> std::io::Result<()> {
        self.relayout(ctx);
        let rect = self.visible.iter().find(|(p, _)| *p == id).map(|(_, r)| *r).unwrap_or_default();
        let grid = self.grid_for(rect, ctx.cfg);
        let proxy = self.proxy.clone();
        let wid = self.window.id();
        let events = EventProxy::new(move |e| {
            let _ = proxy.send_event(UserEvent::Term(wid, id, e));
        });
        let proxy = self.proxy.clone();
        let shell_events: kraken_term::shell::ShellListener = Arc::new(move |e, at| {
            let _ = proxy.send_event(UserEvent::Shell(wid, id, e, at));
        });
        let session = Session::spawn(ctx.cfg, grid, events, shell_events, id, cwd)?;
        let inset = self.inset(ctx.cfg);
        self.panes.insert(
            id,
            Pane {
                session,
                title: String::new(),
                activity: false,
                bell: false,
                scroll_accum: 0.0,
                mouse_held: None,
                last_report_cell: None,
                st: PaneStatus::default(),
                prompts: Vec::new(),
                agent: None,
                last_output: Instant::now(),
                born: Instant::now(),
                grow: None,
                layout: Layout {
                    x: rect.x + inset,
                    y: rect.y + inset,
                    cell_w: self.cell.0,
                    cell_h: self.cell.1,
                    font_px: self.font_pt * self.scale,
                    focused: self.focused,
                    cursor_visible: true,
                },
            },
        );
        self.relayout(ctx);
        Ok(())
    }

    pub(super) fn new_tab(&mut self, ctx: &mut Ctx, cwd: Option<PathBuf>) -> std::io::Result<()> {
        let cwd = cwd.or_else(|| self.focused_cwd());
        let id = ctx.new_id();
        self.tabs.push(Tab { root: Node::Leaf(id), focus: id, zoom: false, born: Instant::now(), name: None, color: None });
        self.active = self.tabs.len() - 1;
        if let Err(e) = self.spawn_pane(ctx, id, cwd) {
            self.tabs.pop();
            self.active = self.tabs.len().saturating_sub(1);
            return Err(e);
        }
        Ok(())
    }

    pub(super) fn split(&mut self, ctx: &mut Ctx, dir: Dir) -> std::io::Result<()> {
        self.split_in(ctx, dir, None)
    }

    pub(super) fn split_in(&mut self, ctx: &mut Ctx, dir: Dir, cwd: Option<PathBuf>) -> std::io::Result<()> {
        let cwd = cwd.or_else(|| self.focused_cwd());
        let id = ctx.new_id();
        let Some(tab) = self.tabs.get_mut(self.active) else { return Ok(()) };
        tab.zoom = false;
        let old = tab.focus;
        tab.root.split(old, id, dir);
        tab.focus = id;
        let spawned = self.spawn_pane(ctx, id, cwd);
        if let Some(p) = self.panes.get_mut(&id) {
            p.grow = Some(dir);
        }
        if let Err(e) = spawned {
            let tab = &mut self.tabs[self.active];
            tab.root.remove(id);
            tab.focus = old;
            self.relayout(ctx);
            return Err(e);
        }
        Ok(())
    }

    /// Close one pane. Returns true when the window has nothing left.
    pub(super) fn close_pane(&mut self, ctx: &mut Ctx, id: PaneId) -> bool {
        self.panes.remove(&id);
        let Some(ti) = self.tabs.iter().position(|t| t.root.panes().contains(&id)) else {
            return self.tabs.is_empty();
        };
        let tab = &mut self.tabs[ti];
        if matches!(tab.root, Node::Leaf(_)) {
            self.tabs.remove(ti);
            if self.tabs.is_empty() {
                return true;
            }
            if ti < self.active || self.active >= self.tabs.len() {
                self.active = self.active.saturating_sub(1);
            }
        } else {
            // Prefer the pane next to the closed one, so focus doesn't jump far.
            let next = [Direction::Left, Direction::Up, Direction::Right, Direction::Down]
                .into_iter()
                .find_map(|d| layout::neighbor(&self.visible, id, d));
            tab.root.remove(id);
            tab.zoom = false;
            if tab.focus == id {
                tab.focus = next.filter(|n| tab.root.panes().contains(n)).unwrap_or_else(|| tab.root.panes()[0]);
            }
        }
        self.relayout(ctx);
        false
    }

    pub(super) fn close_tab(&mut self, ctx: &mut Ctx, index: usize) -> bool {
        let Some(tab) = self.tabs.get(index) else { return false };
        let ids = tab.root.panes();
        let mut empty = false;
        for id in ids {
            empty = self.close_pane(ctx, id);
        }
        empty
    }

    pub(super) fn select_tab(&mut self, ctx: &mut Ctx, index: usize) {
        self.window.request_redraw();
        if index >= self.tabs.len() {
            return;
        }
        self.active = index;
        for id in self.tabs[index].root.panes() {
            if let Some(p) = self.panes.get_mut(&id) {
                p.activity = false;
                p.bell = false;
            }
        }
        self.relayout(ctx);
    }

    pub(super) fn set_focus(&mut self, ctx: &mut Ctx, id: PaneId) {
        self.refresh_a11y(true);
        if let Some(tab) = self.tabs.get_mut(self.active)
            && tab.focus != id
        {
            tab.focus = id;
            self.relayout(ctx);
        }
    }

    pub(super) fn focus_dir(&mut self, ctx: &mut Ctx, d: Direction) {
        if let Some(cur) = self.focus_id()
            && let Some(n) = layout::neighbor(&self.visible, cur, d)
        {
            self.set_focus(ctx, n);
        }
    }

    pub(super) fn toggle_zoom(&mut self, ctx: &mut Ctx) {
        if let Some(tab) = self.tabs.get_mut(self.active)
            && (!matches!(tab.root, Node::Leaf(_)) || tab.zoom)
        {
            tab.zoom = !tab.zoom;
            self.relayout(ctx);
        }
    }

    pub(super) fn resize_split(&mut self, ctx: &mut Ctx, dir: Dir, delta: f32) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            let f = tab.focus;
            tab.root.resize(f, dir, delta);
            self.relayout(ctx);
        }
    }

    pub(super) fn change_font(&mut self, ctx: &mut Ctx, pt: f32) {
        self.font_pt = pt.clamp(MIN_FONT_PT, MAX_FONT_PT);
        self.apply_size(ctx);
    }

    /// The little handle at the top of a pane you can grab to move it.
    /// Only shown when there's somewhere to move it to.
    pub(super) fn grip_rect(&self, id: PaneId) -> Option<R> {
        if self.visible.len() < 2 && self.tabs.len() < 2 {
            return None;
        }
        let r = self.visible.iter().find(|(p, _)| *p == id)?.1;
        let (gw, gh) = (44.0 * self.scale, 12.0 * self.scale);
        Some(R { x: r.x + (r.w - gw) / 2.0, y: r.y + 2.0 * self.scale, w: gw, h: gh })
    }

    /// What's under the mouse while dragging pane `src`.
    pub(super) fn drop_target(&self, src: PaneId) -> Option<DropTarget> {
        let (x, y) = self.cursor_px;
        if y < self.titlebar_h() + self.tabbar_h() {
            if self.plus_hit.is_some_and(|r| r.contains(x, y)) {
                return Some(DropTarget::NewTab);
            }
            return self.tab_hits.iter().find(|h| h.rect.contains(x, y) && h.index != self.active).map(|h| DropTarget::Tab(h.index));
        }
        let (target, r) = *self.visible.iter().find(|(_, r)| r.contains(x, y))?;
        if target == src {
            return None;
        }
        // Near an edge → split there; in the middle → swap.
        let (fx, fy) = ((x - r.x) / r.w, (y - r.y) / r.h);
        let edges = [(fx, Direction::Left), (1.0 - fx, Direction::Right), (fy, Direction::Up), (1.0 - fy, Direction::Down)];
        let (dist, dir) = edges.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
        Some(if dist < 0.3 { DropTarget::Edge(target, dir) } else { DropTarget::Swap(target) })
    }

    pub(super) fn drop_pane(&mut self, ctx: &mut Ctx, src: PaneId, target: DropTarget) {
        let Some(from_tab) = self.tabs.iter().position(|t| t.root.panes().contains(&src)) else { return };
        let alone = matches!(self.tabs[from_tab].root, Node::Leaf(_));
        // Take the pane out of its tab (the tab disappears if it was the only pane).
        let take = |tabs: &mut Vec<Tab>, active: &mut usize| {
            if alone {
                tabs.remove(from_tab);
                if from_tab < *active {
                    *active -= 1;
                }
            } else {
                let t = &mut tabs[from_tab];
                t.root.remove(src);
                t.zoom = false;
                if t.focus == src {
                    t.focus = t.root.panes()[0];
                }
            }
        };
        match target {
            DropTarget::Swap(t) => {
                self.tabs[self.active].root.swap(src, t);
            }
            DropTarget::Edge(t, dir) => {
                take(&mut self.tabs, &mut self.active);
                let (axis, first) = match dir {
                    Direction::Left => (Dir::Horizontal, true),
                    Direction::Right => (Dir::Horizontal, false),
                    Direction::Up => (Dir::Vertical, true),
                    Direction::Down => (Dir::Vertical, false),
                };
                let tab = &mut self.tabs[self.active];
                tab.root.insert_beside(t, src, axis, first);
                tab.focus = src;
                tab.zoom = false;
                if let Some(p) = self.panes.get_mut(&src) {
                    p.born = Instant::now();
                    p.grow = Some(axis);
                }
            }
            DropTarget::Tab(mut i) => {
                if alone && from_tab < i {
                    i -= 1;
                }
                take(&mut self.tabs, &mut self.active);
                let tab = &mut self.tabs[i];
                let f = tab.focus;
                tab.root.insert_beside(f, src, Dir::Horizontal, false);
                tab.focus = src;
                tab.zoom = false;
                self.select_tab(ctx, i);
            }
            DropTarget::NewTab => {
                if alone {
                    return; // already alone in its own tab
                }
                take(&mut self.tabs, &mut self.active);
                self.tabs.push(Tab { root: Node::Leaf(src), focus: src, zoom: false, born: Instant::now(), name: None, color: None });
                let last = self.tabs.len() - 1;
                self.select_tab(ctx, last);
            }
        }
        self.relayout(ctx);
    }

    pub fn snapshot(&self) -> WindowSession {
        let size = self.window.inner_size().to_logical::<f64>(self.scale as f64);
        let pos = self.window.outer_position().ok();
        let tabs = self
            .tabs
            .iter()
            .map(|t| TabSession {
                name: t.name.clone(),
                color: t.color,
                zoom: t.zoom,
                focus: t.root.panes().iter().position(|&id| id == t.focus).unwrap_or(0),
                layout: NodeSession::from_node(&t.root, &|id| {
                    self.panes.get(&id).and_then(|p| p.st.cwd.clone().or_else(|| p.session.cwd()))
                }),
            })
            .collect();
        WindowSession { width: size.width, height: size.height, x: pos.map(|p| p.x), y: pos.map(|p| p.y), active: self.active, tabs }
    }

    pub(super) fn restore_tabs(&mut self, ctx: &mut Ctx, saved: &WindowSession) {
        for ts in &saved.tabs {
            let (root, panes) = ts.layout.to_node(&mut || ctx.new_id());
            let focus = panes.get(ts.focus).or(panes.first()).map(|p| p.0).unwrap_or(0);
            self.tabs.push(Tab { root, focus, zoom: false, born: Instant::now(), name: ts.name.clone(), color: ts.color });
            self.active = self.tabs.len() - 1;
            for (id, cwd) in panes {
                if let Err(e) = self.spawn_pane(ctx, id, cwd) {
                    log::error!("could not restore a pane: {e}");
                    let tab = self.tabs.last_mut().unwrap();
                    if !tab.root.remove(id) {
                        self.tabs.pop();
                        break;
                    }
                }
            }
            if let Some(tab) = self.tabs.last_mut() {
                tab.zoom = ts.zoom && !matches!(tab.root, Node::Leaf(_));
            }
        }
        self.active = saved.active.min(self.tabs.len().saturating_sub(1));
        self.relayout(ctx);
    }

    pub fn has_presented(&self) -> bool {
        self.presented
    }

    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }
}
