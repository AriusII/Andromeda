use andromeda_core::AndromedaResult;
use sha2::{Digest, Sha256};

use crate::{PageHeader, PageTrailer};

use super::error::storage_error;

pub fn payload_crc64(payload: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut state = FNV_OFFSET;
    for byte in payload {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(FNV_PRIME);
    }
    if state == 0 { 1 } else { state }
}

pub fn payload_hash(payload: &[u8]) -> [u8; 32] {
    let hash: [u8; 32] = Sha256::digest(payload).into();
    if hash == [0; 32] { [1; 32] } else { hash }
}

pub fn torn_write_guard(header: &PageHeader, payload_crc64: u64) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut state = FNV_OFFSET;
    for value in [
        u64::from(header.magic),
        u64::from(header.format_version),
        header.page_id.get(),
        header.object_id.get(),
        header.allocation_id.get(),
        header.page_lsn.get(),
        header.page_epoch,
        payload_crc64,
        u64::from(header.payload_offset),
        u64::from(header.payload_len),
    ] {
        for byte in value.to_le_bytes() {
            state ^= u64::from(byte);
            state = state.wrapping_mul(FNV_PRIME);
        }
    }

    if state == 0 || state == header.page_id.get() {
        state ^ 0xA9D3_78B5_4C2F_6101
    } else {
        state
    }
}

pub fn integrity_trailer_for_payload(header: &PageHeader, payload: &[u8]) -> PageTrailer {
    let payload_crc64 = payload_crc64(payload);
    PageTrailer {
        payload_crc64,
        page_hash: payload_hash(payload),
        torn_write_guard: torn_write_guard(header, payload_crc64),
    }
}

pub fn validate_payload_integrity(
    header: &PageHeader,
    payload: &[u8],
    trailer: &PageTrailer,
) -> AndromedaResult<()> {
    let expected = integrity_trailer_for_payload(header, payload);
    if trailer.payload_crc64 != expected.payload_crc64 {
        return Err(storage_error("page payload CRC mismatch"));
    }
    if trailer.page_hash != expected.page_hash {
        return Err(storage_error("page payload hash mismatch"));
    }
    if trailer.torn_write_guard != expected.torn_write_guard {
        return Err(storage_error("page torn-write guard mismatch"));
    }
    Ok(())
}
