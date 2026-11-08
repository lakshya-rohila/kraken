//! Find the URL or file path under the mouse, so ⌘-click can open it.
//! Understands `path:line:col` (compiler errors, grep -n, stack traces)
//! and git diff prefixes (`a/src/x.rs`).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum Link {
    Url(String),
    File { path: PathBuf, line: Option<u32>, col: Option<u32> },
}

/// Characters that end a token (paths and URLs never contain these unescaped).
fn is_delim(c: char) -> bool {
    c.is_whitespace() || "\"'`<>()[]{}|│".contains(c)
}

/// `row` is the line's text, one char per column (wide-char spacers removed,
/// with `cols[i]` the grid column of `row[i]`). Returns the link and the
/// first/last grid column it covers.
pub fn find_at(row: &[char], cols: &[usize], at_col: usize, cwd: Option<&Path>) -> Option<(Link, usize, usize)> {
    let i = cols.iter().position(|&c| c == at_col)?;
    if is_delim(row[i]) {
        return None;
    }
    let mut s = i;
    while s > 0 && !is_delim(row[s - 1]) {
        s -= 1;
    }
    let mut e = i;
    while e + 1 < row.len() && !is_delim(row[e + 1]) {
        e += 1;
    }
    // Sentence punctuation stuck to the end isn't part of the link.
    while e > s && ".,;:!?".contains(row[e]) {
        e -= 1;
    }
    if i > e {
        return None;
    }
    let token: String = row[s..=e].iter().collect();
    classify(&token, cwd).map(|l| (l, cols[s], cols[e]))
}

pub fn classify(token: &str, cwd: Option<&Path>) -> Option<Link> {
    if let Some(p) = token.find("://") {
        let scheme = &token[..p];
        if !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphanumeric() || c == '+') {
            if scheme == "file" {
                return Some(Link::File { path: PathBuf::from(&token[p + 3..]), line: None, col: None });
            }
            return Some(Link::Url(token.to_string()));
        }
    }
    if token.starts_with("www.") && token.len() > 6 {
        return Some(Link::Url(format!("https://{token}")));
    }
    let (path, line, col) = split_line_col(token);
    if !(path.contains('/') || path.contains('.')) || path.len() < 2 {
        return None;
    }
    resolve_path(path, cwd).map(|path| Link::File { path, line, col })
}

/// `src/a.rs:12:5` → ("src/a.rs", 12, 5).
fn split_line_col(token: &str) -> (&str, Option<u32>, Option<u32>) {
    let mut parts = token.rsplitn(3, ':');
    let last = parts.next().unwrap_or("");
    let mid = parts.next();
    let first = parts.next();
    match (first, mid.map(str::parse::<u32>), last.parse::<u32>()) {
        (Some(p), Some(Ok(l)), Ok(c)) => (p, Some(l), Some(c)),
        _ => match token.rsplit_once(':') {
            Some((p, l)) if l.parse::<u32>().is_ok() => (p, l.parse().ok(), None),
            _ => (token.trim_end_matches(':'), None, None),
        },
    }
}

/// An existing file or folder for `raw`, trying the forms tools print.
fn resolve_path(raw: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut push = |p: &str| {
        if let Some(rest) = p.strip_prefix("~/") {
            if let Some(h) = &home {
                candidates.push(h.join(rest));
            }
        } else if p.starts_with('/') {
            candidates.push(PathBuf::from(p));
        } else if let Some(c) = cwd {
            candidates.push(c.join(p));
        }
    };
    push(raw);
    // git diff: "a/src/x.rs" and "b/src/x.rs".
    if let Some(rest) = raw.strip_prefix("a/").or_else(|| raw.strip_prefix("b/")) {
        push(rest);
    }
    candidates.into_iter().find(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str, col: usize, cwd: &Path) -> Option<(Link, usize, usize)> {
        let row: Vec<char> = text.chars().collect();
        let cols: Vec<usize> = (0..row.len()).collect();
        find_at(&row, &cols, col, Some(cwd))
    }

    #[test]
    fn urls() {
        let cwd = Path::new("/");
        let (l, s, e) = at("see https://example.com/a?b=1. ok", 10, cwd).unwrap();
        assert_eq!(l, Link::Url("https://example.com/a?b=1".into()));
        assert_eq!((s, e), (4, 28));
        assert_eq!(at("(www.rust-lang.org)", 5, cwd).unwrap().0, Link::Url("https://www.rust-lang.org".into()));
        assert!(at("just words here", 3, cwd).is_none());
    }

    #[test]
    fn paths_with_lines() {
        let dir = std::env::temp_dir().join("kraken-links-test");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/main.rs"), "").unwrap();
        let (l, ..) = at("error: src/main.rs:42:7: oops", 10, &dir).unwrap();
        assert_eq!(l, Link::File { path: dir.join("src/main.rs"), line: Some(42), col: Some(7) });
        let (l, ..) = at("--- a/src/main.rs", 8, &dir).unwrap();
        assert_eq!(l, Link::File { path: dir.join("src/main.rs"), line: None, col: None });
        let (l, ..) = at("src/main.rs:9", 3, &dir).unwrap();
        assert!(matches!(l, Link::File { line: Some(9), col: None, .. }));
        // Paths that don't exist aren't links (no false positives on prose).
        assert!(at("src/nope.rs:1", 3, &dir).is_none());
    }
}
