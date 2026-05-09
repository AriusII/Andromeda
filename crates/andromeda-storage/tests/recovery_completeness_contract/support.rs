#![allow(dead_code)]

use andromeda_storage::{Lsn, WalRecord, WalRecordKind};
use andromeda_types::TransactionId;

pub const ALL_WAL_RECORD_KINDS: [WalRecordKind; 26] = [
    WalRecordKind::TxBegin,
    WalRecordKind::TxCommit,
    WalRecordKind::TxRollback,
    WalRecordKind::PageAllocate,
    WalRecordKind::PageFormat,
    WalRecordKind::RowInsert,
    WalRecordKind::RowUpdate,
    WalRecordKind::RowDelete,
    WalRecordKind::IndexInsert,
    WalRecordKind::IndexDelete,
    WalRecordKind::MvccVersionCreate,
    WalRecordKind::MvccVersionClose,
    WalRecordKind::MapDeltaAppend,
    WalRecordKind::CheckpointBegin,
    WalRecordKind::CheckpointEnd,
    WalRecordKind::SnapshotBegin,
    WalRecordKind::SnapshotEnd,
    WalRecordKind::ManifestSwitch,
    WalRecordKind::CatalogChangeBegin,
    WalRecordKind::CatalogChangeApply,
    WalRecordKind::CatalogChangeCommit,
    WalRecordKind::SecurityAuditAppend,
    WalRecordKind::BTreeInsert,
    WalRecordKind::BTreeDelete,
    WalRecordKind::BTreeSplit,
    WalRecordKind::BTreeMerge,
];

pub const SKIPPED_OR_IMPLEMENTED_KINDS: [WalRecordKind; 12] = [
    WalRecordKind::TxBegin,
    WalRecordKind::TxCommit,
    WalRecordKind::TxRollback,
    WalRecordKind::RowInsert,
    WalRecordKind::RowUpdate,
    WalRecordKind::RowDelete,
    WalRecordKind::CheckpointBegin,
    WalRecordKind::CheckpointEnd,
    WalRecordKind::SnapshotBegin,
    WalRecordKind::SnapshotEnd,
    WalRecordKind::ManifestSwitch,
    WalRecordKind::SecurityAuditAppend,
];

pub const FUTURE_WORK_KINDS: [WalRecordKind; 14] = [
    WalRecordKind::PageAllocate,
    WalRecordKind::PageFormat,
    WalRecordKind::IndexInsert,
    WalRecordKind::IndexDelete,
    WalRecordKind::MvccVersionCreate,
    WalRecordKind::MvccVersionClose,
    WalRecordKind::MapDeltaAppend,
    WalRecordKind::CatalogChangeBegin,
    WalRecordKind::CatalogChangeApply,
    WalRecordKind::CatalogChangeCommit,
    WalRecordKind::BTreeInsert,
    WalRecordKind::BTreeDelete,
    WalRecordKind::BTreeSplit,
    WalRecordKind::BTreeMerge,
];

pub fn is_index_btree_recovery_kind(kind: WalRecordKind) -> bool {
    matches!(
        kind,
        WalRecordKind::IndexInsert
            | WalRecordKind::IndexDelete
            | WalRecordKind::BTreeInsert
            | WalRecordKind::BTreeDelete
            | WalRecordKind::BTreeSplit
            | WalRecordKind::BTreeMerge
    )
}

pub fn record_for_kind(kind: WalRecordKind, lsn: Lsn) -> WalRecord {
    WalRecord::from_parts(
        kind,
        lsn,
        None,
        kind.requires_transaction_id()
            .then_some(TransactionId::new(1)),
        Vec::new(),
    )
    .expect("test WAL record should be valid")
}

pub fn manifest_for_replay_from(
    required_wal_start_lsn: Lsn,
) -> andromeda_storage::DatabaseManifest {
    andromeda_storage::DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::ZERO,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xCAFE_BABE,
    }
}

pub fn manifest_switch_payload(
    manifest_version: u64,
    snapshot_id: u64,
    base_checkpoint_lsn: u64,
    required_wal_start_lsn: u64,
    previous_manifest_hash: [u8; 32],
    manifest_crc: u32,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(68);
    payload.extend_from_slice(&manifest_version.to_le_bytes());
    payload.extend_from_slice(&snapshot_id.to_le_bytes());
    payload.extend_from_slice(&base_checkpoint_lsn.to_le_bytes());
    payload.extend_from_slice(&required_wal_start_lsn.to_le_bytes());
    payload.extend_from_slice(&previous_manifest_hash);
    payload.extend_from_slice(&manifest_crc.to_le_bytes());
    payload
}
