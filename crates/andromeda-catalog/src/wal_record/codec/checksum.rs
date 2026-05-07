//! Stable checksum calculation for catalog WAL payloads.

use super::super::constants::CATALOG_WAL_PAYLOAD_MAGIC;

pub(super) fn catalog_wal_payload_checksum(
    version: u16,
    kind_tag: u16,
    body_len: u64,
    body: &[u8],
) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    fn fold_bytes(state: &mut u64, bytes: &[u8]) {
        for byte in bytes {
            *state ^= u64::from(*byte);
            *state = state.wrapping_mul(FNV_PRIME);
        }
    }

    let mut state = FNV_OFFSET;
    fold_bytes(&mut state, &CATALOG_WAL_PAYLOAD_MAGIC.to_le_bytes());
    fold_bytes(&mut state, &version.to_le_bytes());
    fold_bytes(&mut state, &kind_tag.to_le_bytes());
    fold_bytes(&mut state, &body_len.to_le_bytes());
    fold_bytes(&mut state, body);
    if state == 0 { 1 } else { state }
}
