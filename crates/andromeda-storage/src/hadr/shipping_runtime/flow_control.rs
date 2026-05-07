use crate::Lsn;

/// Segment shipping condition: a segment is shippable when all its records are durable.
///
/// The primary's WriterThread flushes records to durable storage. The ShippingThread
/// polls durable_lsn and ships segments whose end_lsn <= durable_lsn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShippingCondition {
    /// Highest LSN known to be durable at the primary.
    pub primary_durable_lsn: Lsn,
    /// Highest LSN the segment covers.
    pub segment_end_lsn: Lsn,
}

impl ShippingCondition {
    pub fn is_shippable(&self) -> bool {
        self.segment_end_lsn <= self.primary_durable_lsn
    }
}

/// Replica backpressure request: signal to primary that replica needs older segments.
///
/// If a replica falls behind (e.g., due to network lag or slow I/O), it can request
/// replaying an earlier segment. The primary ships it again; the replica re-validates
/// and re-applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShippingBackpressureRequest {
    /// Replica's current head: the highest LSN it has received and validated.
    pub replica_received_lsn: Lsn,
    /// Next LSN the replica expects to receive.
    pub replica_expected_next_lsn: Lsn,
}

impl ShippingBackpressureRequest {
    pub fn new(received_lsn: Lsn, expected_next_lsn: Lsn) -> Self {
        Self {
            replica_received_lsn: received_lsn,
            replica_expected_next_lsn: expected_next_lsn,
        }
    }
}
