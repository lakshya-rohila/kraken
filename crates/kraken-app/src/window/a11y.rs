//! VoiceOver support through AccessKit (which speaks NSAccessibility on macOS).
//!
//! The tree is small on purpose:
//!
//! ```text
//! Window "Kraken — <tab title>"
//! ├── Terminal  value = the focused pane's visible text
//! ├── Status    label = "zsh · ~/project · main · last command exit 1 after 3 s"
//! └── Announce  live region: toasts ("Copied", "Settings applied") are spoken
//! ```
//!
//! [`accesskit_winit::Adapter::update_if_active`] only runs our closure while
//! an assistive technology is connected, so nothing is gathered — and the
//! terminal is never locked for it — when VoiceOver is off.

use super::*;
use accesskit::{Live, Node, NodeId, Rect as AxRect, Role, TreeId, TreeInfo, TreeUpdate};

const ROOT: NodeId = NodeId(0);
const TERMINAL: NodeId = NodeId(1);
const STATUS: NodeId = NodeId(2);
const ANNOUNCE: NodeId = NodeId(3);

/// How often the terminal text is re-sent while output is changing.
const MIN_INTERVAL: Duration = Duration::from_millis(250);

pub struct A11y {
    adapter: accesskit_winit::Adapter,
    last_sent: Option<Instant>,
}

/// What the tree shows, gathered from the window.
pub(super) struct Snapshot {
    pub title: String,
    pub text: String,
    pub status: String,
    pub announce: String,
    /// Focused pane in window pixels.
    pub bounds: R,
}

impl A11y {
    /// Must be called before the window is first shown.
    pub fn new(event_loop: &ActiveEventLoop, window: &Window, proxy: EventLoopProxy<UserEvent>) -> A11y {
        A11y { adapter: accesskit_winit::Adapter::with_event_loop_proxy(event_loop, window, proxy), last_sent: None }
    }

    pub fn process_event(&mut self, window: &Window, event: &WindowEvent) {
        self.adapter.process_event(window, event);
    }

    /// Send the tree if VoiceOver (or another client) is listening. `force`
    /// skips the rate limit (first tree, focus changes).
    pub(super) fn update(&mut self, force: bool, snapshot: impl FnOnce() -> Snapshot) {
        if !force && self.last_sent.is_some_and(|t| t.elapsed() < MIN_INTERVAL) {
            return;
        }
        let mut sent = false;
        self.adapter.update_if_active(|| {
            sent = true;
            tree(snapshot())
        });
        if sent {
            self.last_sent = Some(Instant::now());
        }
    }
}

fn tree(s: Snapshot) -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_label(s.title);
    root.set_children(vec![TERMINAL, STATUS, ANNOUNCE]);

    let mut term = Node::new(Role::Terminal);
    term.set_label("Terminal");
    term.set_value(s.text);
    term.set_bounds(AxRect {
        x0: s.bounds.x as f64,
        y0: s.bounds.y as f64,
        x1: (s.bounds.x + s.bounds.w) as f64,
        y1: (s.bounds.y + s.bounds.h) as f64,
    });

    let mut status = Node::new(Role::Status);
    status.set_label(s.status);

    let mut announce = Node::new(Role::Label);
    announce.set_live(Live::Polite);
    announce.set_label(s.announce);

    TreeUpdate {
        nodes: vec![(ROOT, root), (TERMINAL, term), (STATUS, status), (ANNOUNCE, announce)],
        tree: Some(TreeInfo::new(ROOT)),
        tree_id: TreeId::ROOT,
        focus: TERMINAL,
    }
}

