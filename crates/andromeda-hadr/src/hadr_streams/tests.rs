use super::*;

#[test]
fn test_stream_allocation_deterministic() {
    let alloc1 = StreamAllocation::new(0).unwrap();
    let alloc2 = StreamAllocation::new(0).unwrap();

    assert_eq!(alloc1.heartbeat_stream_id(), alloc2.heartbeat_stream_id());
    assert_eq!(
        alloc1.wal_shipping_stream_id(),
        alloc2.wal_shipping_stream_id()
    );
    assert_eq!(
        alloc1.promotion_vote_stream_id(),
        alloc2.promotion_vote_stream_id()
    );
}

#[test]
fn test_stream_allocation_non_overlapping() {
    let alloc0 = StreamAllocation::new(0).unwrap();
    let alloc1 = StreamAllocation::new(1).unwrap();

    let ids0 = vec![
        alloc0.heartbeat_stream_id(),
        alloc0.wal_shipping_stream_id(),
        alloc0.promotion_vote_stream_id(),
    ];
    let ids1 = vec![
        alloc1.heartbeat_stream_id(),
        alloc1.wal_shipping_stream_id(),
        alloc1.promotion_vote_stream_id(),
    ];

    for &id0 in &ids0 {
        for &id1 in &ids1 {
            assert_ne!(id0, id1, "stream IDs must not overlap");
        }
    }
}

#[test]
fn test_logical_stream_ids_do_not_encode_quic_parity() {
    let alloc0 = StreamAllocation::new(0).unwrap();
    let alloc1 = StreamAllocation::new(1).unwrap();

    assert_eq!(alloc0.heartbeat_stream_id(), HEARTBEAT_STREAM_MIN);
    assert_eq!(alloc1.heartbeat_stream_id(), HEARTBEAT_STREAM_MIN + 1);
    assert_eq!(alloc1.wal_shipping_stream_id(), WAL_SHIPPING_STREAM_MIN + 1);
    assert_eq!(alloc1.promotion_vote_stream_id(), VOTE_STREAM_MIN + 1);
    assert_eq!(alloc1.heartbeat_stream_id() % 2, 1);
    assert!(alloc1.heartbeat_logical_stream_id().is_hadr_reserved());
}

#[test]
fn test_stream_allocation_bounds() {
    let alloc = StreamAllocation::new(0).unwrap();

    assert!(alloc.heartbeat_stream_id() >= HEARTBEAT_STREAM_MIN);
    assert!(alloc.heartbeat_stream_id() <= HEARTBEAT_STREAM_MAX);

    assert!(alloc.wal_shipping_stream_id() >= WAL_SHIPPING_STREAM_MIN);
    assert!(alloc.wal_shipping_stream_id() <= WAL_SHIPPING_STREAM_MAX);

    assert!(alloc.promotion_vote_stream_id() >= VOTE_STREAM_MIN);
    assert!(alloc.promotion_vote_stream_id() <= VOTE_STREAM_MAX);
}

#[test]
fn test_stream_allocation_replica_index_bounds() {
    for i in 0..HEARTBEAT_MAX_REPLICAS
        .min(WAL_SHIPPING_MAX_REPLICAS)
        .min(VOTE_MAX_REPLICAS)
    {
        assert!(StreamAllocation::new(i).is_ok());
    }

    let max_index = HEARTBEAT_MAX_REPLICAS
        .min(WAL_SHIPPING_MAX_REPLICAS)
        .min(VOTE_MAX_REPLICAS);
    assert!(StreamAllocation::new(max_index).is_err());
}

#[test]
fn test_stream_kind_lookup() {
    let alloc = StreamAllocation::new(2).unwrap();

    assert_eq!(
        alloc.stream_kind_for_id(alloc.heartbeat_stream_id()),
        Some(HadrStreamKind::Heartbeat)
    );
    assert_eq!(
        alloc.stream_kind_for_id(alloc.wal_shipping_stream_id()),
        Some(HadrStreamKind::WalShipping)
    );
    assert_eq!(
        alloc.stream_kind_for_id(alloc.promotion_vote_stream_id()),
        Some(HadrStreamKind::PromotionVote)
    );

    assert_eq!(alloc.stream_kind_for_id(999), None);
}

#[test]
fn test_stream_multiplexer_allocate_replicas() {
    let mut mux = StreamMultiplexer::new();

    let alloc0 = mux.allocate_replica_streams(0).unwrap();
    let alloc1 = mux.allocate_replica_streams(1).unwrap();

    assert_eq!(mux.open_stream_count(), 6);
    assert_eq!(
        mux.lookup_stream(alloc0.heartbeat_stream_id()),
        Some((0, HadrStreamKind::Heartbeat))
    );
    assert_eq!(
        mux.lookup_stream(alloc1.wal_shipping_stream_id()),
        Some((1, HadrStreamKind::WalShipping))
    );
}

#[test]
fn test_stream_multiplexer_duplicate_allocation() {
    let mut mux = StreamMultiplexer::new();

    let _ = mux.allocate_replica_streams(0).unwrap();
    let result = mux.allocate_replica_streams(0);

    assert!(result.is_err());
}

#[test]
fn test_stream_multiplexer_close_stream() {
    let mut mux = StreamMultiplexer::new();
    let alloc = mux.allocate_replica_streams(0).unwrap();

    assert!(mux.is_stream_open(alloc.heartbeat_stream_id()));
    assert!(!mux.is_stream_closed(alloc.heartbeat_stream_id()));

    mux.close_stream(alloc.heartbeat_stream_id()).unwrap();

    assert!(!mux.is_stream_open(alloc.heartbeat_stream_id()));
    assert!(mux.is_stream_closed(alloc.heartbeat_stream_id()));
}

#[test]
fn test_orphan_stream_cleanup_idempotent() {
    let mut cleanup = HadrStreamCleanup::new();

    cleanup.mark_orphan(128);
    cleanup.mark_orphan(160);
    cleanup.mark_orphan(192);

    assert_eq!(cleanup.orphan_count(), 3);

    let cleaned1 = cleanup.execute_cleanup();
    assert_eq!(cleaned1.len(), 3);
    assert!(cleaned1.contains(&128));

    let cleaned2 = cleanup.execute_cleanup();
    assert!(cleaned2.is_empty());
}

#[test]
fn test_hadr_stream_kind_bounds() {
    assert_eq!(
        HadrStreamKind::Heartbeat.min_stream_id(),
        HEARTBEAT_STREAM_MIN
    );
    assert_eq!(
        HadrStreamKind::Heartbeat.max_stream_id(),
        HEARTBEAT_STREAM_MAX
    );

    assert_eq!(
        HadrStreamKind::WalShipping.min_stream_id(),
        WAL_SHIPPING_STREAM_MIN
    );
    assert_eq!(
        HadrStreamKind::WalShipping.max_stream_id(),
        WAL_SHIPPING_STREAM_MAX
    );

    assert_eq!(
        HadrStreamKind::PromotionVote.min_stream_id(),
        VOTE_STREAM_MIN
    );
    assert_eq!(
        HadrStreamKind::PromotionVote.max_stream_id(),
        VOTE_STREAM_MAX
    );
}
