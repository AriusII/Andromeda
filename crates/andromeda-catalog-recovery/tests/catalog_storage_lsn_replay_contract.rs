use andromeda_catalog_recovery::{
    CatalogStorageReplayLsnRecord, CatalogStorageWalRecord, replay_catalog_from_lsn,
};
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash};
use andromeda_wal::Lsn;

#[test]
fn catalog_publication_replays_from_storage_lsn_boundary() {
    let lsn_records = vec![CatalogStorageReplayLsnRecord::new(
        Lsn::new(20),
        CatalogStorageWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(42),
            signature_hash: ContractHash::test_vector(0x42),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1_700_000_000,
        },
    )];

    let replay = replay_catalog_from_lsn(&lsn_records, Lsn::new(20), CatalogVersion::new(1))
        .expect("catalog replay at shipped storage boundary");

    assert_eq!(replay.records_replayed, 1);
    assert_eq!(replay.snapshot.catalog_version, CatalogVersion::new(1));
    assert!(
        replay
            .snapshot
            .procedure_ids
            .contains(&CatalogObjectId::new(42))
    );
}
