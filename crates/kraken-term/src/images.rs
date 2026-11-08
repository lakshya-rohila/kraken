//! Inline images: Kitty graphics protocol, iTerm2 `imgcat`, and sixel.
//!
//! The terminal emulator underneath knows nothing about images, so we use a
//! trick: when an image arrives, we decode it, store it in a "placement slot",
//! and send the emulator a few ordinary characters instead — one placeholder
//! per image row, from Unicode's Private Use Area. Each placeholder encodes
//! (slot, row). The grid stores, scrolls and reflows them like any text, and
//! the renderer draws the real image wherever it finds them.

use base64::Engine;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// First placeholder code point: U+F0000 + slot * 256 + row.
pub const PLACEHOLDER_BASE: u32 = 0xF0000;
pub const MAX_SLOTS: usize = 255;
pub const MAX_ROWS: usize = 256;

static NEXT_KEY: AtomicU64 = AtomicU64::new(1);

/// A decoded image (RGBA8, straight alpha).
#[derive(Debug)]
pub struct Image {
    /// Globally unique: the renderer caches GPU textures by this.
    pub key: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<Vec<u8>>,
}

impl Image {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Arc<Image> {
        Arc::new(Image { key: NEXT_KEY.fetch_add(1, Ordering::Relaxed), width, height, rgba: Arc::new(rgba) })
    }
}

#[derive(Debug, Clone)]
pub struct Placement {
    pub image: Arc<Image>,
    pub cols: u32,
    pub rows: u32,
    /// Size to draw at, in physical pixels (keeps the aspect ratio).
    pub draw_w: f32,
    pub draw_h: f32,
}

/// Cell size and width of the pane, needed to size images in cells.
#[derive(Debug, Clone, Copy)]
pub struct Metrics {
    pub cell_w: f32,
    pub cell_h: f32,
    pub cols: usize,
}

pub struct ImageStore {
    /// Images by the id programs gave them (Kitty `i=`).
    by_id: HashMap<u32, Arc<Image>>,
    placements: Vec<Option<Placement>>,
    next_slot: usize,
    pub metrics: Metrics,
}

pub type SharedImages = Arc<Mutex<ImageStore>>;

impl ImageStore {
    pub fn new(metrics: Metrics) -> SharedImages {
        Arc::new(Mutex::new(ImageStore { by_id: HashMap::new(), placements: vec![None; MAX_SLOTS], next_slot: 0, metrics }))
    }

    pub fn placement(&self, slot: usize) -> Option<&Placement> {
        self.placements.get(slot)?.as_ref()
    }

    pub fn clear(&mut self) {
        self.by_id.clear();
        self.placements.iter_mut().for_each(|p| *p = None);
    }

    /// Size an image in cells (shrinking it to fit the pane) and give it a
    /// slot. Returns the bytes to feed the emulator in the image's place.
    pub fn place(&mut self, image: Arc<Image>, cols: Option<u32>, rows: Option<u32>) -> Vec<u8> {
        let m = self.metrics;
        let (iw, ih) = (image.width as f32, image.height as f32);
        let max_w = (m.cols.max(1) as f32) * m.cell_w;
        let (dw, dh) = match (cols, rows) {
            (Some(c), Some(r)) => (c as f32 * m.cell_w, r as f32 * m.cell_h),
            (Some(c), None) => (c as f32 * m.cell_w, c as f32 * m.cell_w * ih / iw),
            (None, Some(r)) => (r as f32 * m.cell_h * iw / ih, r as f32 * m.cell_h),
            (None, None) => {
                let scale = (max_w / iw).min(1.0);
                (iw * scale, ih * scale)
            }
        };
        let cols = ((dw / m.cell_w).ceil() as u32).clamp(1, m.cols.max(1) as u32);
        let rows = ((dh / m.cell_h).ceil() as u32).clamp(1, MAX_ROWS as u32);
        let slot = self.next_slot;
        self.next_slot = (self.next_slot + 1) % MAX_SLOTS;
        self.placements[slot] =
            Some(Placement { image, cols, rows, draw_w: dw.min(cols as f32 * m.cell_w), draw_h: dh.min(rows as f32 * m.cell_h) });
        placeholder_bytes(slot, cols, rows)
    }
}

