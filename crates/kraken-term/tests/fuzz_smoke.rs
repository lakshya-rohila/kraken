//! Smoke fuzzing for everything that parses bytes a program (possibly on a
//! remote host, over ssh) writes to the terminal: the OSC/APC/DCS scanner,
//! Kitty graphics, sixel and iTerm2 images.
//!
//! Runs on stable in every `cargo test`: known-hostile inputs first, then a
//! few thousand deterministic random mutations of valid sequences. Each input
//! must finish quickly without panicking or allocating absurd amounts of
//! memory. Deeper coverage-guided fuzzing lives in `fuzz/` (cargo-fuzz).

use kraken_term::images::{self, ImageStore, KittyState, Metrics};
use kraken_term::shell::Rewriter;
use std::time::{Duration, Instant};

fn store() -> images::SharedImages {
    ImageStore::new(Metrics { cell_w: 10.0, cell_h: 20.0, cols: 80 })
}

/// Run one input through every parser; each must finish within `budget`.
fn exercise(input: &[u8], budget: Duration) {
    let start = Instant::now();
    let s = store();
    let mut rw = Rewriter::new(Some(s.clone()));
    let mut out = Vec::new();
    // Feed in uneven chunks: sequences split across reads must still work.
    for chunk in input.chunks(7) {
        rw.feed(chunk, &mut out, |_| {});
    }
    let _ = images::decode_sixel(input);
    let _ = images::sixel_image(input);
    let _ = images::iterm_image(&s, input);
    let mut kitty = KittyState::default();
    let _ = kitty.handle(&s, input);
    if let Some(img) = images::decode_sixel(input) {
        assert_eq!(img.rgba.len(), img.width as usize * img.height as usize * 4, "sixel buffer/size mismatch");
    }
    let took = start.elapsed();
    assert!(took < budget, "input took {took:?}: {:?}", String::from_utf8_lossy(&input[..input.len().min(80)]));
}

fn kitty_apc(ctrl: &str, payload: &[u8]) -> Vec<u8> {
    use base64::Engine;
    let mut v = format!("\x1b_G{ctrl};").into_bytes();
    v.extend_from_slice(base64::engine::general_purpose::STANDARD.encode(payload).as_bytes());
    v.extend_from_slice(b"\x1b\\");
    v
}

#[test]
fn hostile_inputs_are_rejected_quickly() {
    let zlib_bomb = {
        use std::io::Write;
        let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        e.write_all(&vec![0u8; 256 * 1024 * 1024]).unwrap();
        e.finish().unwrap()
    };
    let cases: Vec<Vec<u8>> = vec![
        // Sixel: enormous repeat count, colour register, raster size, overflow.
        b"\x1bPq!99999999999~\x1b\\".to_vec(),
        b"\x1bPq#4000000000;2;0;0;0~\x1b\\".to_vec(),
        b"\x1bPq\"1;1;4294967296;4294967296~\x1b\\".to_vec(),
        b"\x1bPq\"1;1;18446744073709551615;2~\x1b\\".to_vec(),
        b"\x1bPq#1;2;99999999999999;0;0~\x1b\\".to_vec(),
        b"\x1bPq!18446744073709551615~!18446744073709551615~\x1b\\".to_vec(),
        // Kitty: width*height overflowing u32 with a tiny buffer.
        kitty_apc("a=T,f=32,s=65536,v=65537", &[0; 16]),
        kitty_apc("a=T,f=24,s=4294967295,v=4294967295", &[0; 16]),
        // Kitty: reading a device that never ends, and a zlib bomb.
        kitty_apc("a=T,t=f,f=100", b"/dev/zero"),
        kitty_apc("a=T,f=32,s=8192,v=8192,o=z", &zlib_bomb),
        // iTerm2 with nonsense sizes.
        b"\x1b]1337;File=inline=1;width=999999999;height=-5:AAAA\x07".to_vec(),
    ];
    for c in &cases {
        exercise(c, Duration::from_secs(2));
    }
}

#[test]
fn kitty_temp_file_deletion_is_confined_to_temp_dirs() {
    let dir = std::env::temp_dir().join(format!("kraken-fuzz-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A file outside the temp folder whose name merely contains the magic word.
    let home_like = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("keep-tty-graphics-protocol.txt");
    std::fs::write(&home_like, b"important").unwrap();
    let s = store();
    let mut kitty = KittyState::default();
    let _ = kitty.handle(&s, &kitty_apc("a=T,t=t,f=100", home_like.to_string_lossy().as_bytes())[3..]);
    let in_temp = home_like.starts_with(std::env::temp_dir());
    assert!(in_temp || home_like.exists(), "t=t deleted a file outside the temp folder");
    let _ = std::fs::remove_file(&home_like);
}

#[test]
fn kitty_chunked_transfers_are_capped() {
    let s = store();
    let mut kitty = KittyState::default();
    let chunk = kitty_apc("a=T,f=32,s=1,v=1,m=1", &vec![0u8; 3 * 1024 * 1024]);
    let body = &chunk[3..chunk.len() - 2];
    let start = Instant::now();
    for _ in 0..200 {
        let _ = kitty.handle(&s, body); // ~600 MB if nothing stops it
    }
    assert!(kitty.pending_bytes() <= images::MAX_IMAGE_BYTES, "pending grew to {}", kitty.pending_bytes());
    assert!(start.elapsed() < Duration::from_secs(10));
}

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

#[test]
fn random_mutations_never_panic() {
    let seeds: Vec<Vec<u8>> = vec![
        b"\x1bPq#0;2;100;0;0#0!10~-#1;1;120;50;100~~$~\x1b\\".to_vec(),
        b"\x1bP0;1;0q\"1;1;20;12#2!20@\x1b\\".to_vec(),
        kitty_apc("a=T,f=24,s=2,v=2", &[255; 12]),
        kitty_apc("a=T,f=32,s=1,v=1,i=7,q=2", &[1, 2, 3, 4]),
        kitty_apc("a=p,i=7,c=3,r=2", b""),
        b"\x1b]1337;File=inline=1;width=2;height=1:iVBORw0KGgo=\x07".to_vec(),
        b"\x1b]7;file://host/Users/me/dir\x07\x1b]133;A\x07$ \x1b]133;B\x07ls\x1b]133;C\x07\x1b]133;D;0\x07".to_vec(),
        b"\x1b]9;hello\x07\x1b]777;notify;t;b\x07\x1b]8;;https://x.y\x07link\x1b]8;;\x07".to_vec(),
    ];
    let alphabet = b"\x1b\\\x07;=,:#!\"$-?~@0123456789qGPm_]";
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..4000 {
        let mut input = seeds[rng.below(seeds.len())].clone();
        for _ in 0..1 + rng.below(8) {
            let pos = rng.below(input.len() + 1);
            match rng.below(5) {
                0 if !input.is_empty() => {
                    input.remove(pos.min(input.len() - 1));
                }
                1 => input.insert(pos, alphabet[rng.below(alphabet.len())]),
                2 => input.insert(pos, rng.next() as u8),
                3 => {
                    // Splice in a long run of digits: huge numbers everywhere.
                    for _ in 0..1 + rng.below(25) {
                        input.insert(pos, b'0' + rng.below(10) as u8);
                    }
                }
                _ => {
                    let other = &seeds[rng.below(seeds.len())];
                    let a = rng.below(other.len());
                    let b = a + rng.below(other.len() - a + 1);
                    input.splice(pos..pos, other[a..b].iter().copied());
                }
            }
        }
        exercise(&input, Duration::from_secs(2));
    }
}
