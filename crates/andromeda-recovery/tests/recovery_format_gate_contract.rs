use andromeda_error::AndromedaErrorKind;
use andromeda_manifest::format_version::{FormatVersion, StorageFormatKind};
use andromeda_manifest::{DatabaseManifest, StorageFormatManifest};
use andromeda_recovery::{
    PreRedoStorageFormatDecision, PreRedoStorageFormatGate, PreRedoStorageFormatRejection,
    RECOVERY_REQUIRED_STORAGE_FORMATS, RecoveryPlan, StartupMode, StorageFormatFingerprint,
};
use andromeda_types::TransactionId;
use andromeda_wal::{Lsn, WalRecord, WalRecordKind};

fn recovery_v1_format_fingerprints_with(
    override_kind: StorageFormatKind,
    override_version: FormatVersion,
) -> Vec<StorageFormatFingerprint> {
    RECOVERY_REQUIRED_STORAGE_FORMATS
        .iter()
        .map(|kind| {
            StorageFormatFingerprint::new(
                *kind,
                if *kind == override_kind {
                    override_version
                } else {
                    FormatVersion::V1_0
                },
            )
        })
        .collect()
}

fn manifest() -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 2,
        snapshot_id: 3,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 99,
    }
}

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
    assert!(!decision.replay_allowed());
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
    let observed =
        recovery_v1_format_fingerprints_with(StorageFormatKind::HeapPage, FormatVersion::V2_0);

    let err = RecoveryPlan::from_manifest_wal_and_format_fingerprints(
        &manifest(),
        StartupMode::SafeStart,
        &records,
        &observed,
    )
    .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(
        err.message()
            .contains("unsupported storage subformat `Heap Page`")
    );
}

#[test]
fn recovery_plan_validates_supplied_storage_format_manifest_before_redo() {
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
    let mut storage_manifest = StorageFormatManifest::from_database_manifest(&manifest()).unwrap();
    storage_manifest.fingerprints[0].version = FormatVersion::V1_5;

    let err = RecoveryPlan::from_manifest_wal_and_storage_format_manifest(
        &manifest(),
        StartupMode::SafeStart,
        &records,
        &storage_manifest,
    )
    .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(err.message().contains("fingerprint hash mismatch"));
}