/// Text that reserves the image's area: a placeholder at the start of each
/// row, keeping the cursor's column, and leaving the cursor just after the
/// image's last row (as the Kitty protocol specifies).
pub fn placeholder_bytes(slot: usize, cols: u32, rows: u32) -> Vec<u8> {
    let mut out = String::new();
    for r in 0..rows {
        out.push(char::from_u32(PLACEHOLDER_BASE + slot as u32 * 256 + r).unwrap_or(' '));
        if r + 1 < rows {
            // Down one line (scrolling if needed), back to the image's column.
            out.push_str("\n\x1b[1D");
        } else if cols > 1 {
            out.push_str(&format!("\x1b[{}C", cols - 1));
        }
    }
    out.into_bytes()
}

/// Decode a placeholder character into (slot, row).
pub fn decode_placeholder(c: char) -> Option<(usize, u32)> {
    let v = (c as u32).checked_sub(PLACEHOLDER_BASE)?;
    let (slot, row) = ((v / 256) as usize, v % 256);
    (slot < MAX_SLOTS).then_some((slot, row))
}

pub fn decode_file(bytes: &[u8]) -> Option<Arc<Image>> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(MAX_IMAGE_BYTES as u64);
    reader.limits(limits);
    let img = reader.decode().ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    rgba_len(w, h)?;
    Some(Image::new(w, h, img.into_raw()))
}

