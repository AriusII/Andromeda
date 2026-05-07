use andromeda_core::{CatalogObjectId, CatalogVersion, ContractHash};
use andromeda_storage::{CatalogWalRecord, replay_catalog_wal_records};

#[test]
fn replay_complete_procedure_lifecycle_add_alter_drop() {
    let records = vec![
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(1),
            signature_hash: ContractHash::test_vector(0x11),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1000,
        },
        CatalogWalRecord::ProcedureAltered {
            procedure_id: CatalogObjectId::new(1),
            old_hash: ContractHash::test_vector(0x11),
            new_hash: ContractHash::test_vector(0x22),
            new_catalog_version: CatalogVersion::new(2),
            timestamp_secs: 2000,
        },
        CatalogWalRecord::ProcedureDropped {
            procedure_id: CatalogObjectId::new(1),
            dropped_version: CatalogVersion::new(2),
            new_catalog_version: CatalogVersion::new(3),
            timestamp_secs: 3000,
        },
    ];

    let snapshot =
        replay_catalog_wal_records(&records, CatalogVersion::new(100)).expect("replay failed");

    assert_eq!(snapshot.catalog_version, CatalogVersion::new(3));
    assert_eq!(snapshot.visible_procedure_count(), 0);
}

#[test]
fn replay_respects_target_version_filter() {
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
            new_catalog_version: CatalogVersion::new(10),
            timestamp_secs: 2000,
        },
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(3),
            signature_hash: ContractHash::test_vector(0xDD),
            new_catalog_version: CatalogVersion::new(20),
            timestamp_secs: 3000,
        },
    ];

    let snapshot =
        replay_catalog_wal_records(&records, CatalogVersion::new(11)).expect("replay failed");

    assert_eq!(snapshot.catalog_version, CatalogVersion::new(10));
    assert_eq!(snapshot.visible_procedure_count(), 2);
    assert!(!snapshot.procedure_ids.contains(&CatalogObjectId::new(3)));
}
