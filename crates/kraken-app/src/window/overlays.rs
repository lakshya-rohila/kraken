//! Agents dashboard, "needs you" cards and the file sidebar glue,
//! plus the once-a-second tick that drives them.

use super::*;

impl TermWindow {
    /// Refresh foreground program, SSH target and cwd for visible panes.
    /// Called every second, and right after a command finishes.
    pub fn tick(&mut self, ctx: &mut Ctx) {
        // Test hook: KRAKEN_LATE_ACTIONS="9000:cardclick:0,0|tab-2" runs after 9 s.
        if let Ok(spec) = std::env::var("KRAKEN_LATE_ACTIONS") {
            static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            let start = *START.get_or_init(Instant::now);
            if let Some((ms, actions)) = spec.split_once(':')
                && start.elapsed() >= Duration::from_millis(ms.parse().unwrap_or(0))
                && !DONE.swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                for a in actions.split('|') {
                    self.run_test_action(ctx, a);
                }
            }
        }
        self.tick_agents(ctx);
        let visible: Vec<PaneId> = self.visible.iter().map(|(id, _)| *id).collect();
        for id in visible {
            let Some(p) = self.panes.get_mut(&id) else { continue };
            let fg = p.session.foreground();
            let name = fg.as_ref().map(|f| f.name.clone()).unwrap_or_default();
            let was_ssh = p.st.ssh.is_some();
            p.st.ssh = match &fg {
                Some(f) if status::is_remote_program(&f.name) => {
                    status::process_args(f.pid).and_then(|a| status::remote_target(&a)).or_else(|| Some(f.name.clone()))
                }
                _ => None,
            };
            p.st.program = (!name.is_empty() && !SHELLS.contains(&name.as_str())).then_some(name);
            // CPU and memory of what's running (a runaway build shows up here).
            match (&p.st.program, &fg) {
                (Some(_), Some(f)) if p.agent.is_none() => {
                    if let Some((cpu_ns, rss)) = kraken_term::process_tree_usage(f.pid) {
                        let now = Instant::now();
                        if let Some((at, prev)) = p.st.cpu_sample {
                            let wall = now.duration_since(at).as_nanos().max(1) as f64;
                            let pct = (cpu_ns.saturating_sub(prev) as f64 / wall * 100.0) as f32;
                            p.st.usage = Some((pct, rss));
                        }
                        p.st.cpu_sample = Some((now, cpu_ns));
                    }
                }
                _ => {
                    p.st.usage = None;
                    p.st.cpu_sample = None;
                }
            }
            let cwd = p.session.cwd();
            // Walking up the folders for `.git` only when the folder changed.
            if cwd != p.st.cwd {
                p.st.git_root = cwd.as_deref().and_then(status::git_root);
            }
            p.st.cwd = cwd;
            if was_ssh != p.st.ssh.is_some() {
                self.relayout(ctx);
            }
        }
        // Git status runs a process: only for a window you're looking at.
        if self.focused
            && !self.occluded
            && let Some(root) = self.focused_pane().and_then(|p| p.st.git_root.clone())
        {
            self.request_git(root, false);
        }
        if !self.occluded {
            self.refresh_sidebar();
        }
        // Redraw only if something the tick affects actually changed.
        let shown = self.tick_signature();
        if shown != self.tick_shown && !self.occluded {
            self.tick_shown = shown;
            self.window.request_redraw();
        }
    }

    /// Everything the tick can change on screen: status bar, tab labels and
    /// agent states. Compared each second instead of redrawing blindly.
    fn tick_signature(&self) -> String {
        let labels: Vec<String> = (0..self.tabs.len()).map(|i| self.tab_label(i)).collect();
        let agents: Vec<(PaneId, bool, bool)> =
            self.panes.iter().filter_map(|(id, p)| p.agent.as_ref().map(|a| (*id, a.state == AgentState::Working, a.attention))).collect();
        format!("{:?}|{labels:?}|{agents:?}|{}", self.status_view(), self.cards.len())
    }

    /// Find agents in every pane and notice when one finishes and waits for you.
    pub(super) fn tick_agents(&mut self, ctx: &mut Ctx) {
        let now = Instant::now();
        let _ = ctx;
        let mut waiting_now: Vec<(PaneId, &'static str)> = Vec::new();
        for (&id, p) in self.panes.iter_mut() {
            let fg = p.session.foreground();
            // SSH state for every pane, so background tabs can turn orange too.
            p.st.ssh = match &fg {
                Some(f) if status::is_remote_program(&f.name) => {
                    status::process_args(f.pid).and_then(|a| status::remote_target(&a)).or_else(|| Some(f.name.clone()))
                }
                _ => None,
            };
            let found = fg.as_ref().and_then(|f| agents::detect(&f.name, || status::process_args(f.pid)));
            match (found, p.agent.as_mut()) {
                (None, _) => p.agent = None,
                (Some(name), None) => {
                    let state = if now.saturating_duration_since(p.last_output) < agents::QUIET {
                        AgentState::Working
                    } else {
                        AgentState::Waiting
                    };
                    p.agent = Some(Agent { name, state, since: now, attention: false });
                }
                (Some(_), Some(a)) => {
                    if agents::update(a, p.last_output, now) {
                        waiting_now.push((id, a.name));
                    }
                }
            }
        }
        for (id, _) in waiting_now {
            self.raise_card(id);
        }
        self.settle_cards();
        self.window.request_redraw();
    }

    /// An agent needs you: show (or refresh) its card, unless you're already
    /// looking at it. Away from Kraken entirely? macOS notifies you too.
    pub(super) fn raise_card(&mut self, id: PaneId) {
        let visible = self.visible.iter().any(|(p, _)| *p == id);
        if visible && self.focused {
            return;
        }
        let Some(agent) = self.panes.get(&id).and_then(|p| p.agent.clone()) else { return };
        let ask = cards::read_ask(&self.screen_tail(id, 16));
        let tab = self.tabs.iter().position(|t| t.root.panes().contains(&id)).unwrap_or(0);
        let where_ = format!("tab {} · {}", tab + 1, truncate(&self.tab_label(tab), 18));
        if let Some(p) = self.panes.get_mut(&id) {
            p.bell = !visible;
            if let Some(a) = p.agent.as_mut() {
                a.attention = true;
            }
        }
        let fresh = match self.cards.iter_mut().find(|c| c.pane == id && c.leaving.is_none()) {
            Some(c) => {
                let changed = c.question != ask.question;
                c.question = ask.question.clone();
                c.choices = ask.choices.clone();
                changed
            }
            None => {
                self.cards.push(Card {
                    pane: id,
                    agent: agent.name.to_string(),
                    where_: where_.clone(),
                    question: ask.question.clone(),
                    choices: ask.choices.clone(),
                    born: Instant::now(),
                    leaving: None,
                });
                true
            }
        };
        if fresh && !self.focused {
            let body = ask.question.first().cloned().unwrap_or_else(|| "Waiting for your input".into());
            notify(&format!("{} needs you", agent.name), &format!("{body}\n{where_}"));
            self.window.request_user_attention(Some(UserAttentionType::Informational));
        }
        self.window.request_redraw();
    }

    /// Slide away cards that no longer apply: the agent is working again,
    /// it has gone, or you're now looking at its pane.
    pub(super) fn settle_cards(&mut self) {
        let now = Instant::now();
        for c in &mut self.cards {
            if c.leaving.is_some() {
                continue;
            }
            let agent = self.panes.get(&c.pane).and_then(|p| p.agent.as_ref());
            let looking = self.focused && self.visible.iter().any(|(p, _)| *p == c.pane);
            if agent.is_none_or(|a| a.state == AgentState::Working) || looking {
                c.leaving = Some(now);
            }
        }
    }

    pub(super) fn dismiss_card(&mut self, i: usize) {
        if let Some(c) = self.cards.get_mut(i) {
            c.leaving.get_or_insert(Instant::now());
        }
        self.window.request_redraw();
    }

    /// A click on a card: answer, open, or close it.
    pub(super) fn click_card(&mut self, ctx: &mut Ctx, hit: CardHit) {
        match hit {
            CardHit::Choice(i, ci) => {
                let Some(card) = self.cards.get(i).cloned() else { return };
                let Some(choice) = card.choices.get(ci) else { return };
                if let Some(p) = self.panes.get_mut(&card.pane) {
                    p.session.write(choice.keys.clone().into_bytes());
                    p.bell = false; // handled — no need for the tab's orange dot
                    if let Some(a) = p.agent.as_mut() {
                        a.attention = false;
                    }
                }
                self.dismiss_card(i);
                self.show_toast(&format!("Answered {} with “{}”", card.agent, choice.label), false);
            }
            CardHit::Open(i) => {
                let Some(card) = self.cards.get(i).cloned() else { return };
                if let Some(tab) = self.tabs.iter().position(|t| t.root.panes().contains(&card.pane)) {
                    self.select_tab(ctx, tab);
                    self.set_focus(ctx, card.pane);
                }
                self.dismiss_card(i);
            }
            CardHit::Close(i) => self.dismiss_card(i),
        }
    }

    /// Every agent in this window, waiting ones first.
    pub(super) fn agent_rows(&self) -> Vec<agents::Row> {
        let mut rows: Vec<agents::Row> = Vec::new();
        for (ti, tab) in self.tabs.iter().enumerate() {
            for id in tab.root.panes() {
                let Some(p) = self.panes.get(&id) else { continue };
                let Some(a) = &p.agent else { continue };
                let folder =
                    p.st.cwd.as_ref().and_then(|c| c.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "~".into());
                rows.push(agents::Row {
                    pane: id,
                    tab: ti,
                    tab_label: format!("{} {}", ti + 1, self.tab_label(ti)),
                    folder,
                    agent: a.clone(),
                });
            }
        }
        rows.sort_by_key(|r| (r.agent.state == AgentState::Working, !r.agent.attention, r.tab));
        rows
    }

    /// Folder the sidebar should show for the focused pane: its git repo, else its cwd.
    pub(super) fn pane_project(&self) -> Option<PathBuf> {
        let p = self.focused_pane()?;
        p.st.git_root.clone().or_else(|| p.st.cwd.clone()).or_else(|| p.session.cwd())
    }

    pub(super) fn refresh_sidebar(&mut self) {
        let project = self.pane_project();
        let Some(sb) = self.sidebar.as_mut() else { return };
        if !sb.pinned
            && let Some(p) = project
        {
            sb.set_root(p);
        }
        sb.git_root = kraken_term::status::git_root(&sb.root);
        if let Some(root) = sb.git_root.clone() {
            sb.changes = self.git.get(&root).and_then(|e| e.info.as_ref()).map(|g| g.changes.clone()).unwrap_or_default();
            self.request_git(root, false);
        }
    }

    pub(super) fn sidebar_geom(&self) -> Option<SidebarGeom> {
        self.sidebar.as_ref()?;
        let s = self.scale;
        let top = self.titlebar_h() + self.tabbar_h();
        let h = self.renderer.config.height as f32 - top - self.status_h();
        let w = (sidebar::WIDTH_PT * s).min(self.renderer.config.width as f32 * 0.45).round();
        let px = 12.0 * s;
        Some(SidebarGeom { rect: R { x: 0.0, y: top, w, h }, scale: s, char_w: self.cell.0 * px / (self.font_pt * s), font_px: px })
    }
}