impl TermWindow {
    /// Everything the accessibility tree describes, read from current state.
    pub(super) fn a11y_snapshot(&self) -> Snapshot {
        let focus = self.focus_id();
        let rows = self.focused_pane().map_or(0, |p| p.session.size.rows);
        let text = focus.map(|id| self.screen_tail(id, rows.max(1)).join("\n")).unwrap_or_default();
        let bounds = focus.and_then(|id| self.visible.iter().find(|(v, _)| *v == id).map(|(_, r)| *r)).unwrap_or_default();
        let title = focus.map(|id| self.pane_title(id)).unwrap_or_default();
        let announce = self.toast.as_ref().map(|(t, _, _)| t.clone()).unwrap_or_default();
        Snapshot { title: format!("Kraken — {title}"), text, status: describe_status(&self.status_view()), announce, bounds }
    }

    /// AccessKit asked for something (first tree, an action).
    pub fn handle_a11y_event(&mut self, event: accesskit_winit::WindowEvent) {
        match event {
            accesskit_winit::WindowEvent::InitialTreeRequested => {
                let snapshot = self.a11y_snapshot();
                if let Some(a) = self.a11y.as_mut() {
                    a.update(true, || snapshot);
                }
            }
            accesskit_winit::WindowEvent::ActionRequested(req) => {
                if req.action == accesskit::Action::Focus {
                    self.window.focus_window();
                }
            }
            accesskit_winit::WindowEvent::AccessibilityDeactivated => {}
        }
    }

    /// Keep the tree current (cheap no-op when VoiceOver is off).
    pub(super) fn refresh_a11y(&mut self, force: bool) {
        let Some(mut a) = self.a11y.take() else { return };
        a.update(force, || self.a11y_snapshot());
        self.a11y = Some(a);
    }
}

/// The status bar as one spoken sentence.
fn describe_status(v: &StatusView) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(ssh) = &v.ssh {
        parts.push(format!("connected to {ssh}"));
    }
    if let Some(p) = &v.program {
        parts.push(format!("running {p}"));
    }
    if let Some(cwd) = &v.cwd {
        parts.push(format!("in {cwd}"));
    }
    if let Some(g) = &v.git {
        parts.push(format!("git branch {}", g.branch));
    }
    if let Some(d) = v.running_for {
        parts.push(format!("command running for {} seconds", d.as_secs()));
    } else if let Some((code, took)) = v.last {
        let result = match code {
            Some(0) => "succeeded".to_string(),
            Some(c) => format!("failed with exit code {c}"),
            None => "finished".to_string(),
        };
        parts.push(format!("last command {result} after {:.1} seconds", took.as_secs_f32()));
    }
    if let Some((working, waiting)) = v.agents
        && working + waiting > 0
    {
        parts.push(format!("{working} agents working, {waiting} waiting for you"));
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_reads_as_a_sentence() {
        let v = StatusView {
            program: Some("cargo".into()),
            ssh: None,
            cwd: Some("~/Kraken".into()),
            git: None,
            running_for: None,
            last: Some((Some(1), Duration::from_millis(3200))),
            clock: "12:00".into(),
            agents: Some((1, 2)),
            ..Default::default()
        };
        assert_eq!(
            describe_status(&v),
            "running cargo, in ~/Kraken, last command failed with exit code 1 after 3.2 seconds, 1 agents working, 2 waiting for you"
        );
    }

    #[test]
    fn tree_has_terminal_focus_and_live_announcements() {
        let t = tree(Snapshot {
            title: "Kraken — zsh".into(),
            text: "$ ls".into(),
            status: String::new(),
            announce: "Copied".into(),
            bounds: R::default(),
        });
        assert_eq!(t.focus, TERMINAL);
        assert_eq!(t.nodes.len(), 4);
        let term = &t.nodes.iter().find(|(id, _)| *id == TERMINAL).unwrap().1;
        assert_eq!(term.role(), Role::Terminal);
        assert_eq!(term.value(), Some("$ ls"));
        let ann = &t.nodes.iter().find(|(id, _)| *id == ANNOUNCE).unwrap().1;
        assert_eq!(ann.live(), Some(Live::Polite));
    }
}
