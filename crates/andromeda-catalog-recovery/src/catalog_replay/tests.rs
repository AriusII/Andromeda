use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash};
use andromeda_wal::Lsn;

use crate::CatalogStorageWalRecord as CatalogWalRecord;

use super::*;

#[test]
fn replay_empty_records_produces_empty_snapshot() {
    let records = vec![];
    let snapshot =
        replay_catalog_wal_records(&records, CatalogVersion::new(10)).expect("replay failed");

    assert_eq!(snapshot.catalog_version, CatalogVersion::new(0));
    assert_eq!(snapshot.visible_procedure_count(), 0);
}

#[test]
fn replay_single_procedure_added_record() {
    let records = vec![CatalogWalRecord::ProcedureAdded {
        procedure_id: CatalogObjectId::new(1),
        signature_hash: ContractHash::test_vector(0xAA),
        new_catalog_version: CatalogVersion::new(1),
        timestamp_secs: 1000,
    }];

    let snapshot =
        replay_catalog_wal_records(&records, CatalogVersion::new(10)).expect("replay failed");

    assert_eq!(snapshot.catalog_version, CatalogVersion::new(1));
    assert_eq!(snapshot.visible_procedure_count(), 1);
    assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(1)));
}

#[test]
fn replay_validates_version_monotonicity() {
    let records = vec![
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xAA),
            new_catalog_version: CatalogVersion::new(5),
            timestamp_secs: 1000,
        },
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(2),
            signature_hash: ContractHash::test_vector(0xBB),
            new_catalog_version: CatalogVersion::new(3),
            timestamp_secs: 2000,
        },
    ];

    let result = replay_catalog_wal_records(&records, CatalogVersion::new(10));
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("version reordering"));
}

#[test]
fn replay_procedure_altered_validates_existence() {
    let records = vec![CatalogWalRecord::ProcedureAltered {
        procedure_id: CatalogObjectId::new(999),
        old_hash: ContractHash::test_vector(0x11),
        new_hash: ContractHash::test_vector(0x22),
        new_catalog_version: CatalogVersion::new(1),
        timestamp_secs: 1000,
    }];

    let result = replay_catalog_wal_records(&records, CatalogVersion::new(10));
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .message()
            .contains("non-existent procedure")
    );
}

#[test]
fn replay_procedure_dropped_removes_from_visible_set() {
    let records = vec![
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xAA),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        },
        CatalogWalRecord::ProcedureDropped {
            procedure_id: CatalogObjectId::new(1),
            dropped_version: CatalogVersion::new(1),
            new_catalog_version: CatalogVersion::new(2),
            timestamp_secs: 2000,
        },
    ];

    let snapshot =
        replay_catalog_wal_records(&records, CatalogVersion::new(10)).expect("replay failed");

    assert_eq!(snapshot.catalog_version, CatalogVersion::new(2));
    assert_eq!(snapshot.visible_procedure_count(), 0);
}

#[test]
fn replay_respects_target_version_filter() {
    let records = vec![
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xAA),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        },
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(2),
            signature_hash: ContractHash::test_vector(0xBB),
            new_catalog_version: CatalogVersion::new(5),
            timestamp_secs: 2000,
        },
    ];

    let snapshot =
        replay_catalog_wal_records(&records, CatalogVersion::new(3)).expect("replay failed");

    assert_eq!(snapshot.catalog_version, CatalogVersion::new(1));
    assert_eq!(snapshot.visible_procedure_count(), 1);
    assert!(!snapshot.procedure_ids.contains(&CatalogObjectId::new(2)));
}

#[test]
fn replay_checkpoint_validation_succeeds_on_match() {
    let records = vec![
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xAA),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        },
        CatalogWalRecord::CatalogCheckpoint {
            checkpoint_lsn: Lsn::new(100),
            catalog_version: CatalogVersion::new(1),
            visible_procedure_count: 1,
            timestamp_secs: 1001,
        },
    ];

    let result = replay_catalog_wal_records(&records, CatalogVersion::new(10));
    assert!(result.is_ok());
}

