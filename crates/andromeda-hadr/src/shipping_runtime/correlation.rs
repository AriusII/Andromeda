use crate::Lsn;
use std::collections::HashMap;

/// LSN correlation state: tracks what the primary has shipped and what replicas have received.
///
/// At the primary:
/// - `wal_shipped_lsn[replica_id]` = highest LSN sent to that replica.
///
/// At a replica:
/// - `wal_received_lsn` = highest LSN received, validated, and durably appended.
///
/// The replica's lag is computed as `primary_durable_lsn - replica_received_lsn`.
#[derive(Debug, Clone)]
pub struct LsnCorrelationState {
    /// Per-replica shipping position (what we've sent to each).
    pub shipped_lsn_by_replica: HashMap<u64, Lsn>,
    /// Replica's received position (what it has told us it received).
    pub replica_received_lsn: Lsn,
}

impl LsnCorrelationState {
    pub fn new() -> Self {
        Self {
            shipped_lsn_by_replica: HashMap::new(),
            replica_received_lsn: Lsn::ZERO,
        }
    }

    /// Record that we've shipped records up to this LSN to a specific replica.
    pub fn update_shipped(&mut self, replica_id: u64, lsn: Lsn) {
        self.shipped_lsn_by_replica.insert(replica_id, lsn);
    }

    /// Record that the replica has told us it received up to this LSN.
    pub fn update_received(&mut self, lsn: Lsn) {
        if lsn > self.replica_received_lsn {
            self.replica_received_lsn = lsn;
        }
    }

    /// Compute lag: how far behind is this replica?
    pub fn replica_lag(&self, primary_durable_lsn: Lsn) -> Lsn {
        if self.replica_received_lsn > primary_durable_lsn {
            Lsn::ZERO
        } else {
            // Simulate lag as bytes; in practice this is a numeric difference.
            Lsn::new(primary_durable_lsn.get() - self.replica_received_lsn.get())
        }
    }

    /// Is this replica fully caught up?
    pub fn is_caught_up(&self, primary_durable_lsn: Lsn) -> bool {
        self.replica_lag(primary_durable_lsn) == Lsn::ZERO
    }
}

impl Default for LsnCorrelationState {
    fn default() -> Self {
        Self::new()
    }
}
