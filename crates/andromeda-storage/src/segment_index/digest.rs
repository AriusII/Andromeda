use std::ops::Range;

use sha2::{Digest, Sha256};

pub(super) fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub(super) fn crc32_iso_hdlc(bytes: &[u8], zeroed: Range<usize>) -> u32 {
    const POLY: u32 = 0xEDB8_8320;

    let mut state = 0xFFFF_FFFF;
    for (idx, byte) in bytes.iter().enumerate() {
        let byte = if zeroed.contains(&idx) { 0 } else { *byte };
        state ^= u32::from(byte);
        for _ in 0..8 {
            state = if state & 1 == 1 {
                (state >> 1) ^ POLY
            } else {
                state >> 1
            };
        }
    }

    normalize_u32(!state)
}

pub(super) fn crc64_ecma(bytes: &[u8]) -> u64 {
    const POLY: u64 = 0x42F0_E1EB_A9EA_3693;

    let mut state = 0_u64;
    for byte in bytes {
        state ^= u64::from(*byte) << 56;
        for _ in 0..8 {
            state = if state & 0x8000_0000_0000_0000 != 0 {
                (state << 1) ^ POLY
            } else {
                state << 1
            };
        }
    }

    normalize_u64(state)
}

fn normalize_u32(value: u32) -> u32 {
    if value == 0 { 1 } else { value }
}

fn normalize_u64(value: u64) -> u64 {
    if value == 0 { 1 } else { value }
}
