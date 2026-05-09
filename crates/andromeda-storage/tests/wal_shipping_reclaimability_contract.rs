use andromeda_storage::{
    CatalogWalRecord, Lsn, LsnBoundCatalogRecord, replay_catalog_from_lsn,
    write_ahead_log::{WalReplicaSafeLsnTracker, WalShippingAck},
};
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash};

#[test]
fn shipped_catalog_publication_can_replay_at_storage_lsn_boundary() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2]).expect("tracker");
    tracker
        .record_ack(WalShippingAck::new(2, Lsn::new(20)))
        .expect("catalog publication shipped");

    let lsn_records = vec![LsnBoundCatalogRecord::new(
        Lsn::new(20),
        CatalogWalRecord::ProcedureAdded {
            procedure_id: CatalogObjectId::new(42),
            signature_hash: ContractHash::test_vector(0x42),
            new_catalog_version: CatalogVersion::new(1),
            timestamp_secs: 1_700_000_000,
        },
    )];

    assert!(tracker.all_required_replicas_have_shipped(Lsn::new(20)));
    let replay = replay_catalog_from_lsn(&lsn_records, Lsn::new(20), CatalogVersion::new(1))
        .expect("catalog replay");

    assert_eq!(replay.records_replayed, 1);
    assert_eq!(replay.snapshot.catalog_version, CatalogVersion::new(1));
    assert!(
        replay
            .snapshot
            .procedure_ids
            .contains(&CatalogObjectId::new(42))
    );
}
