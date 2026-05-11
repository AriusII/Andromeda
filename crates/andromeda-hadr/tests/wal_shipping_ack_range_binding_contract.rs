use andromeda_hadr::shipping_contract::{
    WalReplicaSafeLsnTracker, WalShipmentRange, WalShippingAck, WalShippingAckBindingRejection,
};
use andromeda_wal::Lsn;

fn range(first: u64, last: u64) -> WalShipmentRange {
    WalShipmentRange {
        first: Lsn::new(first),
        last: Lsn::new(last),
        count: (last - first + 1) as usize,
    }
}

#[test]
fn ack_is_bound_to_contiguous_validated_durable_prefix() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2]).expect("tracker");

    tracker
        .record_validated_range(2, range(1, 3))
        .expect("prefix");
    tracker
        .record_validated_range(2, range(5, 6))
        .expect("out-of-order gap segment");

    tracker
        .record_ack_bound(WalShippingAck::new(2, Lsn::new(3)))
        .expect("ack at contiguous prefix end");

    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(2, Lsn::new(4)))
            .expect_err("gap not validated"),
        WalShippingAckBindingRejection::AckOutsideDurablePrefix
    );
    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(2, Lsn::new(7)))
            .expect_err("ack above shipped max"),
        WalShippingAckBindingRejection::AckExceedsShippedMax
    );

    tracker
        .record_validated_range(2, range(4, 4))
        .expect("bridge gap");
    tracker
        .record_ack_bound(WalShippingAck::new(2, Lsn::new(6)))
        .expect("prefix extends after gap is bridged");

    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(2, Lsn::new(6)))
            .expect_err("stale ack"),
        WalShippingAckBindingRejection::StaleAck
    );
}

#[test]
fn ack_binding_rejects_invalid_ranges_unknown_replicas_and_reordered_bridgeing() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2]).expect("tracker");

    assert_eq!(
        tracker
            .record_validated_range(
                2,
                WalShipmentRange {
                    first: Lsn::new(50),
                    last: Lsn::new(52),
                    count: 2,
                },
            )
            .expect_err("range count/span mismatch"),
        WalShippingAckBindingRejection::InvalidValidatedRange
    );

    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(3, Lsn::new(1)))
            .expect_err("ack from non-required replica"),
        WalShippingAckBindingRejection::ReplicaNotRequired
    );

    tracker
        .record_validated_range(2, range(1, 3))
        .expect("range A");
    tracker
        .record_validated_range(2, range(5, 6))
        .expect("range C");
    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(2, Lsn::new(5)))
            .expect_err("gap blocks ack"),
        WalShippingAckBindingRejection::AckOutsideDurablePrefix
    );

    tracker
        .record_validated_range(2, range(4, 4))
        .expect("bridge B");
    tracker
        .record_ack_bound(WalShippingAck::new(2, Lsn::new(6)))
        .expect("bridged reordered ranges produce contiguous prefix");
}

#[test]
fn ack_binding_rejects_jump_without_validated_prefix_from_current_safe_lsn() {
    let mut tracker = WalReplicaSafeLsnTracker::new([2]).expect("tracker");
    tracker
        .record_validated_range(2, range(100, 100))
        .expect("high isolated range");

    assert_eq!(
        tracker
            .record_ack_bound(WalShippingAck::new(2, Lsn::new(100)))
            .expect_err("ack must not jump over unvalidated prefix"),
        WalShippingAckBindingRejection::AckOutsideDurablePrefix
    );

    tracker
        .record_validated_range(2, range(1, 99))
        .expect("prefix bridge");
    tracker
        .record_ack_bound(WalShippingAck::new(2, Lsn::new(100)))
        .expect("ack is valid once prefix from current safe lsn is complete");
}
