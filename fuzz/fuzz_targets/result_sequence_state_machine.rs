#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut state = 0u8;
    for byte in data {
        state = state.wrapping_add(*byte);
        let _terminal = state == u8::MAX;
    }
});
