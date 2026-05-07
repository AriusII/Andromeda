use andromeda_core::{CatalogObjectId, CatalogVersion, ContractHash};
use andromeda_storage::{CatalogWalRecord, replay_catalog_wal_records};

#[test]
fn replay_rejects_version_going_backward() {
    let records = vec![
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xFF),
            new_catalog_version: CatalogVersion::new(10),
            timestamp_secs: 1000,
        },
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(2),
            signature_hash: ContractHash::test_vector(0xEE),
            new_catalog_version: CatalogVersion::new(5),
            timestamp_secs: 2000,
        },
    ];

    let err = replay_catalog_wal_records(&records, CatalogVersion::new(100))
        .expect_err("version reordering must be rejected");
    assert!(err.message().contains("version reordering"));
}

#[test]
fn replay_rejects_duplicate_version() {
    let records = vec![
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xFF),
            new_catalog_version: CatalogVersion::new(5),
            timestamp_secs: 1000,
        },
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(2),
            signature_hash: ContractHash::test_vector(0xEE),
            new_catalog_version: CatalogVersion::new(5),
            timestamp_secs: 2000,
        },
    ];

    let err = replay_catalog_wal_records(&records, CatalogVersion::new(100))
        .expect_err("duplicate version must be rejected");
    assert!(err.message().contains("version reordering"));
}

#[test]
fn replay_accepts_strictly_increasing_versions() {
    let records = vec![
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0xFF),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        },
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(2),
            signature_hash: ContractHash::test_vector(0xEE),
            new_catalog_version: CatalogVersion::new(2),
            timestamp_secs: 2000,
        },
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(3),
            signature_hash: ContractHash::test_vector(0xDD),
            new_catalog_version: CatalogVersion::new(5),
            timestamp_secs: 3000,
        },
    ];

    let snapshot =
        replay_catalog_wal_records(&records, CatalogVersion::new(100)).expect("replay failed");
    assert_eq!(snapshot.visible_procedure_count(), 3);
    assert_eq!(snapshot.catalog_version, CatalogVersion::new(5));
}

#[test]
fn replay_rejects_alter_on_non_existent_procedure() {
    let records = vec![CatalogWalRecord::ProcedureAltered {
        procedure_id: CatalogObjectId::new(999),
        old_hash: ContractHash::test_vector(0x11),
        new_hash: ContractHash::test_vector(0x22),
        new_catalog_version: CatalogVersion::new(1),
        timestamp_secs: 1000,
    }];

    let err = replay_catalog_wal_records(&records, CatalogVersion::new(100))
        .expect_err("alter on absent procedure must be rejected");
    assert!(err.message().contains("non-existent procedure"));
}

#[test]
fn replay_rejects_drop_on_non_existent_procedure() {
    let records = vec![CatalogWalRecord::ProcedureDropped {
        procedure_id: CatalogObjectId::new(888),
        dropped_version: CatalogVersion::new(1),
        new_catalog_version: CatalogVersion::new(2),
        timestamp_secs: 1000,
    }];

    let err = replay_catalog_wal_records(&records, CatalogVersion::new(100))
        .expect_err("drop on absent procedure must be rejected");
    assert!(err.message().contains("non-existent procedure"));
}

#[test]
fn replay_validates_all_batch_procedures_affect_visible_set() {
    let records = vec![CatalogWalRecord::DefinitionBatchApplied {
        batch_id: 1,
        new_catalog_version: CatalogVersion::new(1),
        procedure_count: 2,
        affected_procedure_ids: vec![CatalogObjectId::new(1), CatalogObjectId::new(2)],
        timestamp_secs: 1000,
        operator_principal: "test".to_string(),
    }];

    let snapshot =
        replay_catalog_wal_records(&records, CatalogVersion::new(100)).expect("replay failed");

    assert_eq!(snapshot.visible_procedure_count(), 2);
    assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(1)));
    assert!(snapshot.procedure_ids.contains(&CatalogObjectId::new(2)));
}
