use andromeda_catalog_recovery::{
    CatalogStorageWalRecord as CatalogWalRecord, replay_catalog_wal_records,
};
use andromeda_types::{CatalogObjectId, CatalogVersion};
use andromeda_wal::Lsn;

#[test]
fn replay_checkpoint_validates_procedure_count_match() {
    let records = vec![
        CatalogWalRecord::DefinitionBatchApplied {
            batch_id: 1,
            new_catalog_version: CatalogVersion::new(1),
            procedure_count: 1,
            affected_procedure_ids: vec![CatalogObjectId::new(1)],
            timestamp_secs: 1000,
            operator_principal: "test".to_string(),
        },
        CatalogWalRecord::CatalogCheckpoint {
            checkpoint_lsn: Lsn::new(100),
            catalog_version: CatalogVersion::new(1),
            visible_procedure_count: 1,
            timestamp_secs: 1001,
        },
    ];

    assert!(replay_catalog_wal_records(&records, CatalogVersion::new(100)).is_ok());
}

#[test]
fn replay_checkpoint_rejects_procedure_count_mismatch() {
    let records = vec![
        CatalogWalRecord::DefinitionBatchApplied {
            batch_id: 1,
            new_catalog_version: CatalogVersion::new(1),
            procedure_count: 1,
            affected_procedure_ids: vec![CatalogObjectId::new(1)],
            timestamp_secs: 1000,
            operator_principal: "test".to_string(),
        },
        CatalogWalRecord::CatalogCheckpoint {
            checkpoint_lsn: Lsn::new(100),
            catalog_version: CatalogVersion::new(1),
            visible_procedure_count: 42,
            timestamp_secs: 1001,
        },
    ];

    let err = replay_catalog_wal_records(&records, CatalogVersion::new(100))
        .expect_err("checkpoint count mismatch must be rejected");
    assert!(err.message().contains("visible_procedure_count mismatch"));
}
