use andromeda_error::AndromedaResult;
use andromeda_wal::{Lsn, WalRecord};

use super::{identity::WalNodeIdentity, rejection::WalShipmentRejection};

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
