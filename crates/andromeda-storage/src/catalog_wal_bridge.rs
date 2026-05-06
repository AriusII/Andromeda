//! Bridge for encoding and decoding catalog WAL records at the storage boundary.
//!
//! This module owns:
//! - `encode_catalog_record`: Deterministic binary encoding to `Vec<u8>`
//! - `decode_catalog_record`: Binary decoding with validation
//! - Checksum validation (SHA256 digest of payload)
//! - LSN-addressable record boundaries
//!
//! `wal_record_catalog` owns the semantic record types and invariants. This
//! module owns the durable byte representation used by storage WAL replay.
//!
//! ## Encoding Format
//!
//! Records are encoded as a deterministic binary format using a simple length-prefixed
//! encoding scheme:
//!
//! ```text
//! [version:u16][payload_len:u32][checksum:32 bytes][payload:N bytes]
//! ```
//!
//! The checksum is computed over the payload only (not the version/length prefix).
//! This ensures round-trip encode/decode symmetry.

use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogObjectId, CatalogVersion,
    ContractHash,
};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

use crate::Lsn;
use crate::wal_record_catalog::{CatalogWalRecord, CatalogWalRecordVersion};

/// Helper to convert io::Error to AndromedaError for codec operations.
fn io_error(e: std::io::Error) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Storage,
        format!("catalog WAL codec I/O error: {}", e),
    )
}

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
pub fn encode_catalog_record(record: &CatalogWalRecord) -> AndromedaResult<Vec<u8>> {
    // Validate the record first
    record.validate()?;

    // Encode the payload deterministically
    let mut payload = Vec::new();
    encode_record_payload(&mut payload, record)?;

    // Compute SHA256 checksum of payload
    let mut hasher = Sha256::new();
    hasher.update(&payload);
    let checksum: [u8; 32] = hasher.finalize().into();

    // Build the full frame: version + payload_len + checksum + payload
    let mut frame = Vec::new();
    frame
        .write_all(&CatalogWalRecordVersion::CURRENT.as_u16().to_le_bytes())
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
pub fn decode_catalog_record(bytes: &[u8]) -> AndromedaResult<CatalogWalRecord> {
    if bytes.len() < 6 + 32 {
        // Minimum: 2 (version) + 4 (len) + 32 (checksum)
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "catalog record frame too short: {} bytes (need at least 38)",
                bytes.len()
            ),
        ));
    }

    let mut cursor = std::io::Cursor::new(bytes);

    // Read version (u16)
    let mut version_bytes = [0u8; 2];
    cursor.read_exact(&mut version_bytes).map_err(io_error)?;
    let version = u16::from_le_bytes(version_bytes);
    let record_version = CatalogWalRecordVersion::try_from_u16(version)?;

    // Read payload length (u32)
    let mut len_bytes = [0u8; 4];
    cursor.read_exact(&mut len_bytes).map_err(io_error)?;
    let payload_len = u32::from_le_bytes(len_bytes) as usize;

    // Read checksum (32 bytes)
    let mut expected_checksum = [0u8; 32];
    cursor
        .read_exact(&mut expected_checksum)
        .map_err(io_error)?;

    // Read payload
    let mut payload = vec![0u8; payload_len];
    cursor.read_exact(&mut payload).map_err(io_error)?;

    // Verify checksum
    let mut hasher = Sha256::new();
    hasher.update(&payload);
    let computed_checksum: [u8; 32] = hasher.finalize().into();

    if computed_checksum != expected_checksum {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!(
                "catalog record checksum mismatch: expected {}, got {}",
                format_checksum(&expected_checksum),
                format_checksum(&computed_checksum)
            ),
        ));
    }

    // Decode the record from payload
    let record = decode_record_payload(&payload, record_version)?;

    // Validate the record
    record.validate()?;

    Ok(record)
}

