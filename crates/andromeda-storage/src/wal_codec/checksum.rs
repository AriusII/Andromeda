use super::{
    WAL_RECORD_HEADER_LEN,
    binary::{push_u16, push_u64},
    frame::WalFrameHeader,
};

pub(super) fn header_checksum_without_checksum(header: &WalFrameHeader) -> u64 {
    let mut bytes = Vec::with_capacity(WAL_RECORD_HEADER_LEN - 8);
    push_u64(&mut bytes, header.magic);
    push_u16(&mut bytes, header.format_version);
    push_u16(&mut bytes, header.header_length);
    push_u64(&mut bytes, header.total_length);
    push_u16(&mut bytes, header.kind_tag);
    push_u16(&mut bytes, header.flags);
    push_u64(&mut bytes, header.lsn.get());
    push_u64(&mut bytes, header.previous_lsn.map_or(0, crate::Lsn::get));
    push_u64(
        &mut bytes,
        header
            .transaction_id
            .map_or(0, andromeda_core::TransactionId::get),
    );
    push_u64(&mut bytes, header.payload_length);
    push_u64(&mut bytes, header.record_checksum);
    fnv64_nonzero(&bytes)
}

fn fnv64_nonzero(bytes: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut state = FNV_OFFSET;
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV_PRIME);
    }
    if state == 0 { 1 } else { state }
}
