use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::{Lsn, WalReplicaSafeLsnBoundaryProvider};
use std::collections::{BTreeMap, BTreeSet};

use super::WalShipmentRange;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalShippingAckBindingRejection {
    ReplicaIdZero,
    ReplicaNotRequired,
    NoValidatedRange,
    InvalidValidatedRange,
    AckExceedsShippedMax,
    AckOutsideDurablePrefix,
    AckBeforeDurablePrefix,
    StaleAck,
}

impl WalShippingAckBindingRejection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReplicaIdZero => "WAL shipping replica id must not be zero",
            Self::ReplicaNotRequired => {
                "WAL shipping ACK came from a replica not required for retention"
            },
            Self::NoValidatedRange => "WAL shipping ACK has no validated range evidence",
            Self::InvalidValidatedRange => "WAL shipping validated range is invalid",
            Self::AckExceedsShippedMax => "WAL shipping ACK exceeds the replica shipped max LSN",
            Self::AckOutsideDurablePrefix => {
                "WAL shipping ACK exceeds the contiguous validated durable prefix"
            },
            Self::AckBeforeDurablePrefix => {
                "WAL shipping ACK predates the contiguous validated durable prefix"
            },
            Self::StaleAck => "WAL shipping ACK is stale or non-monotonic",
        }
    }

    pub fn into_error(self) -> AndromedaError {
        AndromedaError::new(AndromedaErrorKind::Storage, self.as_str())
    }
}

impl std::fmt::Display for WalShippingAckBindingRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::error::Error for WalShippingAckBindingRejection {}

/// Tracks the durable shipping floor for replicas required by retention.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WalReplicaSafeLsnTracker {
    required_replicas: BTreeSet<u64>,
    safe_lsn_by_replica: BTreeMap<u64, Lsn>,
    validated_ranges_by_replica: BTreeMap<u64, ReplicaValidatedRangeState>,
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
            return Err(WalShippingAckBindingRejection::ReplicaIdZero.into_error());
        }
        self.required_replicas.insert(replica_id);
        self.safe_lsn_by_replica
            .entry(replica_id)
            .or_insert(Lsn::ZERO);
        self.validated_ranges_by_replica
            .entry(replica_id)
            .or_default();
        Ok(())
    }

    pub fn record_ack(&mut self, ack: WalShippingAck) -> AndromedaResult<Lsn> {
        self.record_ack_bound(ack)
            .map_err(WalShippingAckBindingRejection::into_error)
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

    pub fn record_validated_range(
        &mut self,
        replica_id: u64,
        range: WalShipmentRange,
    ) -> Result<(), WalShippingAckBindingRejection> {
        self.ensure_required_replica(replica_id)?;
        let state = self
            .validated_ranges_by_replica
            .get_mut(&replica_id)
            .ok_or(WalShippingAckBindingRejection::NoValidatedRange)?;
        state.insert_range(range)?;
        Ok(())
    }

    pub fn validate_ack_binding(
        &self,
        ack: WalShippingAck,
    ) -> Result<(), WalShippingAckBindingRejection> {
        self.ensure_required_replica(ack.replica_id)?;
        let state = self
            .validated_ranges_by_replica
            .get(&ack.replica_id)
            .ok_or(WalShippingAckBindingRejection::NoValidatedRange)?;
        if state.is_empty() {
            return Err(WalShippingAckBindingRejection::NoValidatedRange);
        }
        if ack.safe_lsn > state.shipped_max {
            return Err(WalShippingAckBindingRejection::AckExceedsShippedMax);
        }
        let (prefix_start, prefix_end) = state
            .contiguous_durable_prefix()
            .ok_or(WalShippingAckBindingRejection::NoValidatedRange)?;
        let current_safe_lsn = self
            .safe_lsn_by_replica
            .get(&ack.replica_id)
            .copied()
            .unwrap_or(Lsn::ZERO);
        let next_required_lsn = current_safe_lsn
            .try_next()
            .map_err(|_| WalShippingAckBindingRejection::InvalidValidatedRange)?;
        if prefix_start > next_required_lsn {
            return Err(WalShippingAckBindingRejection::AckOutsideDurablePrefix);
        }
        if ack.safe_lsn < prefix_start {
            return Err(WalShippingAckBindingRejection::AckBeforeDurablePrefix);
        }
        if ack.safe_lsn > prefix_end {
            return Err(WalShippingAckBindingRejection::AckOutsideDurablePrefix);
        }
        Ok(())
    }

    pub fn record_ack_bound(
        &mut self,
        ack: WalShippingAck,
    ) -> Result<Lsn, WalShippingAckBindingRejection> {
        self.validate_ack_binding(ack)?;
        let safe_lsn = self
            .safe_lsn_by_replica
            .entry(ack.replica_id)
            .or_insert(Lsn::ZERO);
        if ack.safe_lsn <= *safe_lsn {
            return Err(WalShippingAckBindingRejection::StaleAck);
        }
        *safe_lsn = ack.safe_lsn;
        Ok(*safe_lsn)
    }

    fn ensure_required_replica(
        &self,
        replica_id: u64,
    ) -> Result<(), WalShippingAckBindingRejection> {
        if replica_id == 0 {
            return Err(WalShippingAckBindingRejection::ReplicaIdZero);
        }
        if !self.required_replicas.contains(&replica_id) {
            return Err(WalShippingAckBindingRejection::ReplicaNotRequired);
        }
        Ok(())
    }
}

