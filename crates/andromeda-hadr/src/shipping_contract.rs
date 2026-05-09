//! V0 single-primary WAL shipping contract.
//!
//! This module owns the transport-independent WAL shipping boundary:
//! identities, roles, batch envelopes, ACK tracking, and validation results.
//! Runtime dispatch and quorum coordination live in adjacent HADR modules.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::{Lsn, WalRecord, WalReplicaSafeLsnBoundaryProvider};
use std::collections::{BTreeMap, BTreeSet};

/// Topology role of a WAL node in the V0 single-primary design.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WalNodeRole {
    /// Sole accepting node. Only role permitted to ship WAL.
    Primary,
    /// Read-only follower. Only role permitted to receive shipped WAL.
    Replica,
}

impl WalNodeRole {
    pub const fn is_primary(self) -> bool {
        matches!(self, Self::Primary)
    }

    pub const fn is_replica(self) -> bool {
        matches!(self, Self::Replica)
    }
}

/// Stable identity of a WAL participant. The numeric id is opaque; identity
/// equality is by `(id, role)` so a node cannot silently change role mid-ship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WalNodeIdentity {
    id: u64,
    role: WalNodeRole,
}

impl WalNodeIdentity {
    pub const fn new(id: u64, role: WalNodeRole) -> Self {
        Self { id, role }
    }

    pub const fn id(self) -> u64 {
        self.id
    }

    pub const fn role(self) -> WalNodeRole {
        self.role
    }
}

/// LSN range covered by a shipment, inclusive on both ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalShipmentRange {
    pub first: Lsn,
    pub last: Lsn,
    pub count: usize,
}

/// Successful validation outcome for a shipment batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalShipmentAccepted {
    pub range: WalShipmentRange,
    /// LSN the replica should advertise as its next expected head after this
    /// batch is applied. Equal to `range.last.next()`.
    pub next_expected_lsn: Lsn,
}

/// ACK emitted by a replica after durably receiving a shipment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalShippingAck {
    pub replica_id: u64,
    pub safe_lsn: Lsn,
}

impl WalShippingAck {
    pub const fn new(replica_id: u64, safe_lsn: Lsn) -> Self {
        Self {
            replica_id,
            safe_lsn,
        }
    }
}

/// Tracks the durable shipping floor for replicas required by retention.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WalReplicaSafeLsnTracker {
    required_replicas: BTreeSet<u64>,
    safe_lsn_by_replica: BTreeMap<u64, Lsn>,
}

impl WalReplicaSafeLsnTracker {
    pub fn new(required_replicas: impl IntoIterator<Item = u64>) -> AndromedaResult<Self> {
        let mut tracker = Self::default();
        for replica_id in required_replicas {
            tracker.register_required_replica(replica_id)?;
        }
        Ok(tracker)
    }

    pub fn register_required_replica(&mut self, replica_id: u64) -> AndromedaResult<()> {
        if replica_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL required replica id must not be zero",
            ));
        }
        self.required_replicas.insert(replica_id);
        self.safe_lsn_by_replica
            .entry(replica_id)
            .or_insert(Lsn::ZERO);
        Ok(())
    }

    pub fn record_ack(&mut self, ack: WalShippingAck) -> AndromedaResult<Lsn> {
        if !self.required_replicas.contains(&ack.replica_id) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL shipping ACK came from a replica not required for retention",
            ));
        }

        let safe_lsn = self
            .safe_lsn_by_replica
            .entry(ack.replica_id)
            .or_insert(Lsn::ZERO);
        if ack.safe_lsn > *safe_lsn {
            *safe_lsn = ack.safe_lsn;
        }
        Ok(*safe_lsn)
    }

    pub fn required_replicas(&self) -> &BTreeSet<u64> {
        &self.required_replicas
    }

    pub fn replica_safe_lsn(&self, replica_id: u64) -> Option<Lsn> {
        self.safe_lsn_by_replica.get(&replica_id).copied()
    }

    pub fn min_required_safe_lsn(&self) -> Option<Lsn> {
        self.required_replicas
            .iter()
            .filter_map(|replica_id| self.safe_lsn_by_replica.get(replica_id).copied())
            .min()
    }

    pub fn retention_boundary_lsn(&self) -> Lsn {
        self.min_required_safe_lsn().unwrap_or(Lsn::MAX)
    }

    pub fn all_required_replicas_have_shipped(&self, segment_end_lsn: Lsn) -> bool {
        self.required_replicas.iter().all(|replica_id| {
            self.safe_lsn_by_replica
                .get(replica_id)
                .is_some_and(|safe_lsn| *safe_lsn >= segment_end_lsn)
        })
    }
}

impl WalReplicaSafeLsnBoundaryProvider for WalReplicaSafeLsnTracker {
    fn retention_boundary_lsn(&self) -> Lsn {
        WalReplicaSafeLsnTracker::retention_boundary_lsn(self)
    }
}

/// Replica-side expectation describing where the receiver currently stands.
///
/// `tail_lsn = None` means the replica has no records yet (genesis); in that
/// case `expected_next_lsn` is the very first LSN it expects to receive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalReplicaExpectation {
    pub tail_lsn: Option<Lsn>,
    pub expected_next_lsn: Lsn,
}

impl WalReplicaExpectation {
    pub const fn genesis(first_expected: Lsn) -> Self {
        Self {
            tail_lsn: None,
            expected_next_lsn: first_expected,
        }
    }

    pub const fn after(tail: Lsn, expected_next: Lsn) -> Self {
        Self {
            tail_lsn: Some(tail),
            expected_next_lsn: expected_next,
        }
    }
}

