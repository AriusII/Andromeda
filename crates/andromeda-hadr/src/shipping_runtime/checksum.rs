use crate::write_ahead_log::WalRecord;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

pub(super) fn compute_segment_checksum(records: &[WalRecord]) -> u64 {
    let mut state = FNV_OFFSET;

    for record in records {
        state = fold_fnv1a_u64(state, record.header.kind as u64);
        state = fold_fnv1a_u64(state, record.header.lsn.get());
        state = fold_fnv1a_bytes(state, &record.payload);
    }

    if state == 0 { 1 } else { state }
}

fn fold_fnv1a_u64(state: u64, value: u64) -> u64 {
    fold_fnv1a_bytes(state, &value.to_le_bytes())
}

fn fold_fnv1a_bytes(mut state: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV_PRIME);
    }
    state
}
