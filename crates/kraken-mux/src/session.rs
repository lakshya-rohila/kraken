//! Session restore: remember windows, tabs, split layouts and each pane's
//! folder, so Kraken reopens the way you left it.
//!
//! Saved to `~/Library/Application Support/Kraken/session.json` every few
//! seconds (so a crash loses little) and when quitting. Closing the last
//! window on purpose forgets the session.

use crate::layout::{Dir, Node, PaneId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionFile {
    pub version: u32,
    pub windows: Vec<WindowSession>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowSession {
    /// Inner size in points.
    pub width: f64,
    pub height: f64,
    /// Outer position in physical pixels.
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub active: usize,
    pub tabs: Vec<TabSession>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabSession {
    /// Name you gave the tab (older session files don't have one).
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<usize>,
    pub zoom: bool,
    /// Index of the focused pane in the layout's left-to-right pane order.
    pub focus: usize,
    pub layout: NodeSession,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum NodeSession {
    Pane { cwd: Option<PathBuf> },
    Split { side_by_side: bool, ratio: f32, a: Box<NodeSession>, b: Box<NodeSession> },
}

impl NodeSession {
    /// Capture a layout tree; `cwd` looks up each pane's folder.
    pub fn from_node(node: &Node, cwd: &impl Fn(PaneId) -> Option<PathBuf>) -> Self {
        match node {
            Node::Leaf(id) => NodeSession::Pane { cwd: cwd(*id) },
            Node::Split { dir, ratio, a, b } => NodeSession::Split {
                side_by_side: *dir == Dir::Horizontal,
                ratio: *ratio,
                a: Box::new(Self::from_node(a, cwd)),
                b: Box::new(Self::from_node(b, cwd)),
            },
        }
    }

    /// Rebuild a layout tree with fresh pane ids. Returns the tree and, in
    /// pane order, each new id with the folder to start its shell in.
    pub fn to_node(&self, new_id: &mut impl FnMut() -> PaneId) -> (Node, Vec<(PaneId, Option<PathBuf>)>) {
        match self {
            NodeSession::Pane { cwd } => {
                let id = new_id();
                // Folders may have been deleted since; fall back to home.
                let cwd = cwd.clone().filter(|p| p.is_dir());
                (Node::Leaf(id), vec![(id, cwd)])
            }
            NodeSession::Split { side_by_side, ratio, a, b } => {
                let (na, mut pa) = a.to_node(new_id);
                let (nb, pb) = b.to_node(new_id);
                pa.extend(pb);
                let dir = if *side_by_side { Dir::Horizontal } else { Dir::Vertical };
                (Node::Split { dir, ratio: ratio.clamp(0.1, 0.9), a: Box::new(na), b: Box::new(nb) }, pa)
            }
        }
    }
}

pub fn path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support/Kraken/session.json"))
}

pub fn load() -> Option<SessionFile> {
    let text = std::fs::read_to_string(path()?).ok()?;
    let file: SessionFile = serde_json::from_str(&text).ok()?;
    (file.version == VERSION && !file.windows.is_empty()).then_some(file)
}

pub fn to_json(windows: Vec<WindowSession>) -> String {
    serde_json::to_string_pretty(&SessionFile { version: VERSION, windows }).unwrap_or_default()
}

/// Write atomically (temp file + rename), so a crash mid-write can't corrupt it.
pub fn save(json: &str) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

pub fn forget() {
    if let Some(p) = path() {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_layout() {
        let mut root = Node::Leaf(1);
        root.split(1, 2, Dir::Horizontal);
        root.split(2, 3, Dir::Vertical);
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let saved = NodeSession::from_node(&root, &|id| if id == 3 { home.clone() } else { None });

        let json = to_json(vec![WindowSession {
            width: 1100.0,
            height: 700.0,
            x: Some(10),
            y: None,
            active: 0,
            tabs: vec![TabSession { name: Some("api server".into()), color: Some(1), zoom: false, focus: 2, layout: saved.clone() }],
        }]);
        let back: SessionFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.windows[0].tabs[0].layout, saved);
        assert_eq!(back.windows[0].tabs[0].name.as_deref(), Some("api server"));
        // Session files from before tab names existed still load.
        let old: TabSession = serde_json::from_str(r#"{"zoom":false,"focus":0,"layout":{"type":"pane","cwd":null}}"#).unwrap();
        assert_eq!(old.name, None);

        let mut next = 100;
        let (node, panes) = saved.to_node(&mut || {
            next += 1;
            next
        });
        assert_eq!(node.panes(), vec![101, 102, 103]);
        assert_eq!(panes[2].1, home);
        assert!(panes[0].1.is_none());
    }

    #[test]
    fn missing_folders_fall_back_to_home() {
        let s = NodeSession::Pane { cwd: Some("/definitely/not/here".into()) };
        let (_, panes) = s.to_node(&mut || 1);
        assert_eq!(panes[0].1, None);
    }
}