impl WalReplicaSafeLsnBoundaryProvider for WalReplicaSafeLsnTracker {
    fn retention_boundary_lsn(&self) -> Lsn {
        WalReplicaSafeLsnTracker::retention_boundary_lsn(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ReplicaValidatedRangeState {
    /// Normalized, merged map of start_lsn -> end_lsn inclusive.
    merged_ranges: BTreeMap<u64, u64>,
    shipped_max: Lsn,
}

impl ReplicaValidatedRangeState {
    fn is_empty(&self) -> bool {
        self.merged_ranges.is_empty()
    }

    fn contiguous_durable_prefix(&self) -> Option<(Lsn, Lsn)> {
        self.merged_ranges
            .first_key_value()
            .map(|(start, end)| (Lsn::new(*start), Lsn::new(*end)))
    }

    fn insert_range(
        &mut self,
        range: WalShipmentRange,
    ) -> Result<(), WalShippingAckBindingRejection> {
        validate_shipment_range(range)?;
        let mut merged_start = range.first.get();
        let mut merged_end = range.last.get();

        if let Some((&candidate_start, &candidate_end)) =
            self.merged_ranges.range(..=merged_start).next_back()
            && are_connected(candidate_start, candidate_end, merged_start, merged_end)
        {
            merged_start = candidate_start.min(merged_start);
            merged_end = candidate_end.max(merged_end);
            self.merged_ranges.remove(&candidate_start);
        }

        loop {
            let next = self
                .merged_ranges
                .range(merged_start..)
                .next()
                .map(|(&start, &end)| (start, end));
            let Some((next_start, next_end)) = next else {
                break;
            };
            if are_connected(merged_start, merged_end, next_start, next_end) {
                merged_start = merged_start.min(next_start);
                merged_end = merged_end.max(next_end);
                self.merged_ranges.remove(&next_start);
                continue;
            }
            break;
        }

        self.merged_ranges.insert(merged_start, merged_end);
        if range.last > self.shipped_max {
            self.shipped_max = range.last;
        }
        Ok(())
    }
}

fn validate_shipment_range(range: WalShipmentRange) -> Result<(), WalShippingAckBindingRejection> {
    if range.count == 0 || range.first > range.last {
        return Err(WalShippingAckBindingRejection::InvalidValidatedRange);
    }
    let span = range
        .last
        .get()
        .checked_sub(range.first.get())
        .and_then(|delta| delta.checked_add(1))
        .ok_or(WalShippingAckBindingRejection::InvalidValidatedRange)?;
    let count = u64::try_from(range.count)
        .map_err(|_| WalShippingAckBindingRejection::InvalidValidatedRange)?;
    if span != count {
        return Err(WalShippingAckBindingRejection::InvalidValidatedRange);
    }
    Ok(())
}

fn are_connected(left_start: u64, left_end: u64, right_start: u64, right_end: u64) -> bool {
    let left_end_plus_one = left_end.saturating_add(1);
    let right_end_plus_one = right_end.saturating_add(1);
    left_start <= right_end_plus_one && right_start <= left_end_plus_one
}