/// Helper: encode record payload to writer in deterministic order.
fn encode_record_payload(writer: &mut dyn Write, record: &CatalogWalRecord) -> AndromedaResult<()> {
    // Collect bytes first (Vec::write never fails), then write all at once
    let mut bytes = Vec::new();

    match record {
        CatalogWalRecord::DefinitionBatchApplied {
            batch_id,
            new_catalog_version,
            procedure_count,
            affected_procedure_ids,
            timestamp_secs,
            operator_principal,
        } => {
            bytes.push(0u8); // Tag
            bytes.extend_from_slice(&batch_id.to_le_bytes());
            bytes.extend_from_slice(&new_catalog_version.get().to_le_bytes());
            bytes.extend_from_slice(&(*procedure_count as u32).to_le_bytes());
            for pid in affected_procedure_ids {
                bytes.extend_from_slice(&pid.get().to_le_bytes());
            }
            bytes.extend_from_slice(&timestamp_secs.to_le_bytes());
            let principal_bytes = operator_principal.as_bytes();
            bytes.extend_from_slice(&(principal_bytes.len() as u32).to_le_bytes());
            bytes.extend_from_slice(principal_bytes);
        }
        CatalogWalRecord::ProcedureAdded {
            procedure_id,
            signature_hash,
            new_catalog_version,
            timestamp_secs,
        } => {
            bytes.push(1u8); // Tag
            bytes.extend_from_slice(&procedure_id.get().to_le_bytes());
            bytes.extend_from_slice(&signature_hash.as_bytes());
            bytes.extend_from_slice(&new_catalog_version.get().to_le_bytes());
            bytes.extend_from_slice(&timestamp_secs.to_le_bytes());
        }
        CatalogWalRecord::ProcedureAltered {
            procedure_id,
            old_hash,
            new_hash,
            new_catalog_version,
            timestamp_secs,
        } => {
            bytes.push(2u8); // Tag
            bytes.extend_from_slice(&procedure_id.get().to_le_bytes());
            bytes.extend_from_slice(&old_hash.as_bytes());
            bytes.extend_from_slice(&new_hash.as_bytes());
            bytes.extend_from_slice(&new_catalog_version.get().to_le_bytes());
            bytes.extend_from_slice(&timestamp_secs.to_le_bytes());
        }
        CatalogWalRecord::ProcedureDropped {
            procedure_id,
            dropped_version,
            new_catalog_version,
            timestamp_secs,
        } => {
            bytes.push(3u8); // Tag
            bytes.extend_from_slice(&procedure_id.get().to_le_bytes());
            bytes.extend_from_slice(&dropped_version.get().to_le_bytes());
            bytes.extend_from_slice(&new_catalog_version.get().to_le_bytes());
            bytes.extend_from_slice(&timestamp_secs.to_le_bytes());
        }
        CatalogWalRecord::StatisticsUpdated {
            stats_version,
            table_id,
            column_id,
            histogram_data_lsn,
            timestamp_secs,
        } => {
            bytes.push(4u8); // Tag
            bytes.extend_from_slice(&stats_version.to_le_bytes());
            bytes.extend_from_slice(&table_id.get().to_le_bytes());
            bytes.extend_from_slice(&column_id.get().to_le_bytes());
            bytes.extend_from_slice(&histogram_data_lsn.get().to_le_bytes());
            bytes.extend_from_slice(&timestamp_secs.to_le_bytes());
        }
        CatalogWalRecord::CatalogCheckpoint {
            checkpoint_lsn,
            catalog_version,
            visible_procedure_count,
            timestamp_secs,
        } => {
            bytes.push(5u8); // Tag
            bytes.extend_from_slice(&checkpoint_lsn.get().to_le_bytes());
            bytes.extend_from_slice(&catalog_version.get().to_le_bytes());
            bytes.extend_from_slice(&(*visible_procedure_count as u32).to_le_bytes());
            bytes.extend_from_slice(&timestamp_secs.to_le_bytes());
        }
    }

    // Now write the collected bytes (convert io::Error to AndromedaError)
    writer.write_all(&bytes).map_err(io_error)?;
    Ok(())
}
/// Helper: decode record payload from reader in deterministic order.
fn decode_record_payload(
    bytes: &[u8],
    _version: CatalogWalRecordVersion,
) -> AndromedaResult<CatalogWalRecord> {
    let mut cursor = std::io::Cursor::new(bytes);

    // Read tag (first byte)
    let mut tag_byte = [0u8; 1];
    cursor.read_exact(&mut tag_byte).map_err(io_error)?;
    let tag = tag_byte[0];

    match tag {
        0 => {
            // DefinitionBatchApplied
            let mut batch_id_bytes = [0u8; 8];
            cursor.read_exact(&mut batch_id_bytes).map_err(io_error)?;
            let batch_id = u64::from_le_bytes(batch_id_bytes);

            let mut version_bytes = [0u8; 8];
            cursor.read_exact(&mut version_bytes).map_err(io_error)?;
            let new_catalog_version = CatalogVersion::new(u64::from_le_bytes(version_bytes));

            let mut count_bytes = [0u8; 4];
            cursor.read_exact(&mut count_bytes).map_err(io_error)?;
            let procedure_count = u32::from_le_bytes(count_bytes) as usize;

            let mut affected_procedure_ids = Vec::with_capacity(procedure_count);
            for _ in 0..procedure_count {
                let mut pid_bytes = [0u8; 8];
                cursor.read_exact(&mut pid_bytes).map_err(io_error)?;
                affected_procedure_ids.push(CatalogObjectId::new(u64::from_le_bytes(pid_bytes)));
            }

            let mut ts_bytes = [0u8; 8];
            cursor.read_exact(&mut ts_bytes).map_err(io_error)?;
            let timestamp_secs = u64::from_le_bytes(ts_bytes);

            let mut principal_len_bytes = [0u8; 4];
            cursor
                .read_exact(&mut principal_len_bytes)
                .map_err(io_error)?;
            let principal_len = u32::from_le_bytes(principal_len_bytes) as usize;

            let mut principal_bytes = vec![0u8; principal_len];
            cursor.read_exact(&mut principal_bytes).map_err(io_error)?;
            let operator_principal = String::from_utf8(principal_bytes).map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!("operator_principal is not valid UTF-8: {}", e),
                )
            })?;

            Ok(CatalogWalRecord::DefinitionBatchApplied {
                batch_id,
                new_catalog_version,
                procedure_count,
                affected_procedure_ids,
                timestamp_secs,
                operator_principal,
            })
        }
        1 => {
            // ProcedureAdded
            let mut pid_bytes = [0u8; 8];
            cursor.read_exact(&mut pid_bytes).map_err(io_error)?;
            let procedure_id = CatalogObjectId::new(u64::from_le_bytes(pid_bytes));

            let mut hash_bytes = [0u8; 32];
            cursor.read_exact(&mut hash_bytes).map_err(io_error)?;
            let signature_hash = ContractHash::new(hash_bytes);

            let mut version_bytes = [0u8; 8];
            cursor.read_exact(&mut version_bytes).map_err(io_error)?;
            let new_catalog_version = CatalogVersion::new(u64::from_le_bytes(version_bytes));

            let mut ts_bytes = [0u8; 8];
            cursor.read_exact(&mut ts_bytes).map_err(io_error)?;
            let timestamp_secs = u64::from_le_bytes(ts_bytes);

            Ok(CatalogWalRecord::ProcedureAdded {
                procedure_id,
                signature_hash,
                new_catalog_version,
                timestamp_secs,
            })
        }
        2 => {
            // ProcedureAltered
            let mut pid_bytes = [0u8; 8];
            cursor.read_exact(&mut pid_bytes).map_err(io_error)?;
            let procedure_id = CatalogObjectId::new(u64::from_le_bytes(pid_bytes));

            let mut old_hash_bytes = [0u8; 32];
            cursor.read_exact(&mut old_hash_bytes).map_err(io_error)?;
            let old_hash = ContractHash::new(old_hash_bytes);

            let mut new_hash_bytes = [0u8; 32];
            cursor.read_exact(&mut new_hash_bytes).map_err(io_error)?;
            let new_hash = ContractHash::new(new_hash_bytes);

            let mut version_bytes = [0u8; 8];
            cursor.read_exact(&mut version_bytes).map_err(io_error)?;
            let new_catalog_version = CatalogVersion::new(u64::from_le_bytes(version_bytes));

            let mut ts_bytes = [0u8; 8];
            cursor.read_exact(&mut ts_bytes).map_err(io_error)?;
            let timestamp_secs = u64::from_le_bytes(ts_bytes);

            Ok(CatalogWalRecord::ProcedureAltered {
                procedure_id,
                old_hash,
                new_hash,
                new_catalog_version,
                timestamp_secs,
            })
        }
        3 => {
            // ProcedureDropped
            let mut pid_bytes = [0u8; 8];
            cursor.read_exact(&mut pid_bytes).map_err(io_error)?;
            let procedure_id = CatalogObjectId::new(u64::from_le_bytes(pid_bytes));

            let mut dropped_ver_bytes = [0u8; 8];
            cursor
                .read_exact(&mut dropped_ver_bytes)
                .map_err(io_error)?;
            let dropped_version = CatalogVersion::new(u64::from_le_bytes(dropped_ver_bytes));

            let mut new_ver_bytes = [0u8; 8];
            cursor.read_exact(&mut new_ver_bytes).map_err(io_error)?;
            let new_catalog_version = CatalogVersion::new(u64::from_le_bytes(new_ver_bytes));

            let mut ts_bytes = [0u8; 8];
            cursor.read_exact(&mut ts_bytes).map_err(io_error)?;
            let timestamp_secs = u64::from_le_bytes(ts_bytes);

            Ok(CatalogWalRecord::ProcedureDropped {
                procedure_id,
                dropped_version,
                new_catalog_version,
                timestamp_secs,
            })
        }
        4 => {
            // StatisticsUpdated
            let mut stats_ver_bytes = [0u8; 8];
            cursor.read_exact(&mut stats_ver_bytes).map_err(io_error)?;
            let stats_version = u64::from_le_bytes(stats_ver_bytes);

            let mut table_id_bytes = [0u8; 8];
            cursor.read_exact(&mut table_id_bytes).map_err(io_error)?;
            let table_id = CatalogObjectId::new(u64::from_le_bytes(table_id_bytes));

            let mut col_id_bytes = [0u8; 8];
            cursor.read_exact(&mut col_id_bytes).map_err(io_error)?;
            let column_id = CatalogObjectId::new(u64::from_le_bytes(col_id_bytes));

            let mut lsn_bytes = [0u8; 8];
            cursor.read_exact(&mut lsn_bytes).map_err(io_error)?;
            let histogram_data_lsn = Lsn::new(u64::from_le_bytes(lsn_bytes));

            let mut ts_bytes = [0u8; 8];
            cursor.read_exact(&mut ts_bytes).map_err(io_error)?;
            let timestamp_secs = u64::from_le_bytes(ts_bytes);

            Ok(CatalogWalRecord::StatisticsUpdated {
                stats_version,
                table_id,
                column_id,
                histogram_data_lsn,
                timestamp_secs,
            })
        }
        5 => {
            // CatalogCheckpoint
            let mut lsn_bytes = [0u8; 8];
            cursor.read_exact(&mut lsn_bytes).map_err(io_error)?;
            let checkpoint_lsn = Lsn::new(u64::from_le_bytes(lsn_bytes));

            let mut version_bytes = [0u8; 8];
            cursor.read_exact(&mut version_bytes).map_err(io_error)?;
            let catalog_version = CatalogVersion::new(u64::from_le_bytes(version_bytes));

            let mut count_bytes = [0u8; 4];
            cursor.read_exact(&mut count_bytes).map_err(io_error)?;
            let visible_procedure_count = u32::from_le_bytes(count_bytes) as usize;

            let mut ts_bytes = [0u8; 8];
            cursor.read_exact(&mut ts_bytes).map_err(io_error)?;
            let timestamp_secs = u64::from_le_bytes(ts_bytes);

            Ok(CatalogWalRecord::CatalogCheckpoint {
                checkpoint_lsn,
                catalog_version,
                visible_procedure_count,
                timestamp_secs,
            })
        }
        t => Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!("unknown catalog record tag: {}", t),
        )),
    }
}

