#![no_main]

use libfuzzer_sys::fuzz_target;

const MAX_SRPL_FUZZ_BYTES: usize = 16 * 1024;

fuzz_target!(|data: &[u8]| {
    let data = &data[..data.len().min(MAX_SRPL_FUZZ_BYTES)];
    if let Ok(input) = core::str::from_utf8(data) {
        let _ = andromeda_srpl::parse_procedure_signature(input);
    }
});