#[test]
fn replay_checkpoint_validation_fails_on_mismatch() {
    let records = vec![
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xAA),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        },
        CatalogWalRecord::CatalogCheckpoint {
            checkpoint_lsn: Lsn::new(100),
            catalog_version: CatalogVersion::new(1),
            visible_procedure_count: 99,
            timestamp_secs: 1001,
        },
    ];

    let result = replay_catalog_wal_records(&records, CatalogVersion::new(10));
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .message()
            .contains("visible_procedure_count mismatch")
    );
}

#[test]
fn replay_definition_batch_applied_adds_all_procedures() {
    let records = vec![CatalogWalRecord::DefinitionBatchApplied {
        batch_id: 42,
        new_catalog_version: CatalogVersion::new(1),
        procedure_count: 3,
        affected_procedure_ids: vec![
            CatalogObjectId::new(10),
            CatalogObjectId::new(11),
            CatalogObjectId::new(12),
        ],
        timestamp_secs: 1000,
        operator_principal: "alice".to_string(),
    }];

    let snapshot =
        replay_catalog_wal_records(&records, CatalogVersion::new(10)).expect("replay failed");

    assert_eq!(snapshot.visible_procedure_count(), 3);
    assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(10)));
    assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(11)));
    assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(12)));
}

#[test]
fn replay_complex_sequence_maintains_consistency() {
    let records = vec![
        CatalogWalRecord::DefinitionBatchApplied {
            batch_id: 1,
            new_catalog_version: CatalogVersion::new(1),
            procedure_count: 2,
            affected_procedure_ids: vec![CatalogObjectId::new(1), CatalogObjectId::new(2)],
            timestamp_secs: 1000,
            operator_principal: "alice".to_string(),
        },
        CatalogWalRecord::ProcedureAltered {
            procedure_id: CatalogObjectId::new(1),
            old_hash: ContractHash::test_vector(0x11),
            new_hash: ContractHash::test_vector(0x22),
            new_catalog_version: CatalogVersion::new(2),
            timestamp_secs: 2000,
        },
        CatalogWalRecord::DefinitionBatchApplied {
            batch_id: 2,
            new_catalog_version: CatalogVersion::new(3),
            procedure_count: 2,
            affected_procedure_ids: vec![CatalogObjectId::new(3), CatalogObjectId::new(2)],
            timestamp_secs: 3000,
            operator_principal: "bob".to_string(),
        },
        CatalogWalRecord::ProcedureDropped {
            procedure_id: CatalogObjectId::new(2),
            dropped_version: CatalogVersion::new(1),
            new_catalog_version: CatalogVersion::new(3),
            timestamp_secs: 3001,
        },
    ];

    let snapshot =
        replay_catalog_wal_records(&records, CatalogVersion::new(10)).expect("replay failed");

    assert_eq!(snapshot.catalog_version, CatalogVersion::new(3));
    assert_eq!(snapshot.visible_procedure_count(), 2);
    assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(1)));
    assert!(!snapshot.procedure_ids.contains(&CatalogObjectId::new(2)));
    assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(3)));
}

#[test]
fn replay_catalog_from_lsn_empty_records_ok() {
    let report = replay_catalog_from_lsn(&[], Lsn::new(1), CatalogVersion::new(99))
        .expect("empty replay must succeed");
    assert_eq!(report.total_records, 0);
    assert_eq!(report.records_replayed, 0);
    assert_eq!(report.records_below_floor, 0);
    assert!(report.end_lsn.is_none());
}