/// Typed shipment envelope. Carries borrowed records so the contract does
/// not impose ownership on the runtime layer.
#[derive(Debug, Clone, Copy)]
pub struct WalShipmentBatch<'a> {
    pub source: WalNodeIdentity,
    pub target: WalNodeIdentity,
    pub expectation: WalReplicaExpectation,
    pub records: &'a [WalRecord],
}

impl<'a> WalShipmentBatch<'a> {
    pub const fn new(
        source: WalNodeIdentity,
        target: WalNodeIdentity,
        expectation: WalReplicaExpectation,
        records: &'a [WalRecord],
    ) -> Self {
        Self {
            source,
            target,
            expectation,
            records,
        }
    }

    /// Validate the shipment against the V0 single-primary contract.
    pub fn validate(&self) -> AndromedaResult<WalShipmentAccepted> {
        if self.source.id() == 0 {
            return Err(WalShipmentRejection::SourceIdZero.into_error());
        }
        if self.target.id() == 0 {
            return Err(WalShipmentRejection::TargetIdZero.into_error());
        }
        if !self.source.role().is_primary() {
            return Err(WalShipmentRejection::SourceNotPrimary.into_error());
        }
        if !self.target.role().is_replica() {
            return Err(WalShipmentRejection::TargetNotReplica.into_error());
        }
        if self.records.is_empty() {
            return Err(WalShipmentRejection::EmptyBatch.into_error());
        }
        if let Some(tail_lsn) = self.expectation.tail_lsn {
            let expected_next = tail_lsn
                .try_next()
                .map_err(|_| WalShipmentRejection::ReplicaExpectationOverflow.into_error())?;
            if self.expectation.expected_next_lsn != expected_next {
                return Err(WalShipmentRejection::ReplicaExpectationMismatch.into_error());
            }
        }

        let first = &self.records[0];
        first
            .validate()
            .map_err(|_| WalShipmentRejection::RecordSelfInvalid.into_error())?;

        if first.header.lsn != self.expectation.expected_next_lsn {
            return Err(WalShipmentRejection::UnexpectedFirstLsn.into_error());
        }
        if first.header.previous_lsn != self.expectation.tail_lsn {
            return Err(WalShipmentRejection::PreviousLsnMismatch.into_error());
        }

        let mut prev_lsn = first.header.lsn;
        for record in &self.records[1..] {
            record
                .validate()
                .map_err(|_| WalShipmentRejection::RecordSelfInvalid.into_error())?;

            if record.header.lsn <= prev_lsn {
                return Err(WalShipmentRejection::NonMonotonicLsn.into_error());
            }
            let expected_lsn = prev_lsn
                .try_next()
                .map_err(|_| WalShipmentRejection::ReplicaExpectationOverflow.into_error())?;
            if record.header.lsn != expected_lsn {
                return Err(WalShipmentRejection::ChainGap.into_error());
            }
            match record.header.previous_lsn {
                Some(linked) if linked == prev_lsn => {},
                _ => return Err(WalShipmentRejection::ChainGap.into_error()),
            }
            prev_lsn = record.header.lsn;
        }

        let last_lsn = prev_lsn;
        let next_expected_lsn = last_lsn
            .try_next()
            .map_err(|_| WalShipmentRejection::ReplicaExpectationOverflow.into_error())?;
        Ok(WalShipmentAccepted {
            range: WalShipmentRange {
                first: first.header.lsn,
                last: last_lsn,
                count: self.records.len(),
            },
            next_expected_lsn,
        })
    }
}

/// Categorical reason a shipment was rejected. Variants are deliberately
/// narrow so upstream tooling can map each to a specific operator action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalShipmentRejection {
    /// Source identity has the reserved zero id.
    SourceIdZero,
    /// Target identity has the reserved zero id.
    TargetIdZero,
    /// Source identity is not a primary.
    SourceNotPrimary,
    /// Target identity is not a replica.
    TargetNotReplica,
    /// Replica tail and expected next LSN do not form one contiguous chain.
    ReplicaExpectationMismatch,
    /// Replica tail cannot advance to a valid expected next LSN.
    ReplicaExpectationOverflow,
    /// Empty batch: shipping must carry at least one record.
    EmptyBatch,
    /// First record's LSN does not match the replica's expected next LSN.
    UnexpectedFirstLsn,
    /// First record's `previous_lsn` does not link to the replica's prior tail.
    PreviousLsnMismatch,
    /// LSN went backwards or stayed equal between adjacent records.
    NonMonotonicLsn,
    /// Adjacent record's `previous_lsn` does not equal the prior record's LSN.
    ChainGap,
    /// A record failed structural self-validation.
    RecordSelfInvalid,
}

impl WalShipmentRejection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceIdZero => "wal shipment source id must not be zero",
            Self::TargetIdZero => "wal shipment target id must not be zero",
            Self::SourceNotPrimary => "wal shipment source is not a primary",
            Self::TargetNotReplica => "wal shipment target is not a replica",
            Self::ReplicaExpectationMismatch => {
                "wal shipment replica expectation does not match the replica tail"
            },
            Self::ReplicaExpectationOverflow => {
                "wal shipment replica expectation would overflow the LSN space"
            },
            Self::EmptyBatch => "wal shipment batch is empty",
            Self::UnexpectedFirstLsn => {
                "wal shipment first LSN does not match replica expected next LSN"
            },
            Self::PreviousLsnMismatch => {
                "wal shipment first record previous_lsn does not link to replica tail"
            },
            Self::NonMonotonicLsn => "wal shipment contains duplicate or reordered LSNs",
            Self::ChainGap => "wal shipment contains a gap in the previous_lsn chain",
            Self::RecordSelfInvalid => "wal shipment contains a structurally invalid record",
        }
    }

    fn into_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Storage, self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_error::AndromedaErrorKind;
    use andromeda_wal::{WalRecord, WalRecordKind};

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
}
