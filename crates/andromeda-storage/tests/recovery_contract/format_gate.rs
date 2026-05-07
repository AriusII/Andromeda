use crate::support::recovery_v1_format_fingerprints_with;
use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_storage::format_version::{FormatVersion, StorageFormatKind};
use andromeda_storage::{
    DatabaseManifest, Lsn, PreRedoStorageFormatDecision, PreRedoStorageFormatGate,
    PreRedoStorageFormatRejection, RECOVERY_REQUIRED_STORAGE_FORMATS, RecoveryPlan, StartupMode,
    StorageFormatFingerprint, StorageFormatManifest, WalRecord, WalRecordKind,
};

#[test]
fn pre_redo_format_gate_rejects_unknown_heap_layout_before_redo() {
    let observed = [
        StorageFormatFingerprint::new(StorageFormatKind::Page, FormatVersion::V1_0),
        StorageFormatFingerprint::new(StorageFormatKind::BTreeKey, FormatVersion::V1_0),
        StorageFormatFingerprint::new(StorageFormatKind::BTreeNode, FormatVersion::V1_0),
        StorageFormatFingerprint::new(StorageFormatKind::WalPayload, FormatVersion::V1_0),
    ];

    assert_eq!(
        PreRedoStorageFormatGate::decide(
            StartupMode::SafeStart,
            false,
            RECOVERY_REQUIRED_STORAGE_FORMATS,
            &observed,
        ),
        Err(PreRedoStorageFormatRejection::Unknown {
            kind: StorageFormatKind::HeapPage
        })
    );
}

#[test]
fn pre_redo_format_gate_rejects_unsupported_btree_key_version() {
    let observed =
        recovery_v1_format_fingerprints_with(StorageFormatKind::BTreeKey, FormatVersion::V1_5);

    assert_eq!(
        PreRedoStorageFormatGate::decide(
            StartupMode::FastStart,
            false,
            RECOVERY_REQUIRED_STORAGE_FORMATS,
            &observed,
        ),
        Err(PreRedoStorageFormatRejection::Unsupported {
            kind: StorageFormatKind::BTreeKey,
            observed: FormatVersion::V1_5,
            reader: FormatVersion::V1_0,
        })
    );
}

#[test]
fn forensic_start_opens_unknown_format_read_only_without_replay() {
    let observed = [
        StorageFormatFingerprint::new(StorageFormatKind::Page, FormatVersion::V1_0),
        StorageFormatFingerprint::new(StorageFormatKind::BTreeKey, FormatVersion::V1_0),
        StorageFormatFingerprint::new(StorageFormatKind::BTreeNode, FormatVersion::V1_0),
        StorageFormatFingerprint::new(StorageFormatKind::WalPayload, FormatVersion::V1_0),
    ];

    let decision = PreRedoStorageFormatGate::decide(
        StartupMode::ForensicStart,
        true,
        RECOVERY_REQUIRED_STORAGE_FORMATS,
        &observed,
    )
    .unwrap();

    assert_eq!(
        decision,
        PreRedoStorageFormatDecision::ForensicReadOnly {
            reason: PreRedoStorageFormatRejection::Unknown {
                kind: StorageFormatKind::HeapPage
            }
        }
    );
    assert!(
        !decision.replay_allowed(),
        "ForensicStart may inspect unknown storage formats only with replay disabled"
    );
}

#[test]
fn recovery_plan_format_fingerprint_api_fails_before_wal_redo_validation() {
    let transaction_id = TransactionId::new(70);
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::TxBegin,
            Lsn::new(1),
            None,
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::TxCommit,
            Lsn::new(3),
            Some(Lsn::new(1)),
            Some(transaction_id),
            Vec::new(),
        )
        .unwrap(),
    ];
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };
    let observed =
        recovery_v1_format_fingerprints_with(StorageFormatKind::HeapPage, FormatVersion::V2_0);

    let err = RecoveryPlan::from_manifest_wal_and_format_fingerprints(
        &manifest,
        StartupMode::SafeStart,
        &records,
        &observed,
    )
    .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(
        err.message()
            .contains("unsupported storage subformat `Heap Page`"),
        "format gate must fail before WAL redo validation; got: {}",
        err.message()
    );
}

#[test]
fn recovery_plan_reads_and_validates_format_fingerprints_from_manifest_before_redo() {
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::PageAllocate,
            Lsn::new(1),
            None,
            None,
            Vec::new(),
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::PageFormat,
            Lsn::new(3),
            Some(Lsn::new(1)),
            None,
            Vec::new(),
        )
        .unwrap(),
    ];
    let storage_manifest = StorageFormatManifest::new(
        manifest.database_id,
        manifest.manifest_version,
        manifest.snapshot_id,
        recovery_v1_format_fingerprints_with(StorageFormatKind::HeapPage, FormatVersion::V2_0),
    );

    let err = RecoveryPlan::from_manifest_wal_and_storage_format_manifest(
        &manifest,
        StartupMode::SafeStart,
        &records,
        &storage_manifest,
    )
    .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(
        err.message()
            .contains("unsupported storage subformat `Heap Page`"),
        "manifest-backed format gate must fail before WAL coverage validation; got: {}",
        err.message()
    );
}

#[test]
fn recovery_plan_rejects_tampered_storage_format_manifest_before_redo() {
    let manifest = DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    };
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::PageFormat,
            Lsn::new(1),
            None,
            None,
            Vec::new(),
        )
        .unwrap(),
    ];
    let mut storage_manifest = manifest.storage_format_manifest().unwrap();
    storage_manifest.fingerprints[0].version = FormatVersion::V1_5;

    let err = RecoveryPlan::from_manifest_wal_and_storage_format_manifest(
        &manifest,
        StartupMode::SafeStart,
        &records,
        &storage_manifest,
    )
    .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(
        err.message().contains("fingerprint hash mismatch"),
        "tampered manifest fingerprints must be rejected before redo; got: {}",
        err.message()
    );
}
