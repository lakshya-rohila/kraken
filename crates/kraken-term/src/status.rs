//! Facts for the status bar: SSH target and git state.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Programs that mean "this pane is connected to another machine".
pub fn is_remote_program(name: &str) -> bool {
    matches!(name, "ssh" | "mosh" | "mosh-client" | "et" | "autossh" | "teleport" | "tsh")
}

/// Command-line arguments of a process (macOS `KERN_PROCARGS2`).
#[cfg(target_os = "macos")]
pub fn process_args(pid: u32) -> Option<Vec<String>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
    let mut size: libc::size_t = 0;
    // SAFETY: a size query: NULL output buffer, `size` is a live out-parameter.
    let ok = unsafe { libc::sysctl(mib.as_mut_ptr(), 3, std::ptr::null_mut(), &mut size, std::ptr::null_mut(), 0) };
    if ok != 0 || size < 4 {
        return None;
    }
    let mut buf = vec![0u8; size];
    // SAFETY: `buf` holds `size` bytes, and sysctl writes at most `size` bytes into it.
    let ok = unsafe { libc::sysctl(mib.as_mut_ptr(), 3, buf.as_mut_ptr() as *mut libc::c_void, &mut size, std::ptr::null_mut(), 0) };
    if ok != 0 {
        return None;
    }
    buf.truncate(size);
    // Layout: argc (i32), exec path, NUL padding, then argc NUL-terminated args.
    let argc = i32::from_ne_bytes(buf[..4].try_into().ok()?) as usize;
    let mut rest = &buf[4..];
    let path_end = rest.iter().position(|&b| b == 0)?;
    rest = &rest[path_end..];
    let start = rest.iter().position(|&b| b != 0)?;
    rest = &rest[start..];
    Some(rest.split(|&b| b == 0).take(argc).map(|a| String::from_utf8_lossy(a).into_owned()).collect())
}

#[cfg(not(target_os = "macos"))]
pub fn process_args(_pid: u32) -> Option<Vec<String>> {
    None
}

/// Pull `user@host` out of an ssh/mosh command line.
pub fn remote_target(args: &[String]) -> Option<String> {
    // ssh options that take a value as the next argument.
    const WITH_VALUE: &str = "bcDEeFIiJLlmOoPpQRSWw";
    let mut user: Option<String> = None;
    let mut it = args.iter().skip(1);
    while let Some(a) = it.next() {
        if a == "--" {
            return it.next().map(|h| join_user(h, &user));
        }
        if let Some(opts) = a.strip_prefix('-') {
            if opts.is_empty() || a.starts_with("--") {
                continue; // mosh-style long options carry "=value"
            }
            // Flags can be bundled (-vA); the first one that takes a value
            // consumes the rest of the argument, or the next argument.
            for (i, c) in opts.char_indices() {
                if WITH_VALUE.contains(c) {
                    let attached = &opts[i + 1..];
                    let value = if attached.is_empty() { it.next().cloned() } else { Some(attached.to_string()) };
                    if c == 'l' {
                        user = value;
                    }
                    break;
                }
            }
            continue;
        }
        return Some(join_user(a, &user));
    }
    None
}