#[test]
fn replay_catalog_from_lsn_filters_records_below_floor() {
    let lsn_records = vec![
        CatalogStorageReplayLsnRecord::new(
            Lsn::new(5),
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: ContractHash::test_vector(0xAA),
                new_catalog_version: CatalogVersion::new(1),
                timestamp_secs: 1000,
            },
        ),
        CatalogStorageReplayLsnRecord::new(
            Lsn::new(15),
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(2),
                signature_hash: ContractHash::test_vector(0xBB),
                new_catalog_version: CatalogVersion::new(2),
                timestamp_secs: 2000,
            },
        ),
    ];

    let report = replay_catalog_from_lsn(&lsn_records, Lsn::new(10), CatalogVersion::new(99))
        .expect("replay must succeed");

    assert_eq!(report.total_records, 2);
    assert_eq!(report.records_below_floor, 1);
    assert_eq!(report.records_replayed, 1);
    assert_eq!(report.end_lsn, Some(Lsn::new(15)));
    assert_eq!(report.snapshot.visible_procedure_count(), 1);
    assert!(
        report
            .snapshot
            .procedure_ids
            .contains(&CatalogObjectId::new(2))
    );
    assert!(
        !report
            .snapshot
            .procedure_ids
            .contains(&CatalogObjectId::new(1))
    );
}

#[test]
fn replay_catalog_from_lsn_replays_all_when_floor_is_zero() {
    let lsn_records = vec![
        CatalogStorageReplayLsnRecord::new(
            Lsn::new(1),
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(10),
                signature_hash: ContractHash::test_vector(0xCC),
                new_catalog_version: CatalogVersion::new(1),
                timestamp_secs: 1000,
            },
        ),
        CatalogStorageReplayLsnRecord::new(
            Lsn::new(2),
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(11),
                signature_hash: ContractHash::test_vector(0xDD),
                new_catalog_version: CatalogVersion::new(2),
                timestamp_secs: 2000,
            },
        ),
    ];

    let report = replay_catalog_from_lsn(&lsn_records, Lsn::ZERO, CatalogVersion::new(99))
        .expect("replay must succeed");
    assert_eq!(report.records_replayed, 2);
    assert_eq!(report.records_below_floor, 0);
    assert!(report.all_records_replayed());
}

#[test]
fn replay_catalog_from_lsn_tracks_end_lsn_correctly() {
    let lsn_records = vec![
        CatalogStorageReplayLsnRecord::new(
            Lsn::new(100),
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: ContractHash::test_vector(0xAA),
                new_catalog_version: CatalogVersion::new(1),
                timestamp_secs: 1000,
            },
        ),
        CatalogStorageReplayLsnRecord::new(
            Lsn::new(200),
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(2),
                signature_hash: ContractHash::test_vector(0xBB),
                new_catalog_version: CatalogVersion::new(2),
                timestamp_secs: 2000,
            },
        ),
    ];

    let report = replay_catalog_from_lsn(&lsn_records, Lsn::new(100), CatalogVersion::new(99))
        .expect("replay must succeed");
    assert_eq!(report.end_lsn, Some(Lsn::new(200)));
}

#[test]
fn replay_catalog_from_lsn_propagates_version_reorder_error() {
    let lsn_records = vec![
        CatalogStorageReplayLsnRecord::new(
            Lsn::new(10),
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(1),
                signature_hash: ContractHash::test_vector(0xAA),
                new_catalog_version: CatalogVersion::new(5),
                timestamp_secs: 1000,
            },
        ),
        CatalogStorageReplayLsnRecord::new(
            Lsn::new(20),
            CatalogWalRecord::ProcedureAdded {
                procedure_id: CatalogObjectId::new(2),
                signature_hash: ContractHash::test_vector(0xBB),
                new_catalog_version: CatalogVersion::new(3),
                timestamp_secs: 2000,
            },
        ),
    ];

    let result = replay_catalog_from_lsn(&lsn_records, Lsn::new(10), CatalogVersion::new(99));
    assert!(result.is_err());
    assert!(result.unwrap_err().message().contains("version reordering"));
}