// ── Clickable status bar ─────────────────────────────────────────────────

impl TermWindow {
    /// The status-bar segment under the mouse, if any.
    pub(super) fn status_hit_at(&self, x: f32, y: f32) -> Option<statusbar::StatusHit> {
        let top = self.renderer.config.height as f32 - self.status_h();
        if y < top {
            return None;
        }
        self.status_hits.iter().find(|(_, x0, x1)| x >= *x0 && x <= *x1).map(|(h, ..)| *h)
    }

    /// A click on a status-bar segment.
    pub(super) fn status_click(&mut self, ctx: &mut Ctx, hit: statusbar::StatusHit) -> WinAction {
        use statusbar::StatusHit;
        match hit {
            StatusHit::Folder => {
                let dir = self.focused_pane().and_then(|p| p.st.cwd.clone().or_else(|| p.session.cwd()));
                if let Some(dir) = dir {
                    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir.display().to_string());
                    crate::tasks::spawn("open-folder", move || {
                        let _ = std::process::Command::new("open").arg(&dir).status();
                    });
                    self.show_toast(&format!("Opened {name} in Finder"), false);
                }
            }
            StatusHit::Branch => {
                if self.git_pop.take().is_none() {
                    let root = self.focused_pane().and_then(|p| p.st.git_root.clone());
                    let anchor = self.status_hits.iter().find(|(h, ..)| *h == StatusHit::Branch).map_or(0.0, |(_, x0, _)| *x0);
                    if let Some(root) = root {
                        self.git_pop = Some(GitPop { root: root.clone(), anchor_x: anchor, changes: None });
                        self.git_pop_hover = None;
                        let (proxy, wid) = (self.proxy.clone(), self.window.id());
                        crate::tasks::spawn("git-changes", move || {
                            let changes = status::git_changes(&root);
                            let _ = proxy.send_event(UserEvent::GitChanges(wid, root, changes));
                        });
                        // Refresh the counts in the header too.
                        if let Some(r) = self.git_pop.as_ref().map(|p| p.root.clone()) {
                            self.request_git(r, true);
                        }
                    }
                }
            }
            StatusHit::Agents => return self.perform(ctx, Action::Agents),
        }
        self.window.request_redraw();
        WinAction::None
    }

    /// `git status` answered for the popover.
    pub fn handle_git_changes(&mut self, root: PathBuf, changes: Option<Vec<kraken_term::status::GitChange>>) {
        if let Some(pop) = self.git_pop.as_mut().filter(|p| p.root == root) {
            pop.changes = Some(changes.unwrap_or_default());
            self.window.request_redraw();
        }
    }

    /// A click while the popover is open: open a file, or close it.
    pub(super) fn git_pop_click(&mut self, ctx: &mut Ctx, x: f32, y: f32) {
        let picked = self.git_pop_hits.iter().find(|(r, _)| r.contains(x, y)).map(|(_, i)| *i);
        let Some(pop) = self.git_pop.take() else { return };
        if let Some(path) = picked.and_then(|i| pop.changes.as_ref()?.get(i)).map(|c| pop.root.join(&c.path)) {
            self.perform(ctx, Action::OpenFile(path, None, None));
        }
        self.window.request_redraw();
    }
}