fn join_user(host: &str, user: &Option<String>) -> String {
    let host = host.strip_prefix("ssh://").unwrap_or(host);
    match user {
        Some(u) if !host.contains('@') => format!("{u}@{host}"),
        _ => host.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitInfo {
    pub branch: String,
    /// Number of changed / untracked files.
    pub dirty: u32,
    pub ahead: u32,
    pub behind: u32,
    /// Changed paths relative to the repo root, with a one-letter status:
    /// `M` modified, `A` added, `?` untracked, `D` deleted, `R` renamed.
    pub changes: std::collections::BTreeMap<String, char>,
}

/// The repository root containing `dir`, if any.
pub fn git_root(dir: &Path) -> Option<PathBuf> {
    dir.ancestors().find(|d| d.join(".git").exists()).map(Path::to_path_buf)
}

/// Run `git status` (can take a while in big repos: call off the UI thread).
/// One changed file: the two-letter porcelain code (`" M"`, `"??"`, `"R "`…)
/// and its path relative to the repository root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitChange {
    pub code: String,
    pub path: String,
}

/// What `git status` lists, for the status bar's branch popover.
pub fn git_changes(root: &Path) -> Option<Vec<GitChange>> {
    let out = Command::new("git")
        .args(["--no-optional-locks", "status", "--porcelain=v1", "--untracked-files=normal"])
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(parse_git_changes(&String::from_utf8_lossy(&out.stdout)))
}

pub fn parse_git_changes(porcelain: &str) -> Vec<GitChange> {
    porcelain
        .lines()
        .filter(|l| l.len() > 3 && !l.starts_with("##"))
        .map(|l| {
            let path = &l[3..];
            // Renames read "old -> new": show where it lives now.
            let path = path.rsplit(" -> ").next().unwrap_or(path).trim_matches('"');
            GitChange { code: l[..2].to_string(), path: path.to_string() }
        })
        .collect()
}

pub fn git_status(root: &Path) -> Option<GitInfo> {
    let out = Command::new("git")
        .args(["--no-optional-locks", "status", "--porcelain=v1", "--branch"])
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(parse_git_status(&String::from_utf8_lossy(&out.stdout)))
}

pub fn parse_git_status(text: &str) -> GitInfo {
    let mut info = GitInfo::default();
    for line in text.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            let (names, track) = match head.split_once(" [") {
                Some((n, t)) => (n, t.trim_end_matches(']')),
                None => (head, ""),
            };
            info.branch = if let Some(b) = names.strip_prefix("No commits yet on ") {
                b.to_string()
            } else if names.starts_with("HEAD (no branch)") {
                "detached".to_string()
            } else {
                names.split("...").next().unwrap_or(names).to_string()
            };
            for part in track.split(", ") {
                if let Some(n) = part.strip_prefix("ahead ") {
                    info.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix("behind ") {
                    info.behind = n.parse().unwrap_or(0);
                }
            }
        } else if line.len() > 3 {
            info.dirty += 1;
            let code = &line[..2];
            let mut path = &line[3..];
            if let Some((_, to)) = path.split_once(" -> ") {
                path = to;
            }
            let path = path.trim_matches('"').trim_end_matches('/').to_string();
            let kind = if code == "??" {
                '?'
            } else if code.contains('D') {
                'D'
            } else if code.contains('A') {
                'A'
            } else if code.contains('R') {
                'R'
            } else {
                'M'
            };
            info.changes.insert(path, kind);
        }
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_changed_files() {
        let c = parse_git_changes(" M src/main.rs\n?? notes.txt\nR  old.rs -> new.rs\nD  gone.rs\n");
        assert_eq!(c.len(), 4);
        assert_eq!(c[0], GitChange { code: " M".into(), path: "src/main.rs".into() });
        assert_eq!(c[1].code, "??");
        assert_eq!(c[2].path, "new.rs");
        assert_eq!(c[3].code, "D ");
    }

    fn a(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn ssh_targets() {
        assert_eq!(remote_target(&a("ssh prod.example.com")), Some("prod.example.com".into()));
        assert_eq!(remote_target(&a("ssh -p 2222 -i key.pem ubuntu@10.0.0.5 uptime")), Some("ubuntu@10.0.0.5".into()));
        assert_eq!(remote_target(&a("ssh -l deploy -vA web1")), Some("deploy@web1".into()));
        assert_eq!(remote_target(&a("ssh -p2222 box")), Some("box".into()));
        let mosh: Vec<String> = ["mosh", "--ssh=ssh -p 22", "me@host"].map(String::from).to_vec();
        assert_eq!(remote_target(&mosh), Some("me@host".into()));
        assert_eq!(remote_target(&a("ssh -N")), None);
    }

    #[test]
    fn git_parsing() {
        let g = parse_git_status("## main...origin/main [ahead 2, behind 1]\n M src/a.rs\n?? new.txt\nR  old.rs -> new.rs\n");
        assert_eq!((g.branch.as_str(), g.dirty, g.ahead, g.behind), ("main", 3, 2, 1));
        assert_eq!(g.changes.get("src/a.rs"), Some(&'M'));
        assert_eq!(g.changes.get("new.txt"), Some(&'?'));
        assert_eq!(g.changes.get("new.rs"), Some(&'R'));
        assert_eq!(parse_git_status("## No commits yet on dev\n").branch, "dev");
        assert_eq!(parse_git_status("## HEAD (no branch)\n").branch, "detached");
        assert_eq!(parse_git_status("## feature/x\n").dirty, 0);
    }
}
