use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::write_ahead_log::WalReplicaSafeLsnBoundaryProvider;
use std::collections::{BTreeMap, BTreeSet};

use crate::Lsn;

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