fn b64(data: &[u8]) -> Option<Vec<u8>> {
    let clean: Vec<u8> = data.iter().copied().filter(|c| !c.is_ascii_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(&clean)
        .ok()
        .or_else(|| base64::engine::general_purpose::STANDARD_NO_PAD.decode(&clean).ok())
}

// ── iTerm2: OSC 1337 ; File=[args] : base64 ST ────────────────────────────

/// Handle the body of an `OSC 1337;File=` sequence. Returns placeholder bytes.
pub fn iterm_image(store: &SharedImages, body: &[u8]) -> Option<Vec<u8>> {
    let rest = body.strip_prefix(b"1337;File=")?;
    let colon = rest.iter().position(|&b| b == b':')?;
    let args = String::from_utf8_lossy(&rest[..colon]);
    if !args.split(';').any(|a| a == "inline=1") {
        return Some(Vec::new()); // a file download, not an image: ignore it
    }
    let data = b64(&rest[colon + 1..])?;
    let image = decode_file(&data)?;
    let mut store = store.lock().ok()?;
    let m = store.metrics;
    // width=/height= may be N (cells), Npx, N% or auto.
    let size = |key: &str, cell: f32, total: f32| {
        let v = args.split(';').find_map(|a| a.strip_prefix(key))?;
        if let Some(px) = v.strip_suffix("px") {
            px.parse::<f32>().ok().map(|p| (p / cell).ceil() as u32)
        } else if let Some(pct) = v.strip_suffix('%') {
            pct.parse::<f32>().ok().map(|p| (total * p / 100.0 / cell).ceil() as u32)
        } else {
            v.parse::<u32>().ok()
        }
    };
    let cols = size("width=", m.cell_w, m.cols as f32 * m.cell_w);
    let rows = size("height=", m.cell_h, 40.0 * m.cell_h);
    Some(store.place(image, cols, rows))
}

/// The most decoded image data one image may use (also caps chunked uploads
/// and decompression), so hostile output can't exhaust memory.
pub const MAX_IMAGE_BYTES: usize = 256 * 1024 * 1024;
/// Largest width or height accepted for any image, in pixels.
pub const MAX_IMAGE_SIDE: u32 = 16_384;

/// Size check shared by every decoder: non-empty, sides within
/// [`MAX_IMAGE_SIDE`], RGBA bytes within [`MAX_IMAGE_BYTES`] (no overflow).
fn rgba_len(w: u32, h: u32) -> Option<usize> {
    if w == 0 || h == 0 || w > MAX_IMAGE_SIDE || h > MAX_IMAGE_SIDE {
        return None;
    }
    (w as usize).checked_mul(h as usize)?.checked_mul(4).filter(|&n| n <= MAX_IMAGE_BYTES)
}

// ── Kitty graphics: APC G key=value,... ; payload ST ───────────────────────

/// Accumulates chunked Kitty transmissions (`m=1` … `m=0`).
#[derive(Default)]
pub struct KittyState {
    pending: Option<(HashMap<String, String>, Vec<u8>)>,
}

pub struct KittyResult {
    /// Bytes for the emulator (placeholders), if an image was displayed.
    pub output: Vec<u8>,
    /// A reply for the program (sent back through the PTY).
    pub reply: Option<Vec<u8>>,
}

fn parse_kv(s: &str) -> HashMap<String, String> {
    s.split(',').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

impl KittyState {
    /// Bytes buffered from an unfinished chunked transmission.
    pub fn pending_bytes(&self) -> usize {
        self.pending.as_ref().map_or(0, |(_, data)| data.len())
    }

    /// `body` is the APC content after the leading `G`.
    pub fn handle(&mut self, store: &SharedImages, body: &[u8]) -> KittyResult {
        let none = KittyResult { output: Vec::new(), reply: None };
        let (ctrl, payload) = match body.iter().position(|&b| b == b';') {
            Some(i) => (&body[..i], &body[i + 1..]),
            None => (body, &[][..]),
        };
        let mut keys = parse_kv(&String::from_utf8_lossy(ctrl));
        let more = keys.get("m").is_some_and(|m| m == "1");

        // Chunked data: later chunks only carry m= (and maybe q=).
        // Base64 of the largest allowed image; anything beyond is hostile.
        const MAX_PENDING: usize = MAX_IMAGE_BYTES / 3 * 4 + 4;
        if self.pending_bytes() + payload.len() > MAX_PENDING {
            let id = self.pending.take().and_then(|(k, _)| k.get("i").cloned()).or_else(|| keys.get("i").cloned());
            return KittyResult { output: Vec::new(), reply: id.map(|i| format!("\x1b_Gi={i};EFBIG:image too large\x1b\\").into_bytes()) };
        }
        let data = if let Some((first, mut acc)) = self.pending.take() {
            acc.extend_from_slice(payload);
            if more {
                self.pending = Some((first, acc));
                return none;
            }
            keys = first;
            acc
        } else if more {
            self.pending = Some((keys, payload.to_vec()));
            return none;
        } else {
            payload.to_vec()
        };

        let get = |k: &str| keys.get(k).map(String::as_str);
        let num = |k: &str| get(k).and_then(|v| v.parse::<u32>().ok());
        let action = get("a").unwrap_or("t");
        let id = num("i");
        let quiet = num("q").unwrap_or(0);
        let reply = |msg: &str| {
            let ok = msg == "OK";
            if (ok && quiet >= 1) || quiet >= 2 {
                return None;
            }
            id.map(|i| format!("\x1b_Gi={i};{msg}\x1b\\").into_bytes())
        };

        match action {
            "d" => {
                // Delete: we drop everything (placements simply stop drawing).
                if let Ok(mut s) = store.lock() {
                    s.clear();
                }
                none
            }
            "p" => {
                let Some(img) = id.and_then(|i| store.lock().ok()?.by_id.get(&i).cloned()) else {
                    return KittyResult { output: Vec::new(), reply: reply("ENOENT:image not found") };
                };
                let output = store.lock().map(|mut s| s.place(img, num("c"), num("r"))).unwrap_or_default();
                KittyResult { output, reply: reply("OK") }
            }
            "t" | "T" | "q" => {
                let image = match decode_kitty_payload(&keys, &data) {
                    Ok(img) => img,
                    Err(e) => return KittyResult { output: Vec::new(), reply: reply(&format!("EINVAL:{e}")) },
                };
                if action == "q" {
                    return KittyResult { output: Vec::new(), reply: reply("OK") };
                }
                let Ok(mut s) = store.lock() else { return none };
                if let Some(i) = id {
                    s.by_id.insert(i, image.clone());
                }
                let output = if action == "T" { s.place(image, num("c"), num("r")) } else { Vec::new() };
                KittyResult { output, reply: reply("OK") }
            }
            _ => none,
        }
    }
}

fn decode_kitty_payload(keys: &HashMap<String, String>, payload: &[u8]) -> Result<Arc<Image>, &'static str> {
    let get = |k: &str| keys.get(k).map(String::as_str);
    let num = |k: &str| get(k).and_then(|v| v.parse::<u32>().ok());
    let raw = b64(payload).ok_or("bad base64")?;
    // t=f / t=t: the payload is a file path; d (direct) is the default.
    let mut bytes = match get("t").unwrap_or("d") {
        "d" => raw,
        "f" | "t" => {
            let path = String::from_utf8(raw).map_err(|_| "bad path")?;
            let data = read_image_file(std::path::Path::new(&path))?;
            if get("t") == Some("t") {
                delete_temp_file(std::path::Path::new(&path));
            }
            data
        }
        _ => return Err("unsupported transmission medium"),
    };
    if get("o") == Some("z") {
        use std::io::Read;
        let mut out = Vec::new();
        // Bounded: a few KB of zlib can expand to gigabytes.
        flate2::read::ZlibDecoder::new(&bytes[..]).take(MAX_IMAGE_BYTES as u64 + 1).read_to_end(&mut out).map_err(|_| "bad zlib data")?;
        if out.len() > MAX_IMAGE_BYTES {
            return Err("image too large");
        }
        bytes = out;
    }
    match num("f").unwrap_or(32) {
        100 => decode_file(&bytes).ok_or("cannot decode PNG"),
        f @ (24 | 32) => {
            let (w, h) = (num("s").ok_or("missing width")?, num("v").ok_or("missing height")?);
            let px = rgba_len(w, h).ok_or("bad image size")? / 4;
            let rgba = if f == 32 {
                (bytes.len() >= px * 4).then(|| bytes[..px * 4].to_vec()).ok_or("short data")?
            } else {
                if bytes.len() < px * 3 {
                    return Err("short data");
                }
                bytes[..px * 3].as_chunks::<3>().0.iter().flat_map(|c| [c[0], c[1], c[2], 255]).collect()
            };
            Ok(Image::new(w, h, rgba))
        }
        _ => Err("unsupported format"),
    }
}

/// Read a file named by a Kitty `t=f`/`t=t` transmission. Programs (maybe on
/// a remote host) choose the path, so only regular files of sane size are
/// read: never devices (`/dev/zero` would never end) or FIFOs (would block).
fn read_image_file(path: &std::path::Path) -> Result<Vec<u8>, &'static str> {
    use std::io::Read;
    let path = path.canonicalize().map_err(|_| "cannot read file")?;
    let meta = std::fs::metadata(&path).map_err(|_| "cannot read file")?;
    if !meta.is_file() || ["/dev", "/proc", "/sys"].iter().any(|p| path.starts_with(p)) {
        return Err("not a regular file");
    }
    if meta.len() > MAX_IMAGE_BYTES as u64 {
        return Err("image too large");
    }
    let mut data = Vec::new();
    std::fs::File::open(&path).and_then(|f| f.take(MAX_IMAGE_BYTES as u64).read_to_end(&mut data)).map_err(|_| "cannot read file")?;
    Ok(data)
}

