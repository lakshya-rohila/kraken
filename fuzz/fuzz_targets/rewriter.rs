//! Everything a program writes passes through the Rewriter: OSC 7/8/9/133/777/1337,
//! Kitty APC and sixel DCS dispatch. Split points vary so chunk boundaries get fuzzed too.
#![no_main]
use kraken_term::images::{ImageStore, Metrics};
use kraken_term::shell::Rewriter;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&split, rest)) = data.split_first() else { return };
    let store = ImageStore::new(Metrics { cell_w: 10.0, cell_h: 20.0, cols: 80 });
    let mut rw = Rewriter::new(Some(store));
    let mut out = Vec::new();
    for chunk in rest.chunks(split.max(1) as usize) {
        rw.feed(chunk, &mut out, |_| {});
    }
});
