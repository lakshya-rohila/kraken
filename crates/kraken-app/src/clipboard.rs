//! What ⌘V pastes, beyond plain text.
//!
//! Coding agents take images and files as *paths*, so:
//!
//! - files copied in Finder paste as their (quoted) full paths, like dropping them;
//! - an image on the clipboard (a screenshot, a picture copied from a browser
//!   or Preview) is saved as a PNG and its path is pasted: Claude Code turns
//!   it into `[Image #1]`, Codex and Gemini read it as a file.
//!
//! Ctrl+V is never intercepted: agents that read the clipboard themselves
//! (Claude Code does) still get it.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// What the clipboard holds, in the order ⌘V prefers it.
pub enum Clip {
    Files(Vec<PathBuf>),
    Text(String),
    Image { width: usize, height: usize, rgba: Vec<u8> },
    Empty,
}

pub fn read(cb: &mut arboard::Clipboard) -> Clip {
    // Finder copies put the file *name* in the text slot; the paths are what's useful.
    if let Ok(files) = cb.get().file_list()
        && !files.is_empty()
    {
        return Clip::Files(files);
    }
    if let Ok(text) = cb.get_text()
        && !text.is_empty()
    {
        return Clip::Text(text);
    }
    if let Ok(img) = cb.get_image()
        && img.width > 0
        && img.height > 0
    {
        return Clip::Image { width: img.width, height: img.height, rgba: img.bytes.into_owned() };
    }
    Clip::Empty
}

/// Paths as they'd be typed: quoted where needed, space-separated, with a
/// trailing space so you can keep typing.
pub fn paths_text(paths: &[PathBuf]) -> String {
    let mut s = paths.iter().map(|p| crate::open::shell_quote(p)).collect::<Vec<_>>().join(" ");
    s.push(' ');
    s
}

/// Where pasted images are kept (agents read them after the paste).
pub fn pastes_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("Library/Caches/Kraken/pastes")
}

/// Save clipboard pixels as a PNG in `dir`; returns its path.
pub fn save_png(dir: &Path, width: usize, height: usize, rgba: Vec<u8>) -> Result<PathBuf, String> {
    use image::ImageEncoder;
    std::fs::create_dir_all(dir).map_err(|e| format!("can't create {}: {e}", dir.display()))?;
    if rgba.len() < width * height * 4 {
        return Err("clipboard image data is incomplete".into());
    }
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let secs = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = dir.join(format!("paste-{secs}-{n}.png"));
    let file = std::fs::File::create(&path).map_err(|e| format!("can't write {}: {e}", path.display()))?;
    // Fast compression: a 5K screenshot encodes in a blink and agents don't care about size.
    let enc = image::codecs::png::PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    );
    enc.write_image(&rgba[..width * height * 4], width as u32, height as u32, image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("can't encode image: {e}"))?;
    Ok(path)
}

/// Delete pasted images older than `max_age` (run once at startup).
pub fn clean_old(dir: &Path, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let now = SystemTime::now();
    for e in entries.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| now.duration_since(t).unwrap_or_default() > max_age);
        let ours = e.file_name().to_string_lossy().starts_with("paste-");
        if old && ours {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_a_valid_png() {
        let dir = std::env::temp_dir().join(format!("kraken-paste-test-{}", std::process::id()));
        let (w, h) = (3, 2);
        let rgba: Vec<u8> = (0..w * h).flat_map(|i| [i as u8 * 40, 100, 200, 255]).collect();
        let path = save_png(&dir, w, h, rgba).unwrap();
        let img = image::open(&path).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (3, 2));
        assert_eq!(img.get_pixel(1, 0).0, [40, 100, 200, 255]);
        assert!(save_png(&dir, 10, 10, vec![0; 4]).is_err(), "short data is refused, not a crash");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn paths_are_quoted_and_spaced() {
        let p = vec![PathBuf::from("/tmp/a b.png"), PathBuf::from("/tmp/c.png")];
        assert_eq!(paths_text(&p), "'/tmp/a b.png' /tmp/c.png ");
    }

    #[test]
    fn cleanup_only_touches_old_pastes() {
        let dir = std::env::temp_dir().join(format!("kraken-clean-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("paste-1.png"), b"x").unwrap();
        std::fs::write(dir.join("keep.txt"), b"x").unwrap();
        clean_old(&dir, Duration::from_secs(3600)); // nothing is that old yet
        assert!(dir.join("paste-1.png").exists());
        clean_old(&dir, Duration::ZERO);
        assert!(!dir.join("paste-1.png").exists(), "old pastes go");
        assert!(dir.join("keep.txt").exists(), "other files stay");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