/// `t=t` means "a temporary file: delete it after reading". Like kitty, only
/// honour that for files in a temp folder whose name says they are graphics
/// temp files, so output can't delete arbitrary files of yours.
fn delete_temp_file(path: &std::path::Path) {
    let Ok(path) = path.canonicalize() else { return };
    let named = path.file_name().is_some_and(|n| n.to_string_lossy().contains("tty-graphics-protocol"));
    let temp_roots = [std::env::temp_dir(), "/tmp".into(), "/private/tmp".into(), "/dev/shm".into()];
    let in_temp = temp_roots.iter().filter_map(|r| r.canonicalize().ok()).any(|r| path.starts_with(r));
    if named && in_temp && path.is_file() {
        let _ = std::fs::remove_file(&path);
    }
}

// ── Sixel: DCS P1;P2;P3 q <data> ST ────────────────────────────────────────

/// If a DCS body is sixel (`params q data`), decode it.
pub fn sixel_image(body: &[u8]) -> Option<Arc<Image>> {
    let q = body.iter().position(|&b| b == b'q')?;
    if !body[..q].iter().all(|b| b.is_ascii_digit() || *b == b';') {
        return None;
    }
    decode_sixel(&body[q + 1..])
}

/// A compact sixel decoder: colour registers (#n;2;r;g;b and HLS), repeats
/// (!n), carriage return ($), next band (-), raster attributes (").
///
/// Everything here comes from program output, so every number is clamped:
/// repeats and raster sizes to [`MAX_IMAGE_SIDE`], colour registers to
/// [`MAX_REGISTERS`], and painted pixels to the [`MAX_IMAGE_BYTES`] budget.
pub fn decode_sixel(data: &[u8]) -> Option<Arc<Image>> {
    const MAX_REGISTERS: usize = 1024;
    let side = MAX_IMAGE_SIDE as usize;
    let mut palette: Vec<[u8; 4]> = default_palette();
    let mut color = 0usize;
    let (mut x, mut y) = (0usize, 0usize);
    let (mut w, mut h) = (0usize, 0usize);
    // One Vec per pixel row; `None` = transparent. Memory tracks what's painted.
    let mut canvas: Vec<Vec<Option<[u8; 4]>>> = Vec::new();
    let mut budget = MAX_IMAGE_BYTES / 4;
    let mut i = 0;
    // Saturating: "99999999999999999999" is a number, just a big one.
    let read_num = |i: &mut usize| -> Option<usize> {
        let start = *i;
        let mut n = 0usize;
        while *i < data.len() && data[*i].is_ascii_digit() {
            n = n.saturating_mul(10).saturating_add((data[*i] - b'0') as usize);
            *i += 1;
        }
        (*i > start).then_some(n)
    };
    while i < data.len() {
        let c = data[i];
        match c {
            b'"' => {
                // Raster attributes: "Pan;Pad;Ph;Pv — we only use the size hint.
                i += 1;
                let mut nums = Vec::new();
                loop {
                    nums.push(read_num(&mut i).unwrap_or(0));
                    if i < data.len() && data[i] == b';' && nums.len() < 8 {
                        i += 1;
                    } else {
                        break;
                    }
                }
                if nums.len() >= 4 {
                    w = w.max(nums[2].min(side));
                    h = h.max(nums[3].min(side));
                }
                continue;
            }
            b'#' => {
                i += 1;
                let n = read_num(&mut i).unwrap_or(0).min(MAX_REGISTERS - 1);
                if i < data.len() && data[i] == b';' {
                    let mut v = Vec::new();
                    while i < data.len() && data[i] == b';' && v.len() < 8 {
                        i += 1;
                        v.push(read_num(&mut i).unwrap_or(0));
                    }
                    if v.len() >= 4 {
                        let rgb = if v[0] == 1 {
                            hls_to_rgb(v[1].min(360), v[2].min(100), v[3].min(100))
                        } else {
                            let pct = |p: usize| (p.min(100) * 255 / 100) as u8;
                            [pct(v[1]), pct(v[2]), pct(v[3])]
                        };
                        if n >= palette.len() {
                            palette.resize(n + 1, [0, 0, 0, 255]);
                        }
                        palette[n] = [rgb[0], rgb[1], rgb[2], 255];
                    }
                }
                color = n;
                continue;
            }
            b'!' => {
                i += 1;
                let n = read_num(&mut i).unwrap_or(1).min(side);
                if i < data.len() && (0x3f..=0x7e).contains(&data[i]) {
                    let c = palette.get(color).copied().unwrap_or([255, 255, 255, 255]);
                    if !paint(&mut canvas, &mut budget, c, x, y, data[i] - 0x3f, n) {
                        return None;
                    }
                    x = (x + n).min(side);
                    w = w.max(x);
                    i += 1;
                }
                continue;
            }
            b'$' => x = 0,
            b'-' => {
                x = 0;
                y += 6;
                if y >= side {
                    break; // taller than any image we accept: keep what we have
                }
            }
            0x3f..=0x7e => {
                let c = palette.get(color).copied().unwrap_or([255, 255, 255, 255]);
                if !paint(&mut canvas, &mut budget, c, x, y, data[i] - 0x3f, 1) {
                    return None;
                }
                x = (x + 1).min(side);
                w = w.max(x);
            }
            _ => {}
        }
        i += 1;
    }
    h = h.max(canvas.iter().rposition(|row| row.iter().any(Option::is_some)).map_or(0, |r| r + 1));
    let len = rgba_len(w as u32, h as u32)?;
    let mut rgba = vec![0u8; len];
    for (py, row) in canvas.iter().enumerate().take(h) {
        for (px, c) in row.iter().enumerate().take(w) {
            if let Some(c) = c {
                let o = (py * w + px) * 4;
                rgba[o..o + 4].copy_from_slice(c);
            }
        }
    }
    Some(Image::new(w as u32, h as u32, rgba))
}

