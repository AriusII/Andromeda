use super::*;
use crate::Lsn;
use crate::write_ahead_log::record::{WalRecord, WalRecordKind};
use andromeda_core::AndromedaErrorKind;

fn record(lsn: u64, prev: Option<u64>) -> WalRecord {
    WalRecord::from_parts(
        WalRecordKind::PageAllocate,
        Lsn::new(lsn),
        prev.map(Lsn::new),
        None,
        Vec::<u8>::new(),
    )
    .expect("record builds")
}

fn primary() -> WalNodeIdentity {
    WalNodeIdentity::new(1, WalNodeRole::Primary)
}
fn replica() -> WalNodeIdentity {
    WalNodeIdentity::new(2, WalNodeRole::Replica)
}

#[test]
fn contiguous_batch_is_accepted() {
    let records = vec![
        record(10, Some(9)),
        record(11, Some(10)),
        record(12, Some(11)),
    ];
    let batch = WalShipmentBatch::new(
        primary(),
        replica(),
        WalReplicaExpectation::after(Lsn::new(9), Lsn::new(10)),
        &records,
    );

    let accepted = batch.validate().expect("contiguous batch accepted");
    assert_eq!(accepted.range.first, Lsn::new(10));
    assert_eq!(accepted.range.last, Lsn::new(12));
    assert_eq!(accepted.range.count, 3);
    assert_eq!(accepted.next_expected_lsn, Lsn::new(13));
}

#[test]
fn genesis_batch_with_no_previous_lsn_is_accepted() {
    let records = vec![record(1, None), record(2, Some(1))];
    let batch = WalShipmentBatch::new(
        primary(),
        replica(),
        WalReplicaExpectation::genesis(Lsn::new(1)),
        &records,
    );
    let accepted = batch.validate().expect("genesis accepted");
    assert_eq!(accepted.range.first, Lsn::new(1));
    assert_eq!(accepted.next_expected_lsn, Lsn::new(3));
}

#[test]
fn gap_in_chain_is_rejected() {
    // 12 skips LSN 11 even though it links to the prior record.
    let records = vec![record(10, Some(9)), record(12, Some(10))];
    let batch = WalShipmentBatch::new(
        primary(),
        replica(),
        WalReplicaExpectation::after(Lsn::new(9), Lsn::new(10)),
        &records,
    );
    let err = batch.validate().expect_err("gap rejected");
    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(err.to_string().contains("gap"));
}

#[test]
fn first_lsn_mismatch_is_rejected() {
    let records = vec![record(11, Some(10))];
    let batch = WalShipmentBatch::new(
        primary(),
        replica(),
        WalReplicaExpectation::after(Lsn::new(9), Lsn::new(10)),
        &records,
    );
    let err = batch.validate().expect_err("unexpected first lsn rejected");
    assert!(err.to_string().contains("expected next LSN"));
}

#[test]
fn first_previous_lsn_mismatch_is_rejected() {
    // First record's previous_lsn = 8 but replica tail is 9.
    let records = vec![record(10, Some(8))];
    let batch = WalShipmentBatch::new(
        primary(),
        replica(),
        WalReplicaExpectation::after(Lsn::new(9), Lsn::new(10)),
        &records,
    );
    let err = batch
        .validate()
        .expect_err("previous_lsn mismatch rejected");
    assert!(err.to_string().contains("link to replica tail"));
}

#[test]
fn reordered_batch_is_rejected() {
    let records = vec![
        record(10, Some(9)),
        record(12, Some(10)),
        record(11, Some(10)),
    ];
    let batch = WalShipmentBatch::new(
        primary(),
        replica(),
        WalReplicaExpectation::after(Lsn::new(9), Lsn::new(10)),
        &records,
    );
    let err = batch.validate().expect_err("reorder rejected");
    assert!(err.to_string().contains("gap"));
}

#[test]
fn duplicate_lsn_is_rejected() {
    let records = vec![
        record(10, Some(9)),
        record(11, Some(10)),
        record(11, Some(10)),
    ];
    let batch = WalShipmentBatch::new(
        primary(),
        replica(),
        WalReplicaExpectation::after(Lsn::new(9), Lsn::new(10)),
        &records,
    );
    let err = batch.validate().expect_err("duplicate rejected");
    assert!(err.to_string().contains("duplicate or reordered"));
}

#[test]
fn empty_batch_is_rejected() {
    let records: Vec<WalRecord> = Vec::new();
    let batch = WalShipmentBatch::new(
        primary(),
        replica(),
        WalReplicaExpectation::after(Lsn::new(9), Lsn::new(10)),
        &records,
    );
    let err = batch.validate().expect_err("empty rejected");
    assert!(err.to_string().contains("empty"));
}

#[test]
fn shipping_from_non_primary_role_is_rejected() {
    let records = vec![record(10, Some(7))];
    let bad_source = WalNodeIdentity::new(99, WalNodeRole::Replica);
    let batch = WalShipmentBatch::new(
        bad_source,
        replica(),
        WalReplicaExpectation::after(Lsn::new(9), Lsn::new(10)),
        &records,
    );
    let err = batch.validate().expect_err("non-primary source rejected");
    assert!(err.to_string().contains("source is not a primary"));
}

#[test]
fn shipping_to_non_replica_target_is_rejected() {
    let records = vec![record(10, Some(7))];
    let bad_target = WalNodeIdentity::new(3, WalNodeRole::Primary);
    let batch = WalShipmentBatch::new(
        primary(),
        bad_target,
        WalReplicaExpectation::after(Lsn::new(9), Lsn::new(10)),
        &records,
    );
    let err = batch.validate().expect_err("primary target rejected");
    assert!(err.to_string().contains("target is not a replica"));
}

#[test]
fn replica_expectation_must_match_tail_next_lsn() {
    let records = vec![record(10, Some(9))];
    let batch = WalShipmentBatch::new(
        primary(),
        replica(),
        WalReplicaExpectation::after(Lsn::new(7), Lsn::new(10)),
        &records,
    );

    let err = batch
        .validate()
        .expect_err("replica expectation must be contiguous");
    assert!(err.to_string().contains("replica expectation"));
}

#[test]
fn zero_node_identity_is_rejected() {
    let records = vec![record(10, Some(9))];
    let batch = WalShipmentBatch::new(
        WalNodeIdentity::new(0, WalNodeRole::Primary),
        replica(),
        WalReplicaExpectation::after(Lsn::new(9), Lsn::new(10)),
        &records,
    );

    let err = batch.validate().expect_err("zero source id rejected");
    assert!(err.to_string().contains("source id"));
}

#[test]
fn shipment_ending_at_max_lsn_is_rejected_without_overflow() {
    let records = vec![record(u64::MAX, Some(u64::MAX - 1))];
    let batch = WalShipmentBatch::new(
        primary(),
        replica(),
        WalReplicaExpectation::after(Lsn::new(u64::MAX - 1), Lsn::MAX),
        &records,
    );

    let err = batch
        .validate()
        .expect_err("next expected LSN overflow must be rejected");
    assert!(err.to_string().contains("overflow"));
}
