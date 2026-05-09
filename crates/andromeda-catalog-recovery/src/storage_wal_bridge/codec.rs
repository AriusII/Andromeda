use andromeda_error::AndromedaResult;
use std::io::{Read, Write};

use crate::{CatalogStorageWalRecord, CatalogStorageWalRecordVersion};

use super::error::{catalog_wal_error, io_error};
use super::format_evidence::{
    FRAME_HEADER_BYTES, checked_frame_len, format_checksum, payload_checksum,
};
use super::record_parsing::{decode_record_payload, encode_record_payload};

/// Encodes a `CatalogWalRecord` to a deterministic binary format.
///
/// # Contract
///
/// - The encoding is **deterministic**: the same record always produces the same bytes.
/// - The encoding includes a **SHA256 checksum** for integrity validation.
/// - The encoding is **LSN-addressable**: the returned bytes can be treated as a
///   complete record unit.
/// - Round-trip encode/decode is **symmetric**: `decode(encode(r)) == r` (or error).
///
/// # Format
///
/// ```text
/// [version:u16][payload_len:u32][checksum:32][payload:N]
/// ```
pub fn encode_storage_catalog_record(record: &CatalogStorageWalRecord) -> AndromedaResult<Vec<u8>> {
    // Validate the record first
    record.validate()?;

    // Encode the payload deterministically
    let mut payload = Vec::new();
    encode_record_payload(&mut payload, record)?;

    // Compute SHA256 checksum of payload
    let checksum = payload_checksum(&payload);

    // Build the full frame: version + payload_len + checksum + payload
    let mut frame = Vec::new();
    frame
        .write_all(
            &CatalogStorageWalRecordVersion::CURRENT
                .as_u16()
                .to_le_bytes(),
        )
        .map_err(io_error)?;
    frame
        .write_all(&(payload.len() as u32).to_le_bytes())
        .map_err(io_error)?;
    frame.write_all(&checksum).map_err(io_error)?;
    frame.write_all(&payload).map_err(io_error)?;

    Ok(frame)
}

/// Decodes a binary record to a `CatalogWalRecord`.
///
/// # Contract
///
/// - Validates the **SHA256 checksum** against the payload.
/// - Returns error if checksum fails.
/// - Returns error if the record version is unsupported.
/// - The decoded record is re-validated against invariants.
pub fn decode_storage_catalog_record(bytes: &[u8]) -> AndromedaResult<CatalogStorageWalRecord> {
    if bytes.len() < FRAME_HEADER_BYTES {
        // Minimum: 2 (version) + 4 (len) + 32 (checksum)
        return Err(catalog_wal_error(format!(
            "catalog record frame too short: {} bytes (need at least 38)",
            bytes.len()
        )));
    }

    let mut cursor = std::io::Cursor::new(bytes);

    // Read version (u16)
    let mut version_bytes = [0u8; 2];
    cursor.read_exact(&mut version_bytes).map_err(io_error)?;
    let version = u16::from_le_bytes(version_bytes);
    let record_version = CatalogStorageWalRecordVersion::try_from_u16(version)?;

    // Read payload length (u32)
    let mut len_bytes = [0u8; 4];
    cursor.read_exact(&mut len_bytes).map_err(io_error)?;
    let payload_len = u32::from_le_bytes(len_bytes) as usize;

    let expected_frame_len = checked_frame_len(payload_len)
        .ok_or_else(|| catalog_wal_error("catalog record payload length overflows frame size"))?;
    if bytes.len() != expected_frame_len {
        return Err(catalog_wal_error(format!(
            "catalog record frame length mismatch: header declares {} payload bytes, frame has {} bytes",
            payload_len,
            bytes.len()
        )));
    }

    // Read checksum (32 bytes)
    let mut expected_checksum = [0u8; 32];
    cursor
        .read_exact(&mut expected_checksum)
        .map_err(io_error)?;

    // Read payload
    let mut payload = vec![0u8; payload_len];
    cursor.read_exact(&mut payload).map_err(io_error)?;

    // Verify checksum
    let computed_checksum = payload_checksum(&payload);

    if computed_checksum != expected_checksum {
        return Err(catalog_wal_error(format!(
            "catalog record checksum mismatch: expected {}, got {}",
            format_checksum(&expected_checksum),
            format_checksum(&computed_checksum)
        )));
    }

    // Decode the record from payload
    let record = decode_record_payload(&payload, record_version)?;

    // Validate the record
    record.validate()?;

    Ok(record)
}