/// Paint one sixel (six vertical pixels, `bits`) `count` times. Returns
/// `false` once the pixel budget is spent (the image is then rejected).
fn paint(canvas: &mut Vec<Vec<Option<[u8; 4]>>>, budget: &mut usize, c: [u8; 4], x: usize, y: usize, bits: u8, count: usize) -> bool {
    let side = MAX_IMAGE_SIDE as usize;
    let end = (x + count).min(side);
    for bit in 0..6 {
        let py = y + bit;
        if bits & (1 << bit) == 0 || py >= side || x >= end {
            continue;
        }
        if canvas.len() <= py {
            canvas.resize_with(py + 1, Vec::new);
        }
        let row = &mut canvas[py];
        if row.len() < end {
            let grow = end - row.len();
            if grow > *budget {
                return false;
            }
            *budget -= grow;
            row.resize(end, None);
        }
        row[x..end].fill(Some(c));
    }
    true
}

fn hls_to_rgb(h: usize, l: usize, s: usize) -> [u8; 3] {
    // Sixel HLS: hue 0-360 with blue at 0, lightness/saturation 0-100.
    let (h, l, s) = (((h + 240) % 360) as f32 / 360.0, l as f32 / 100.0, s as f32 / 100.0);
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |t: f32| {
        let t = t.rem_euclid(1.0);
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        (v * 255.0).round() as u8
    };
    [f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0)]
}

