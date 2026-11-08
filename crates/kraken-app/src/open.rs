//! Opening things: URLs in the browser, files in your editor (at a line),
//! Quick Look previews, and shell-safe quoting for pasted paths.

use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub enum Editor {
    /// A command-line launcher; `style` decides how the line is passed.
    Cli { bin: PathBuf, style: LineStyle },
    /// `$VISUAL`/`$EDITOR` in a new split inside Kraken.
    Terminal,
    /// Whatever app macOS associates with the file type (no line support).
    System,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineStyle {
    /// `code -g file:line:col` (VS Code, Cursor).
    Goto,
    /// `zed file:line:col`, `subl file:line:col`.
    Suffix,
}

/// Known editors: (setting name, CLI locations to try, how to pass a line).
const KNOWN: &[(&str, &[&str], LineStyle)] = &[
    (
        "cursor",
        &[
            "~/.local/bin/cursor",
            "/usr/local/bin/cursor",
            "/opt/homebrew/bin/cursor",
            "/Applications/Cursor.app/Contents/Resources/app/bin/cursor",
        ],
        LineStyle::Goto,
    ),
    (
        "code",
        &[
            "/usr/local/bin/code",
            "/opt/homebrew/bin/code",
            "~/.local/bin/code",
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
        ],
        LineStyle::Goto,
    ),
    ("zed", &["/usr/local/bin/zed", "/opt/homebrew/bin/zed", "/Applications/Zed.app/Contents/MacOS/cli"], LineStyle::Suffix),
    (
        "subl",
        &["/usr/local/bin/subl", "/opt/homebrew/bin/subl", "/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl"],
        LineStyle::Suffix,
    ),
];

fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(rest),
        None => PathBuf::from(p),
    }
}

/// Turn the `editor` setting into something we can launch.
/// `auto` picks the first installed GUI editor, else the terminal editor.
pub fn resolve(setting: &str) -> Editor {
    let find = |name: &str| {
        KNOWN.iter().find(|(n, ..)| *n == name).and_then(|(_, paths, style)| {
            paths.iter().map(|p| expand(p)).find(|p| p.is_file()).map(|bin| Editor::Cli { bin, style: *style })
        })
    };
    match setting {
        "terminal" => Editor::Terminal,
        "system" => Editor::System,
        "auto" | "" => KNOWN.iter().find_map(|(n, ..)| find(n)).unwrap_or(Editor::Terminal),
        name => find(name).unwrap_or_else(|| {
            let p = expand(name);
            if p.is_file() { Editor::Cli { bin: p, style: LineStyle::Goto } } else { Editor::Terminal }
        }),
    }
}

/// Open `path` in a GUI editor or the default app. (Terminal editing is done
/// by the window, which owns the panes.)
pub fn open_file(editor: &Editor, path: &Path, line: Option<u32>, col: Option<u32>) {
    let target = match (line, col) {
        (Some(l), Some(c)) => format!("{}:{l}:{c}", path.display()),
        (Some(l), None) => format!("{}:{l}", path.display()),
        _ => path.display().to_string(),
    };
    let mut cmd = match editor {
        Editor::Cli { bin, style: LineStyle::Goto } => {
            let mut c = Command::new(bin);
            if line.is_some() {
                c.arg("-g");
            }
            c.arg(&target);
            c
        }
        Editor::Cli { bin, style: LineStyle::Suffix } => {
            let mut c = Command::new(bin);
            c.arg(&target);
            c
        }
        Editor::System | Editor::Terminal => {
            let mut c = Command::new("open");
            c.arg(path);
            c
        }
    };
    spawn(&mut cmd);
}

pub fn open_url(url: &str) {
    spawn(Command::new("open").arg(url));
}

/// Show a file in macOS Quick Look (the space-bar preview).
pub fn quick_look(path: &Path) {
    spawn(Command::new("qlmanage").arg("-p").arg(path));
}

fn spawn(cmd: &mut Command) {
    use std::process::Stdio;
    let cmd = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    match cmd.spawn() {
        // Reap it in the background so it doesn't linger as a zombie.
        Ok(mut child) => {
            crate::tasks::spawn_long("reap-editor", move || {
                let _ = child.wait();
            });
        }
        Err(e) => log::error!("could not run {:?}: {e}", cmd.get_program()),
    }
}

/// Quote a path for the shell: plain if safe, otherwise single-quoted.
pub fn shell_quote(path: &Path) -> String {
    let s = path.to_string_lossy();
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "/._-+@%:,~".contains(c)) {
        s.into_owned()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// The command typed into a new pane to edit a file with the shell's editor.
/// `exec` makes the pane close when the editor quits.
pub fn terminal_edit_command(path: &Path, line: Option<u32>) -> String {
    let at = line.map(|l| format!("+{l} ")).unwrap_or_default();
    format!("exec ${{VISUAL:-${{EDITOR:-vi}}}} {at}-- {}\r", shell_quote(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(shell_quote(Path::new("/tmp/a.rs")), "/tmp/a.rs");
        assert_eq!(shell_quote(Path::new("/tmp/My Files/x.txt")), "'/tmp/My Files/x.txt'");
        assert_eq!(shell_quote(Path::new("/tmp/it's")), "'/tmp/it'\\''s'");
    }

    #[test]
    fn terminal_command() {
        assert_eq!(terminal_edit_command(Path::new("/a b.rs"), Some(42)), "exec ${VISUAL:-${EDITOR:-vi}} +42 -- '/a b.rs'\r");
        assert_eq!(terminal_edit_command(Path::new("/x.rs"), None), "exec ${VISUAL:-${EDITOR:-vi}} -- /x.rs\r");
    }

    #[test]
    fn unknown_editor_falls_back_to_terminal() {
        assert_eq!(resolve("terminal"), Editor::Terminal);
        assert_eq!(resolve("/no/such/editor"), Editor::Terminal);
    }
}
