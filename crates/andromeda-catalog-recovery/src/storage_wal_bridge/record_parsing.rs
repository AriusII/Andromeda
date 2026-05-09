use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash};
use andromeda_wal::Lsn;
use std::io::{Read, Write};

use crate::{CatalogStorageWalRecord, CatalogStorageWalRecordVersion};

use super::error::{catalog_wal_error, io_error};

/// Helper: encode record payload to writer in deterministic order.
pub(super) fn encode_record_payload(
    writer: &mut dyn Write,
    record: &CatalogStorageWalRecord,
) -> AndromedaResult<()> {
    // Collect bytes first (Vec::write never fails), then write all at once
    let mut bytes = Vec::new();

    match record {
        CatalogStorageWalRecord::DefinitionBatchApplied {
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
        },
        CatalogStorageWalRecord::ProcedureAdded {
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
        },
        CatalogStorageWalRecord::ProcedureAltered {
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
        },
        CatalogStorageWalRecord::ProcedureDropped {
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
        },
        CatalogStorageWalRecord::StatisticsUpdated {
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
        },
        CatalogStorageWalRecord::CatalogCheckpoint {
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
        },
    }

    // Now write the collected bytes (convert io::Error to AndromedaError)
    writer.write_all(&bytes).map_err(io_error)?;
    Ok(())
}

/// Helper: decode record payload from reader in deterministic order.
pub(super) fn decode_record_payload(
    bytes: &[u8],
    _version: CatalogStorageWalRecordVersion,
) -> AndromedaResult<CatalogStorageWalRecord> {
    let mut cursor = std::io::Cursor::new(bytes);

    // Read tag (first byte)
    let mut tag_byte = [0u8; 1];
    cursor.read_exact(&mut tag_byte).map_err(io_error)?;
    let tag = tag_byte[0];

    let record = match tag {
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

            let remaining_after_count = bytes.len().saturating_sub(cursor.position() as usize);
            let min_tail_bytes = 8usize + 4usize;
            let required_procedure_and_tail_bytes = procedure_count
                .checked_mul(8)
                .and_then(|procedure_bytes| procedure_bytes.checked_add(min_tail_bytes))
                .ok_or_else(|| {
                    catalog_wal_error(
                        "DefinitionBatchApplied procedure_count overflows payload bounds",
                    )
                })?;
            if required_procedure_and_tail_bytes > remaining_after_count {
                return Err(catalog_wal_error(format!(
                    "DefinitionBatchApplied procedure_count {} exceeds payload remainder {}",
                    procedure_count, remaining_after_count
                )));
            }

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

            let remaining_principal_bytes = bytes.len().saturating_sub(cursor.position() as usize);
            if principal_len > remaining_principal_bytes {
                return Err(catalog_wal_error(format!(
                    "operator_principal length {} exceeds payload remainder {}",
                    principal_len, remaining_principal_bytes
                )));
            }

            let mut principal_bytes = vec![0u8; principal_len];
            cursor.read_exact(&mut principal_bytes).map_err(io_error)?;
            let operator_principal = String::from_utf8(principal_bytes).map_err(|e| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    format!("operator_principal is not valid UTF-8: {}", e),
                )
            })?;

            CatalogStorageWalRecord::DefinitionBatchApplied {
                batch_id,
                new_catalog_version,
                procedure_count,
                affected_procedure_ids,
                timestamp_secs,
                operator_principal,
            }
        },
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

            CatalogStorageWalRecord::ProcedureAdded {
                procedure_id,
                signature_hash,
                new_catalog_version,
                timestamp_secs,
            }
        },
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

            CatalogStorageWalRecord::ProcedureAltered {
                procedure_id,
                old_hash,
                new_hash,
                new_catalog_version,
                timestamp_secs,
            }
        },
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

            CatalogStorageWalRecord::ProcedureDropped {
                procedure_id,
                dropped_version,
                new_catalog_version,
                timestamp_secs,
            }
        },
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

            CatalogStorageWalRecord::StatisticsUpdated {
                stats_version,
                table_id,
                column_id,
                histogram_data_lsn,
                timestamp_secs,
            }
        },
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

            CatalogStorageWalRecord::CatalogCheckpoint {
                checkpoint_lsn,
                catalog_version,
                visible_procedure_count,
                timestamp_secs,
            }
        },
        t => Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            format!("unknown catalog record tag: {}", t),
        ))?,
    };

    let consumed = cursor.position() as usize;
    if consumed != bytes.len() {
        return Err(catalog_wal_error(format!(
            "catalog record payload has trailing bytes: consumed {}, payload has {} bytes",
            consumed,
            bytes.len()
        )));
    }

    Ok(record)
}
