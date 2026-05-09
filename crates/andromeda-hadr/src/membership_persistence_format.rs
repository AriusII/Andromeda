use std::{fs, path::Path};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use sha2::{Digest, Sha256};

use crate::membership_store::HadrMembershipSnapshot;

mod cursor;
mod payload;
mod record;

const FORMAT_VERSION_V1: u64 = 1;
const FORMAT_VERSION_V2: u64 = 2;
const FILE_MAGIC: &[u8; 16] = b"ANDHADR-MSTORE\0\0";
const CHECKSUM_LEN: usize = 32;
const PAYLOAD_LEN_FIELD: usize = 8;
const HEADER_LEN: usize = FILE_MAGIC.len() + CHECKSUM_LEN + PAYLOAD_LEN_FIELD;
const PAYLOAD_V1_HEADER_LEN: usize = 24;
const PAYLOAD_V2_HEADER_LEN: usize = 32;
const NODE_ENTRY_LEN: usize = 24;
const RECORD_ENTRY_LEN: usize = 32;
const HADR_MEMBERSHIP_MAX_FILE_BYTES: u64 = 1024 * 1024;

pub(super) fn encode_snapshot(snapshot: &HadrMembershipSnapshot) -> AndromedaResult<Vec<u8>> {
    let payload = payload::encode_payload(snapshot)?;
    let checksum = Sha256::digest(&payload);
    let capacity = HEADER_LEN
        .checked_add(payload.len())
        .ok_or_else(|| storage_error("HADR membership file length overflow"))?;
    let mut encoded = Vec::with_capacity(capacity);
    encoded.extend_from_slice(FILE_MAGIC);
    encoded.extend_from_slice(&checksum);
    let payload_len = u64::try_from(payload.len())
        .map_err(|_| storage_error("HADR membership payload length exceeds u64"))?;
    encoded.extend_from_slice(&payload_len.to_le_bytes());
    encoded.extend_from_slice(&payload);
    Ok(encoded)
}

pub(super) fn decode_snapshot(bytes: &[u8]) -> AndromedaResult<HadrMembershipSnapshot> {
    if bytes.len() < HEADER_LEN {
        return Err(storage_error("HADR membership file is truncated"));
    }
    if &bytes[..FILE_MAGIC.len()] != FILE_MAGIC {
        return Err(storage_error("HADR membership file magic mismatch"));
    }

    let checksum_start = FILE_MAGIC.len();
    let checksum_end = checksum_start + CHECKSUM_LEN;
    let payload_len_start = checksum_end;
    let payload_len_end = payload_len_start + PAYLOAD_LEN_FIELD;
    let payload_len = usize::try_from(read_u64(&bytes[payload_len_start..payload_len_end])?)
        .map_err(|_| storage_error("HADR membership payload length exceeds usize"))?;
    payload::validate_payload_len(payload_len)?;
    let expected_len = HEADER_LEN
        .checked_add(payload_len)
        .ok_or_else(|| storage_error("HADR membership file length overflow"))?;
    if bytes.len() != expected_len {
        return Err(storage_error("HADR membership file length mismatch"));
    }

    let payload = &bytes[HEADER_LEN..];
    let computed = Sha256::digest(payload);
    let computed_checksum: &[u8] = computed.as_ref();
    if computed_checksum != &bytes[checksum_start..checksum_end] {
        return Err(storage_error("HADR membership file checksum mismatch"));
    }

    payload::decode_payload(payload)
}

pub(super) fn read_membership_file(path: &Path) -> AndromedaResult<Vec<u8>> {
    let metadata = fs::metadata(path).map_err(|err| io_error("stat HADR membership file", err))?;
    if metadata.len() > HADR_MEMBERSHIP_MAX_FILE_BYTES {
        return Err(storage_error(
            "HADR membership file exceeds bounded read limit",
        ));
    }
    fs::read(path).map_err(|err| io_error("read HADR membership file", err))
}

fn read_u64(bytes: &[u8]) -> AndromedaResult<u64> {
    let array: [u8; 8] = bytes
        .try_into()
        .map_err(|_| storage_error("HADR membership u64 field is truncated"))?;
    Ok(u64::from_le_bytes(array))
}

fn membership_payload_capacity(node_count: usize, record_count: usize) -> AndromedaResult<usize> {
    PAYLOAD_V2_HEADER_LEN
        .checked_add(
            node_count
                .checked_mul(NODE_ENTRY_LEN)
                .ok_or_else(|| storage_error("HADR membership node count overflow"))?,
        )
        .and_then(|len| {
            record_count
                .checked_mul(RECORD_ENTRY_LEN)
                .and_then(|records_len| len.checked_add(records_len))
        })
        .ok_or_else(|| storage_error("HADR membership payload length overflow"))
}

fn io_error(action: &str, err: std::io::Error) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, format!("{action}: {err}"))
}

fn storage_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}