// ── Tab overview ─────────────────────────────────────────────────────────

impl TermWindow {
    /// Every tab as a card: its split layout, each pane as a mini-map.
    pub(super) fn overview_cards(&self, t: &Theme) -> Vec<kraken_ui::overview::TabCard> {
        use kraken_ui::overview::{PaneThumb, TabCard};
        // Lay out on a large virtual canvas (the split tree rounds to whole
        // pixels, which would collapse a 1×1 one), then scale to 0..1.
        const V: f32 = 10_000.0;
        let unit = R { x: 0.0, y: 0.0, w: V, h: V };
        self.tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let mut at = Vec::new();
                if tab.zoom {
                    at.push((tab.focus, unit));
                } else {
                    tab.root.layout(unit, 0.0, &mut at, &mut Vec::new());
                }
                for (_, r) in &mut at {
                    *r = R { x: r.x / V, y: r.y / V, w: r.w / V, h: r.h / V };
                }
                let mut working = 0;
                let mut waiting = 0;
                let panes = at
                    .into_iter()
                    .filter_map(|(id, r)| {
                        let p = self.panes.get(&id)?;
                        if let Some(a) = &p.agent {
                            if a.state == AgentState::Working {
                                working += 1;
                            } else {
                                waiting += 1;
                            }
                        }
                        // Bounded: a thumbnail never needs more than this many bars.
                        let (cols, rows, runs) = view::minimap(&p.session.term.lock(), t, 4000);
                        Some(PaneThumb { at: r, cols, rows, runs, focused: id == tab.focus })
                    })
                    .collect();
                TabCard { title: self.tab_label(i), color: self.tab_color(i, t), working, waiting, active: i == self.active, panes }
            })
            .collect()
    }

    /// Close the overview, switching to `pick` if a card was chosen.
    pub(super) fn close_overview(&mut self, ctx: &mut Ctx, pick: Option<usize>) {
        if let Some(i) = pick.filter(|&i| i < self.tabs.len()) {
            self.select_tab(ctx, i);
        }
        if let Some(o) = self.overview.as_mut() {
            o.closing = Some(Instant::now());
        }
        self.window.request_redraw();
    }

    /// Keys while the overview is open.
    pub(super) fn overview_key(&mut self, ctx: &mut Ctx, ev: &KeyEvent) -> WinAction {
        let n = self.tabs.len();
        let Some(selected) = self.overview.as_ref().map(|o| o.selected) else { return WinAction::None };
        if ctx.keymap.lookup(ev, self.modifiers.state()) == Some(Action::TabOverview) {
            self.close_overview(ctx, None);
            return WinAction::None;
        }
        let cols = self.overview_cols();
        let step = |dx, dy| kraken_ui::overview::step(selected, n, cols, dx, dy);
        let next = match &ev.logical_key {
            Key::Named(NamedKey::Escape) => {
                self.close_overview(ctx, None);
                return WinAction::None;
            }
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                self.close_overview(ctx, Some(selected));
                return WinAction::None;
            }
            Key::Named(NamedKey::ArrowLeft) => step(-1, 0),
            Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::Tab) => step(1, 0),
            Key::Named(NamedKey::ArrowUp) => step(0, -1),
            Key::Named(NamedKey::ArrowDown) => step(0, 1),
            Key::Character(c) if c.len() == 1 && ('1'..='9').contains(&c.chars().next().unwrap_or('x')) => {
                let i = c.parse::<usize>().unwrap_or(1) - 1;
                if i < n {
                    self.close_overview(ctx, Some(i));
                }
                return WinAction::None;
            }
            _ => selected,
        };
        if let Some(o) = self.overview.as_mut() {
            o.selected = next;
        }
        self.window.request_redraw();
        WinAction::None
    }

    /// Columns in the overview grid (to move up/down by rows).
    fn overview_cols(&self) -> usize {
        let (w, h) = (self.renderer.config.width as f32, self.renderer.config.height as f32);
        let top = self.titlebar_h() + self.tabbar_h();
        let area_h = (h - top - self.status_h()).max(1.0);
        let g = kraken_ui::overview::OverviewGeom {
            area: R { x: 0.0, y: top, w, h: area_h },
            scale: self.scale,
            ui_px: 13.0 * self.scale,
            aspect: w / area_h,
        };
        kraken_ui::overview::layout(self.tabs.len(), &g).1
    }
}
