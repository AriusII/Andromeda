use super::support::{manifest_switch_payload, replay_wal_from_lsn_into_context};
use andromeda_recovery::{ReplayContext, StartupMode, replay_wal_record};
use andromeda_wal::{Lsn, WalRecord, WalRecordKind};

#[test]
fn manifest_switch_with_valid_payload_is_applied() {
    let mut ctx = ReplayContext::new();
    ctx.known_manifest_crc_by_version.insert(7, 0xAA55_3311);
    let record = WalRecord::from_parts(
        WalRecordKind::ManifestSwitch,
        Lsn::new(77),
        Some(Lsn::new(76)),
        None,
        manifest_switch_payload(7, 90, 40, 45, [9; 32], 0xAA55_3311),
    )
    .expect("manifest switch record should be valid");

    replay_wal_record(&mut ctx, &record).expect("manifest switch should replay");

    assert_eq!(ctx.applied_count, 1);
    assert_eq!(ctx.skipped_count, 0);
    assert!(!ctx.has_errors());
    assert_eq!(
        ctx.active_manifest
            .expect("manifest switch should install active manifest")
            .snapshot_id,
        90
    );
}

#[test]
fn manifest_switch_recovery_rejects_non_monotonic_version() {
    let manifest = andromeda_manifest::DatabaseManifest {
        database_id: 1,
        manifest_version: 5,
        snapshot_id: 10,
        base_checkpoint_lsn: Lsn::new(10),
        required_wal_start_lsn: Lsn::new(20),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xABCD_5505,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    };
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::CheckpointEnd,
            Lsn::new(20),
            Some(Lsn::new(19)),
            None,
            Vec::new(),
        )
        .expect("checkpoint record should be valid"),
        WalRecord::from_parts(
            WalRecordKind::ManifestSwitch,
            Lsn::new(21),
            Some(Lsn::new(20)),
            None,
            manifest_switch_payload(5, 11, 20, 20, [1; 32], 0xABCD_5505),
        )
        .expect("manifest switch record should be valid"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &manifest,
        StartupMode::SafeStart,
        &records,
        &mut ReplayContext::new(),
    )
    .expect_err("manifest switch must fail closed when version does not advance");

    assert!(err.message().contains("regresses manifest version"));
}

#[test]
fn manifest_switch_recovery_rejects_non_advancing_checkpoint_lsn() {
    let manifest = andromeda_manifest::DatabaseManifest {
        database_id: 1,
        manifest_version: 5,
        snapshot_id: 10,
        base_checkpoint_lsn: Lsn::new(20),
        required_wal_start_lsn: Lsn::new(20),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xABCD_5505,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    };
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::CheckpointEnd,
            Lsn::new(20),
            Some(Lsn::new(19)),
            None,
            Vec::new(),
        )
        .expect("checkpoint record should be valid"),
        WalRecord::from_parts(
            WalRecordKind::ManifestSwitch,
            Lsn::new(21),
            Some(Lsn::new(20)),
            None,
            manifest_switch_payload(6, 11, 20, 20, [2; 32], 0xABCD_6606),
        )
        .expect("manifest switch record should be valid"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &manifest,
        StartupMode::SafeStart,
        &records,
        &mut ReplayContext::new(),
    )
    .expect_err("manifest switch must fail closed when checkpoint LSN does not advance");

    assert!(
        err.message()
            .contains("strictly advance base checkpoint LSN")
    );
}

#[test]
fn manifest_switch_recovery_rejects_missing_checkpoint_end_evidence() {
    let manifest = andromeda_manifest::DatabaseManifest {
        database_id: 1,
        manifest_version: 5,
        snapshot_id: 10,
        base_checkpoint_lsn: Lsn::new(10),
        required_wal_start_lsn: Lsn::new(20),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xABCD_5505,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    };
    let records = vec![
        WalRecord::from_parts(
            WalRecordKind::ManifestSwitch,
            Lsn::new(20),
            Some(Lsn::new(19)),
            None,
            manifest_switch_payload(6, 11, 20, 20, [3; 32], 0xABCD_6606),
        )
        .expect("manifest switch record should be valid"),
    ];

    let err = replay_wal_from_lsn_into_context(
        &manifest,
        StartupMode::SafeStart,
        &records,
        &mut ReplayContext::new(),
    )
    .expect_err("manifest switch must fail closed without durable checkpoint evidence");

    assert!(
        err.message()
            .contains("base_checkpoint_lsn lacks durable checkpoint_end evidence")
    );
}
