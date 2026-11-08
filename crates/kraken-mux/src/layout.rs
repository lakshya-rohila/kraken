//! Split-pane layout: each tab is a binary tree whose leaves are panes.
//!
//! ```text
//!        Split(H, 0.5)            ┌──────┬──────┐
//!        /          \             │      │  2   │
//!     Leaf(1)   Split(V, 0.6)     │  1   ├──────┤
//!                /       \        │      │  3   │
//!            Leaf(2)   Leaf(3)    └──────┴──────┘
//! ```
//!
//! Splitting replaces a leaf with a `Split` holding the old leaf and the new
//! one. Closing a pane replaces its parent split with the sibling. Dragging a
//! divider just changes one `ratio`.

pub type PaneId = u64;

/// `Horizontal` = panes side by side (vertical divider);
/// `Vertical` = panes stacked (horizontal divider).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct R {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl R {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// A draggable divider between two children of a split.
#[derive(Debug, Clone)]
pub struct Divider {
    /// Path from the root to the split node (false = first child, true = second).
    pub path: Vec<bool>,
    pub dir: Dir,
    /// The line itself, in pixels.
    pub rect: R,
    /// The area the split node occupies (used to turn mouse position into a ratio).
    pub parent: R,
}

#[derive(Debug, Clone)]
pub enum Node {
    Leaf(PaneId),
    Split { dir: Dir, ratio: f32, a: Box<Node>, b: Box<Node> },
}

impl Node {
    /// Compute every pane's rectangle and every divider inside `r`.
    pub fn layout(&self, r: R, gap: f32, panes: &mut Vec<(PaneId, R)>, dividers: &mut Vec<Divider>) {
        self.layout_inner(r, gap, &mut Vec::new(), panes, dividers);
    }

    fn layout_inner(&self, r: R, gap: f32, path: &mut Vec<bool>, panes: &mut Vec<(PaneId, R)>, dividers: &mut Vec<Divider>) {
        match self {
            Node::Leaf(id) => panes.push((*id, r)),
            Node::Split { dir, ratio, a, b } => {
                let (ra, rb, line) = match dir {
                    Dir::Horizontal => {
                        let wa = ((r.w - gap) * ratio).round();
                        (R { w: wa, ..r }, R { x: r.x + wa + gap, w: r.w - wa - gap, ..r }, R { x: r.x + wa, w: gap, ..r })
                    }
                    Dir::Vertical => {
                        let ha = ((r.h - gap) * ratio).round();
                        (R { h: ha, ..r }, R { y: r.y + ha + gap, h: r.h - ha - gap, ..r }, R { y: r.y + ha, h: gap, ..r })
                    }
                };
                dividers.push(Divider { path: path.clone(), dir: *dir, rect: line, parent: r });
                path.push(false);
                a.layout_inner(ra, gap, path, panes, dividers);
                path.pop();
                path.push(true);
                b.layout_inner(rb, gap, path, panes, dividers);
                path.pop();
            }
        }
    }

