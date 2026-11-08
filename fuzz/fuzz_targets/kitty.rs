//! Kitty graphics, including chunked (m=1) transfers: each input is split on 0xFF
//! into consecutive APC bodies fed to one KittyState.
#![no_main]
use kraken_term::images::{ImageStore, KittyState, Metrics};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let store = ImageStore::new(Metrics { cell_w: 10.0, cell_h: 20.0, cols: 80 });
    let mut kitty = KittyState::default();
    for body in data.split(|&b| b == 0xFF) {
        // Never let fuzz input name local files.
        if body.windows(2).any(|w| w == b"t=") {
            continue;
        }
        let _ = kitty.handle(&store, body);
        assert!(kitty.pending_bytes() <= kraken_term::images::MAX_IMAGE_BYTES / 3 * 4 + 4);
    }
});
