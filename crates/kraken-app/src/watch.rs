//! Live settings reload: watch kraken.toml and tell the event loop when it
//! changes (instead of polling its modification time every second).
//!
//! The *directory* is watched, not the file: most editors save by writing a
//! temporary file and renaming it over the original, which replaces the
//! file's inode and would silently end a watch on the file itself.

use crate::UserEvent;
use notify::{EventKind, RecursiveMode, Watcher};
use std::path::Path;
use winit::event_loop::EventLoopProxy;

/// Keep the returned watcher alive for as long as changes should be noticed.
pub fn config(path: &Path, proxy: EventLoopProxy<UserEvent>) -> Option<notify::RecommendedWatcher> {
    let dir = path.parent()?;
    if let Err(e) = std::fs::create_dir_all(dir) {
        log::warn!("can't watch settings folder {}: {e}", dir.display());
        return None;
    }
    let name = path.file_name()?.to_owned();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        let relevant = matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_));
        if relevant && event.paths.iter().any(|p| p.file_name() == Some(name.as_os_str())) {
            let _ = proxy.send_event(UserEvent::ConfigChanged);
        }
    })
    .map_err(|e| log::warn!("settings watcher unavailable: {e}"))
    .ok()?;
    watcher.watch(dir, RecursiveMode::NonRecursive).map_err(|e| log::warn!("can't watch {}: {e}", dir.display())).ok()?;
    Some(watcher)
}
