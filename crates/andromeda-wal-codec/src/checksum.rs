use crate::{
    binary::{push_u16, push_u64},
    header::{WAL_RECORD_HEADER_LEN, WalCodecFrameHeader},
};

pub(crate) fn header_checksum_without_checksum(header: &WalCodecFrameHeader) -> u64 {
    let mut bytes = Vec::with_capacity(WAL_RECORD_HEADER_LEN - 8);
    push_u64(&mut bytes, header.magic);
    push_u16(&mut bytes, header.format_version);
    push_u16(&mut bytes, header.header_length);
    push_u64(&mut bytes, header.total_length);
    push_u16(&mut bytes, header.kind_tag);
    push_u16(&mut bytes, header.flags);
    push_u64(&mut bytes, header.lsn);
    push_u64(&mut bytes, header.previous_lsn.unwrap_or(0));
    push_u64(&mut bytes, header.transaction_id.unwrap_or(0));
    push_u64(&mut bytes, header.payload_length);
    push_u64(&mut bytes, header.record_checksum);
    fnv64_nonzero(&bytes)
}

/// FNV-64a with nonzero guard for WAL frame header checksums.
///
/// This is FNV-1a (XOR-then-multiply variant) over 64 bits with the standard
/// parameters (offset-basis = 0xcbf2_9ce4_8422_2325, prime = 0x0000_0100_0000_01b3).
/// The only deviation from bare FNV-64a is the nonzero guard: if the FNV-64a
/// digest of the input is 0, the function returns 1 instead.
///
/// **Durable byte contract.** This function is used to compute the header
/// checksum of WAL record frames persisted to durable storage.  Any change
/// to this function is a breaking binary-format change and requires a
/// Doctrine review.  Golden-vector tests in
/// `andromeda-wal-codec/tests/fnv64_nonzero_golden_vectors.rs` are the
/// primary regression line.
pub fn fnv64_nonzero(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut state = FNV_OFFSET;
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV_PRIME);
    }
    if state == 0 { 1 } else { state }
}
