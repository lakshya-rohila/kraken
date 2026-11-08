#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Some(img) = kraken_term::images::decode_sixel(data) {
        assert_eq!(img.rgba.len(), img.width as usize * img.height as usize * 4);
    }
});