    /// Split the leaf `target`, putting `new` after it. Returns false if not found.
    pub fn split(&mut self, target: PaneId, new: PaneId, dir: Dir) -> bool {
        match self {
            Node::Leaf(id) if *id == target => {
                *self = Node::Split { dir, ratio: 0.5, a: Box::new(Node::Leaf(target)), b: Box::new(Node::Leaf(new)) };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { a, b, .. } => a.split(target, new, dir) || b.split(target, new, dir),
        }
    }

    /// Put `new` next to `target`: before it (left/above) when `first`.
    pub fn insert_beside(&mut self, target: PaneId, new: PaneId, dir: Dir, first: bool) -> bool {
        if !self.split(target, new, dir) {
            return false;
        }
        if first
            && let Some(path) = self.path_to(new)
            && let Some(parent) = self.at_path_mut(&path[..path.len() - 1])
            && let Node::Split { a, b, .. } = parent
        {
            std::mem::swap(a, b);
        }
        true
    }

    /// Exchange the places of two panes.
    pub fn swap(&mut self, x: PaneId, y: PaneId) {
        match self {
            Node::Leaf(id) if *id == x => *id = y,
            Node::Leaf(id) if *id == y => *id = x,
            Node::Leaf(_) => {}
            Node::Split { a, b, .. } => {
                a.swap(x, y);
                b.swap(x, y);
            }
        }
    }

    /// Remove leaf `target`; its sibling takes the parent's place.
    /// Returns false if not found or if `self` is that single leaf.
    pub fn remove(&mut self, target: PaneId) -> bool {
        let Node::Split { a, b, .. } = self else { return false };
        if matches!(**a, Node::Leaf(id) if id == target) {
            *self = std::mem::replace(&mut **b, Node::Leaf(0));
            return true;
        }
        if matches!(**b, Node::Leaf(id) if id == target) {
            *self = std::mem::replace(&mut **a, Node::Leaf(0));
            return true;
        }
        a.remove(target) || b.remove(target)
    }

    pub fn panes(&self) -> Vec<PaneId> {
        match self {
            Node::Leaf(id) => vec![*id],
            Node::Split { a, b, .. } => {
                let mut v = a.panes();
                v.extend(b.panes());
                v
            }
        }
    }

    pub fn at_path_mut(&mut self, path: &[bool]) -> Option<&mut Node> {
        match (path.split_first(), self) {
            (None, node) => Some(node),
            (Some((side, rest)), Node::Split { a, b, .. }) => {
                if *side {
                    b.at_path_mut(rest)
                } else {
                    a.at_path_mut(rest)
                }
            }
            _ => None,
        }
    }

    pub fn path_to(&self, target: PaneId) -> Option<Vec<bool>> {
        match self {
            Node::Leaf(id) => (*id == target).then(Vec::new),
            Node::Split { a, b, .. } => {
                if let Some(mut p) = a.path_to(target) {
                    p.insert(0, false);
                    Some(p)
                } else if let Some(mut p) = b.path_to(target) {
                    p.insert(0, true);
                    Some(p)
                } else {
                    None
                }
            }
        }
    }

    /// Nudge the divider of the closest enclosing split along `dir` by `delta`.
    pub fn resize(&mut self, target: PaneId, dir: Dir, delta: f32) {
        let Some(path) = self.path_to(target) else { return };
        for depth in (0..path.len()).rev() {
            if let Some(Node::Split { dir: d, ratio, .. }) = self.at_path_mut(&path[..depth])
                && *d == dir
            {
                *ratio = (*ratio + delta).clamp(0.1, 0.9);
                return;
            }
        }
    }
}

/// Pick the pane next to `from` in `dir`: it must sit on that side and overlap
/// on the other axis. Closest wins, then the one with the most overlap.
pub fn neighbor(rects: &[(PaneId, R)], from: PaneId, dir: Direction) -> Option<PaneId> {
    let cur = rects.iter().find(|(id, _)| *id == from)?.1;
    let eps = 2.0;
    let overlap = |a0: f32, a1: f32, b0: f32, b1: f32| (a1.min(b1) - a0.max(b0)).max(0.0);
    rects
        .iter()
        .filter(|(id, _)| *id != from)
        .filter_map(|(id, r)| {
            let (dist, ov) = match dir {
                Direction::Right if r.x >= cur.right() - eps => (r.x - cur.right(), overlap(cur.y, cur.bottom(), r.y, r.bottom())),
                Direction::Left if r.right() <= cur.x + eps => (cur.x - r.right(), overlap(cur.y, cur.bottom(), r.y, r.bottom())),
                Direction::Down if r.y >= cur.bottom() - eps => (r.y - cur.bottom(), overlap(cur.x, cur.right(), r.x, r.right())),
                Direction::Up if r.bottom() <= cur.y + eps => (cur.y - r.bottom(), overlap(cur.x, cur.right(), r.x, r.right())),
                _ => return None,
            };
            (ov > 0.0).then_some((*id, dist, ov))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1).then(b.2.total_cmp(&a.2)))
        .map(|(id, _, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> R {
        R { x: 0.0, y: 0.0, w: 1000.0, h: 600.0 }
    }

    #[test]
    fn split_and_remove() {
        let mut root = Node::Leaf(1);
        assert!(root.split(1, 2, Dir::Horizontal));
        assert!(root.split(2, 3, Dir::Vertical));
        assert_eq!(root.panes(), vec![1, 2, 3]);
        let (mut p, mut d) = (vec![], vec![]);
        root.layout(area(), 2.0, &mut p, &mut d);
        assert_eq!(d.len(), 2);
        assert!(root.remove(2));
        assert_eq!(root.panes(), vec![1, 3]);
        assert!(root.remove(1));
        assert!(matches!(root, Node::Leaf(3)));
        assert!(!root.remove(3));
    }

    #[test]
    fn neighbors() {
        let mut root = Node::Leaf(1);
        root.split(1, 2, Dir::Horizontal);
        root.split(2, 3, Dir::Vertical);
        let (mut p, mut d) = (vec![], vec![]);
        root.layout(area(), 2.0, &mut p, &mut d);
        assert_eq!(neighbor(&p, 1, Direction::Right), Some(2));
        assert_eq!(neighbor(&p, 2, Direction::Down), Some(3));
        assert_eq!(neighbor(&p, 3, Direction::Left), Some(1));
        assert_eq!(neighbor(&p, 1, Direction::Left), None);
    }

    #[test]
    fn insert_and_swap() {
        let mut root = Node::Leaf(1);
        root.split(1, 2, Dir::Horizontal);
        // Move 2 to above 1.
        root.remove(2);
        assert!(root.insert_beside(1, 2, Dir::Vertical, true));
        assert_eq!(root.panes(), vec![2, 1]);
        let (mut p, mut d) = (vec![], vec![]);
        root.layout(area(), 2.0, &mut p, &mut d);
        assert_eq!(neighbor(&p, 2, Direction::Down), Some(1));
        root.swap(1, 2);
        assert_eq!(root.panes(), vec![1, 2]);
        assert!(!root.insert_beside(9, 3, Dir::Vertical, false));
    }

    #[test]
    fn resize_finds_matching_split() {
        let mut root = Node::Leaf(1);
        root.split(1, 2, Dir::Horizontal);
        root.split(2, 3, Dir::Vertical);
        root.resize(3, Dir::Horizontal, 0.1);
        let Node::Split { ratio, .. } = root else { panic!() };
        assert!((ratio - 0.6).abs() < 1e-6);
    }
}