/// Helper: format checksum as hex string for logging.
fn format_checksum(checksum: &[u8; 32]) -> String {
    checksum
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_definition_batch_applied_round_trip() {
        let original = CatalogWalRecord::DefinitionBatchApplied {
            batch_id: 42,
            new_catalog_version: CatalogVersion::new(10),
            procedure_count: 2,
            affected_procedure_ids: vec![CatalogObjectId::new(100), CatalogObjectId::new(101)],
            timestamp_secs: 1234567890,
            operator_principal: "alice".to_string(),
        };

        let encoded = encode_catalog_record(&original).expect("encode failed");
        let decoded = decode_catalog_record(&encoded).expect("decode failed");

        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_procedure_added_round_trip() {
        let original = CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xAA),
            new_catalog_version: CatalogVersion::new(5),
            timestamp_secs: 1000,
        };

        let encoded = encode_catalog_record(&original).expect("encode failed");
        let decoded = decode_catalog_record(&encoded).expect("decode failed");

        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_procedure_altered_round_trip() {
        let original = CatalogWalRecord::ProcedureAltered {
            procedure_id: CatalogObjectId::new(2),
            old_hash: ContractHash::test_vector(0x11),
            new_hash: ContractHash::test_vector(0x22),
            new_catalog_version: CatalogVersion::new(6),
            timestamp_secs: 2000,
        };

        let encoded = encode_catalog_record(&original).expect("encode failed");
        let decoded = decode_catalog_record(&encoded).expect("decode failed");

        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_procedure_dropped_round_trip() {
        let original = CatalogWalRecord::ProcedureDropped {
            procedure_id: CatalogObjectId::new(3),
            dropped_version: CatalogVersion::new(5),
            new_catalog_version: CatalogVersion::new(6),
            timestamp_secs: 3000,
        };

        let encoded = encode_catalog_record(&original).expect("encode failed");
        let decoded = decode_catalog_record(&encoded).expect("decode failed");

        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_statistics_updated_round_trip() {
        let original = CatalogWalRecord::StatisticsUpdated {
            stats_version: 1,
            table_id: CatalogObjectId::new(10),
            column_id: CatalogObjectId::new(11),
            histogram_data_lsn: Lsn::new(5000),
            timestamp_secs: 4000,
        };

        let encoded = encode_catalog_record(&original).expect("encode failed");
        let decoded = decode_catalog_record(&encoded).expect("decode failed");

        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_decode_catalog_checkpoint_round_trip() {
        let original = CatalogWalRecord::CatalogCheckpoint {
            checkpoint_lsn: Lsn::new(10000),
            catalog_version: CatalogVersion::new(42),
            visible_procedure_count: 17,
            timestamp_secs: 5000,
        };

        let encoded = encode_catalog_record(&original).expect("encode failed");
        let decoded = decode_catalog_record(&encoded).expect("decode failed");

        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_is_deterministic() {
        let record = CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xFF),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        };

        let encoded1 = encode_catalog_record(&record).expect("encode 1 failed");
        let encoded2 = encode_catalog_record(&record).expect("encode 2 failed");

        assert_eq!(encoded1, encoded2);
    }

    #[test]
    fn decode_rejects_corrupted_checksum() {
        let original = CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xAA),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        };

        let mut encoded = encode_catalog_record(&original).expect("encode failed");

        // Corrupt the checksum (bytes 6..38)
        if encoded.len() > 38 {
            encoded[6] ^= 0xFF; // Flip bits in checksum
        }

        let result = decode_catalog_record(&encoded);
        assert!(result.is_err());
        assert!(result.unwrap_err().message().contains("checksum mismatch"));
    }

    #[test]
    fn decode_rejects_truncated_frame() {
        let bytes = vec![0u8; 10]; // Too short
        let result = decode_catalog_record(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn decode_rejects_invalid_version() {
        let mut frame = vec![99u8, 0u8]; // Invalid version
        frame.extend_from_slice(&[0u8; 4]); // payload_len
        frame.extend_from_slice(&[0u8; 32]); // checksum
        frame.push(0u8); // minimal payload

        let result = decode_catalog_record(&frame);
        assert!(result.is_err());
    }

    #[test]
    fn encode_rejects_invalid_record() {
        // Record with zero procedure count
        let record = CatalogWalRecord::DefinitionBatchApplied {
            batch_id: 1,
            new_catalog_version: CatalogVersion::new(1),
            procedure_count: 0,
            affected_procedure_ids: vec![],
            timestamp_secs: 1000,
            operator_principal: "test".to_string(),
        };

        let result = encode_catalog_record(&record);
        assert!(result.is_err());
    }
}