fn default_palette() -> Vec<[u8; 4]> {
    // The VT340's 16 colours.
    [
        [0, 0, 0],
        [51, 51, 204],
        [204, 36, 36],
        [51, 204, 51],
        [204, 51, 204],
        [51, 204, 204],
        [204, 204, 51],
        [120, 120, 120],
        [69, 69, 69],
        [87, 87, 153],
        [153, 69, 69],
        [87, 153, 87],
        [153, 87, 153],
        [87, 153, 153],
        [153, 153, 87],
        [204, 204, 204],
    ]
    .iter()
    .map(|c| [c[0], c[1], c[2], 255])
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> SharedImages {
        ImageStore::new(Metrics { cell_w: 10.0, cell_h: 20.0, cols: 80 })
    }

    fn tiny_png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(30, 50, image::Rgba([255, 0, 0, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn placeholders_round_trip() {
        let s = store();
        let bytes = s.lock().unwrap().place(Image::new(30, 50, vec![0; 30 * 50 * 4]), None, None);
        // 30x50 px at 10x20 cells = 3 cols x 3 rows.
        let text = String::from_utf8(bytes).unwrap();
        let marks: Vec<_> = text.chars().filter_map(decode_placeholder).collect();
        assert_eq!(marks, vec![(0, 0), (0, 1), (0, 2)]);
        assert!(text.ends_with("\x1b[2C"));
        let p = s.lock().unwrap().placement(0).cloned().unwrap();
        assert_eq!((p.cols, p.rows), (3, 3));
    }

    #[test]
    fn kitty_png_chunked_with_reply() {
        let s = store();
        let b64 = base64::engine::general_purpose::STANDARD.encode(tiny_png());
        let (a, b) = b64.split_at(b64.len() / 2);
        let mut k = KittyState::default();
        let first = k.handle(&s, format!("a=T,f=100,i=7,m=1;{a}").as_bytes());
        assert!(first.output.is_empty() && first.reply.is_none());
        let done = k.handle(&s, format!("m=0;{b}").as_bytes());
        assert!(!done.output.is_empty());
        assert_eq!(done.reply.unwrap(), b"\x1b_Gi=7;OK\x1b\\");
        // Re-place by id.
        let again = k.handle(&s, b"a=p,i=7,q=1");
        assert!(!again.output.is_empty() && again.reply.is_none());
    }

    #[test]
    fn kitty_query_and_raw_rgb() {
        let s = store();
        let mut k = KittyState::default();
        let rgb = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3]);
        let q = k.handle(&s, format!("a=q,i=31,s=1,v=1,f=24,t=d;{rgb}").as_bytes());
        assert_eq!(q.reply.unwrap(), b"\x1b_Gi=31;OK\x1b\\");
        assert!(q.output.is_empty());
    }

    #[test]
    fn iterm_inline() {
        let s = store();
        let body = format!("1337;File=name=eA==;inline=1;width=5:{}", base64::engine::general_purpose::STANDARD.encode(tiny_png()));
        let out = iterm_image(&s, body.as_bytes()).unwrap();
        assert!(!out.is_empty());
        assert_eq!(s.lock().unwrap().placement(0).unwrap().cols, 5);
    }

    #[test]
    fn sixel_decodes() {
        // Two colours, a 4px wide band: red top half, then green over it.
        let img = sixel_image(b"0;1;0q\"1;1;4;6#1;2;100;0;0#2;2;0;100;0#1!4F$#2!4w").unwrap();
        assert_eq!((img.width, img.height), (4, 6));
        assert_eq!(&img.rgba[0..4], &[255, 0, 0, 255]); // top row red
        assert_eq!(&img.rgba[(5 * 4) * 4..(5 * 4) * 4 + 4], &[0, 255, 0, 255]); // bottom row green
        assert!(sixel_image(b"+q544e").is_none()); // XTGETTCAP, not sixel
    }
}
