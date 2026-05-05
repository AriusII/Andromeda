//! F1 — WAL Shipping Runtime Model and Protocol
//!
//! # Overview
//!
//! This module designs the WAL shipping runtime for Andromeda's single-primary HA/DR architecture.
//! The runtime coordinates two data flow paths:
//!
//! **Primary side:**
//! - `WriterThread` appends WAL records to durable storage.
//! - `ShippingThread` reads committed segments (LSN ≥ visible_lsn) and sends them to replicas
//!   via QUIC streams.
//! - `wal_shipped_lsn` tracks what has been sent to each replica.
//!
//! **Replica side:**
//! - `ReceivingThread` reads QUIC stream, validates checksums, and appends to local WAL.
//! - `wal_received_lsn` tracks what has been durably received and validated.
//! - Replica enforces replication consistency mode (async vs. sync) via LSN lag signals.
//!
//! # Doctrine Invariants
//!
//! 1. **Durability-before-visibility:** A segment is only shipped if all its records are durable
//!    (LSN ≤ durable_lsn at primary).
//! 2. **Cryptographic integrity:** Every segment carries a checksum. Replica validation rejects
//!    corrupted data with fencing.
//! 3. **LSN chain contiguity:** Replicas enforce previous_lsn linkage; gap detection triggers
//!    connection fencing and replay backpressure.
//! 4. **Deterministic state:** shipped_lsn and received_lsn are explicit, queryable, and
//!    immutable on successful persistence.
//! 5. **No silent failures:** All errors (checksum mismatch, chain gap, connection loss) produce
//!    auditable trace events and fencing decisions.
//!
//! # Design Scope
//!
//! This is a **design-only** deliverable. The runtime model is specified here as types and
//! pure decision functions. Implementation and async wiring are deferred to later phases.
//!
//! - ✓ Segment shipping protocol (identity, range, checksum, validation).
//! - ✓ LSN correlation model (shipped_lsn, received_lsn, lag tracking).
//! - ✓ Replica backpressure (request-replay protocol).
//! - ✓ Fencing decision point (connection loss → quorum policy check).
//! - ✗ Actual QUIC stream I/O (use mock in tests).
//! - ✗ Quorum consensus algorithm (designed in F3).
//! - ✗ Compression/encryption (future scope).

use crate::{write_ahead_log::*, Lsn};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use std::collections::HashMap;

/// Segment identity and metadata for shipping protocol.
///
/// This struct describes a single WAL segment as it moves from primary to replicas.
/// It is immutable once created and carries all metadata required for validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShippingSegmentDescriptor {
    /// Unique identifier for this segment (monotonically increasing).
    pub segment_id: u64,
    /// First LSN in this segment (inclusive).
    pub start_lsn: Lsn,
    /// Last LSN in this segment (inclusive).
    pub end_lsn: Lsn,
    /// Number of records in this segment.
    pub record_count: usize,
    /// Checksum of all segment record bytes (FNV-1a 64-bit).
    pub checksum: u64,
}

impl ShippingSegmentDescriptor {
    pub fn new(
        segment_id: u64,
        start_lsn: Lsn,
        end_lsn: Lsn,
        record_count: usize,
        checksum: u64,
    ) -> Self {
        Self {
            segment_id,
            start_lsn,
            end_lsn,
            record_count,
            checksum,
        }
    }

    /// Compute a segment checksum from a slice of WAL records.
    /// Uses FNV-1a folding of record bytes.
    pub fn compute_checksum(records: &[WalRecord]) -> u64 {
        const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

        let mut state = FNV_OFFSET;

        for record in records {
            // Fold record header fields
            for byte in (record.header.kind as u64).to_le_bytes() {
                state ^= u64::from(byte);
                state = state.wrapping_mul(FNV_PRIME);
            }
            for byte in record.header.lsn.get().to_le_bytes() {
                state ^= u64::from(byte);
                state = state.wrapping_mul(FNV_PRIME);
            }
            // Fold payload bytes
            for byte in &record.payload {
                state ^= u64::from(*byte);
                state = state.wrapping_mul(FNV_PRIME);
            }
        }

        if state == 0 {
            1
        } else {
            state
        }
    }
}

/// Segment shipping envelope: descriptor + borrowed record bytes.
///
/// This is the atomic unit sent from primary to replica over QUIC.
/// The replica receives this, validates the checksum, and appends records to its local WAL.
#[derive(Debug, Clone, Copy)]
pub struct ShippingSegmentEnvelope<'a> {
    pub descriptor: ShippingSegmentDescriptor,
    pub records: &'a [WalRecord],
}

impl<'a> ShippingSegmentEnvelope<'a> {
    pub fn new(descriptor: ShippingSegmentDescriptor, records: &'a [WalRecord]) -> Self {
        Self {
            descriptor,
            records,
        }
    }

    /// Validate the envelope structure (checksum, record count consistency).
    /// Does NOT validate LSN chain; that is the replica's concern.
    pub fn validate_structure(&self) -> AndromedaResult<()> {
        // Check record count matches
        if self.descriptor.record_count != self.records.len() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "shipping segment record count mismatch",
            ));
        }

        // Check LSN range is non-empty
        if self.descriptor.start_lsn > self.descriptor.end_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "shipping segment LSN range invalid",
            ));
        }

        // Check records are non-empty
        if self.records.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "shipping segment has no records",
            ));
        }

        // Check first and last LSN match descriptor
        if self.records[0].header.lsn != self.descriptor.start_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "shipping segment first LSN mismatch",
            ));
        }
        if self.records[self.records.len() - 1].header.lsn != self.descriptor.end_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "shipping segment last LSN mismatch",
            ));
        }

        Ok(())
    }

    /// Validate the segment checksum against the records.
    pub fn validate_checksum(&self) -> AndromedaResult<()> {
        let computed = ShippingSegmentDescriptor::compute_checksum(self.records);
        if computed != self.descriptor.checksum {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "shipping segment checksum mismatch",
            ));
        }
        Ok(())
    }

    /// Full envelope validation: structure + checksum.
    pub fn validate(&self) -> AndromedaResult<()> {
        self.validate_structure()?;
        self.validate_checksum()?;
        Ok(())
    }
}

/// Segment shipping condition: a segment is shippable when all its records are durable.
///
/// The primary's WriterThread flushes records to durable storage. The ShippingThread
/// polls durable_lsn and ships segments whose end_lsn ≤ durable_lsn.
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
            // Simulate lag as bytes; in practice this is a numeric difference
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

/// Fencing decision at the primary: should we block commits if a replica is lost?
///
/// The quorum policy determines replication mode:
/// - Single replica: async (continue writing even if replica is unreachable).
/// - 2+ replicas: quorum (block writes until a quorum of replicas acknowledge).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencingPolicy {
    /// Asynchronous: continue writing regardless of replica ack. Used when replica count = 1.
    Asynchronous,
    /// Quorum-enforced: wait for a majority of replicas to acknowledge before visibility.
    /// Used when replica count >= 2.
    QuorumEnforced,
}

impl FencingPolicy {
    /// Select fencing policy based on replica count.
    pub fn for_replica_count(replica_count: usize) -> Self {
        if replica_count >= 2 {
            FencingPolicy::QuorumEnforced
        } else {
            FencingPolicy::Asynchronous
        }
    }
}

/// Fencing event: what happened when we tried to ship to a replica and it failed?
///
/// The primary emits these as trace events. They inform the promotion/demotion flow
/// and may trigger operational alerts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FencingEvent {
    /// Replica connection lost. No ack received for the last segment sent.
    ReplicaConnectionLost,
    /// Replica returned a checksum mismatch for a segment we sent.
    ReplicaChecksumMismatch,
    /// Replica reported a gap in the LSN chain. Possible data corruption.
    ReplicaChainGap,
    /// Replica returned an error we don't understand. Safe to assume it's fenced.
    ReplicaUnknownError,
}

impl FencingEvent {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReplicaConnectionLost => "replica connection lost",
            Self::ReplicaChecksumMismatch => "replica checksum mismatch",
            Self::ReplicaChainGap => "replica chain gap detected",
            Self::ReplicaUnknownError => "replica unknown error",
        }
    }
}

/// Fencing decision: given a fencing event and the quorum policy, decide what to do.
///
/// Returns `true` if we should block transaction visibility (synchronous mode).
/// Returns `false` if we should continue (asynchronous mode).
pub fn decide_fencing(_event: FencingEvent, policy: FencingPolicy) -> bool {
    match policy {
        FencingPolicy::Asynchronous => {
            // Async mode: continue writing even if replica fails.
            false
        }
        FencingPolicy::QuorumEnforced => {
            // Quorum mode: block visibility on any replica failure.
            // (The actual quorum algorithm is in F3; this is the decision point.)
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::write_ahead_log::record::{WalRecord, WalRecordKind};

    fn sample_record(lsn: u64, prev: Option<u64>) -> WalRecord {
        WalRecord::from_parts(
            WalRecordKind::PageAllocate,
            Lsn::new(lsn),
            prev.map(Lsn::new),
            None,
            vec![42u8; 64], // Dummy payload
        )
        .expect("record builds")
    }

    /// Test 1: Segment committed → ShippingThread reads → sends via QUIC
    ///
    /// Simulates:
    /// - Primary WriterThread appends records LSN 1-10 and flushes.
    /// - ShippingThread sees durable_lsn = 10.
    /// - Creates a ShippingSegmentEnvelope for LSN 1-10.
    /// - Validates structure and checksum.
    /// - (Transport would send via QUIC; we validate the envelope here.)
    #[test]
    fn test_shipping_thread_reads_committed_segments() {
        // Arrange: create a committed segment with 5 records
        let records = vec![
            sample_record(1, None),
            sample_record(2, Some(1)),
            sample_record(3, Some(2)),
            sample_record(4, Some(3)),
            sample_record(5, Some(4)),
        ];

        let descriptor = ShippingSegmentDescriptor {
            segment_id: 1,
            start_lsn: Lsn::new(1),
            end_lsn: Lsn::new(5),
            record_count: 5,
            checksum: ShippingSegmentDescriptor::compute_checksum(&records),
        };

        let envelope = ShippingSegmentEnvelope::new(descriptor, &records);

        // Act: validate the envelope
        let result = envelope.validate();

        // Assert: envelope is valid and ready for shipping
        assert!(result.is_ok(), "committed segment should be valid");

        // Check shipping condition: if durable_lsn >= 5, it's shippable
        let condition = ShippingCondition {
            primary_durable_lsn: Lsn::new(5),
            segment_end_lsn: Lsn::new(5),
        };
        assert!(condition.is_shippable(), "segment should be shippable");

        // Not shippable if only durable up to LSN 3
        let condition_early = ShippingCondition {
            primary_durable_lsn: Lsn::new(3),
            segment_end_lsn: Lsn::new(5),
        };
        assert!(!condition_early.is_shippable(), "segment not yet durable");
    }

    /// Test 2: Replica receives segment → validates checksum → appends to local WAL
    ///
    /// Simulates:
    /// - Primary sends ShippingSegmentEnvelope with LSN 6-10.
    /// - Replica ReceivingThread receives it.
    /// - Validates checksum (should pass).
    /// - Appends records to local WAL via WalShipmentBatch validation.
    #[test]
    fn test_replica_receives_validates_appends() {
        // Arrange: create a segment for LSN 6-10 (following LSN 5)
        let records = vec![
            sample_record(6, Some(5)),
            sample_record(7, Some(6)),
            sample_record(8, Some(7)),
            sample_record(9, Some(8)),
            sample_record(10, Some(9)),
        ];

        let descriptor = ShippingSegmentDescriptor {
            segment_id: 2,
            start_lsn: Lsn::new(6),
            end_lsn: Lsn::new(10),
            record_count: 5,
            checksum: ShippingSegmentDescriptor::compute_checksum(&records),
        };

        let envelope = ShippingSegmentEnvelope::new(descriptor, &records);

        // Act: validate structure and checksum
        let struct_result = envelope.validate_structure();
        let checksum_result = envelope.validate_checksum();

        // Assert: both pass
        assert!(struct_result.is_ok(), "structure should be valid");
        assert!(checksum_result.is_ok(), "checksum should be valid");

        // Now validate via WalShipmentBatch (replica side)
        let primary = WalNodeIdentity::new(1, WalNodeRole::Primary);
        let replica = WalNodeIdentity::new(2, WalNodeRole::Replica);
        let expectation =
            crate::write_ahead_log::WalReplicaExpectation::after(Lsn::new(5), Lsn::new(6));

        let batch = WalShipmentBatch::new(primary, replica, expectation, &records);
        let accepted = batch.validate().expect("batch should be accepted");

        assert_eq!(accepted.range.first, Lsn::new(6));
        assert_eq!(accepted.range.last, Lsn::new(10));
        assert_eq!(accepted.next_expected_lsn, Lsn::new(11));
    }

    /// Test 3: Shipping backpressure — replica falls behind → requests earlier segment
    ///
    /// Simulates:
    /// - Primary has shipped up to LSN 50.
    /// - Replica has only received up to LSN 20 (network lag).
    /// - Replica emits ShippingBackpressureRequest(20, 21).
    /// - Primary can re-ship segment covering LSN 21-50.
    #[test]
    fn test_shipping_backpressure_handles_replica_lag() {
        // Arrange: primary shipping state
        let mut lsn_correlation = LsnCorrelationState::new();
        let replica_id = 2u64;

        // Primary has shipped LSN 1-50
        lsn_correlation.update_shipped(replica_id, Lsn::new(50));
        // Replica has only received LSN 1-20 (lag of 30 LSNs)
        lsn_correlation.update_received(Lsn::new(20));

        // Act: compute lag
        let primary_durable = Lsn::new(50);
        let lag = lsn_correlation.replica_lag(primary_durable);

        // Assert: replica is behind
        assert_eq!(lag, Lsn::new(30), "replica should be 30 LSNs behind");
        assert!(
            !lsn_correlation.is_caught_up(primary_durable),
            "replica not caught up"
        );

        // Replica sends backpressure request
        let backpressure = ShippingBackpressureRequest::new(Lsn::new(20), Lsn::new(21));
        assert_eq!(
            backpressure.replica_received_lsn,
            Lsn::new(20),
            "replica has received up to 20"
        );
        assert_eq!(
            backpressure.replica_expected_next_lsn,
            Lsn::new(21),
            "replica expects 21 next"
        );

        // Primary would then re-ship a segment starting at LSN 21
        // (or earlier if there's a gap). This test just validates the backpressure
        // request structure.
    }

    /// Test 4: Fencing decision on connection loss
    ///
    /// Simulates:
    /// - Primary loses connection to a replica.
    /// - FencingEvent::ReplicaConnectionLost is emitted.
    /// - Fencing policy determines whether to block or continue.
    ///
    /// Async mode (single replica): continue writing.
    /// Quorum mode (2+ replicas): block visibility until quorum is restored.
    #[test]
    fn test_shipping_detects_fencing_on_connection_loss() {
        // Arrange: single-replica cluster (async mode)
        let async_policy = FencingPolicy::for_replica_count(1);
        assert_eq!(async_policy, FencingPolicy::Asynchronous);

        // Act: connection lost
        let event = FencingEvent::ReplicaConnectionLost;
        let should_fence = decide_fencing(event, async_policy);

        // Assert: async mode does not fence
        assert!(
            !should_fence,
            "async mode should continue on connection loss"
        );

        // Now arrange: 2-replica cluster (quorum mode)
        let quorum_policy = FencingPolicy::for_replica_count(2);
        assert_eq!(quorum_policy, FencingPolicy::QuorumEnforced);

        // Act: connection lost (same event)
        let should_fence_quorum = decide_fencing(event, quorum_policy);

        // Assert: quorum mode fences on connection loss
        assert!(
            should_fence_quorum,
            "quorum mode should fence on connection loss"
        );

        // Trace event is emitted for observability
        assert_eq!(
            event.as_str(),
            "replica connection lost",
            "trace event message is correct"
        );
    }

    /// Bonus: Test LSN correlation state tracking
    #[test]
    fn test_lsn_correlation_state_tracking() {
        let mut correlation = LsnCorrelationState::new();

        // Initially, nothing shipped or received
        assert_eq!(correlation.replica_received_lsn, Lsn::ZERO);
        assert!(correlation.shipped_lsn_by_replica.is_empty());

        // Primary ships to replica 1 up to LSN 100
        correlation.update_shipped(1, Lsn::new(100));
        assert_eq!(
            correlation.shipped_lsn_by_replica.get(&1),
            Some(&Lsn::new(100))
        );

        // Primary ships to replica 2 up to LSN 50
        correlation.update_shipped(2, Lsn::new(50));
        assert_eq!(
            correlation.shipped_lsn_by_replica.get(&2),
            Some(&Lsn::new(50))
        );

        // Replica 1 tells us it received LSN 75
        correlation.update_received(Lsn::new(75));
        assert_eq!(correlation.replica_received_lsn, Lsn::new(75));

        // Replica 2 tells us it received LSN 85 (higher)
        correlation.update_received(Lsn::new(85));
        assert_eq!(
            correlation.replica_received_lsn,
            Lsn::new(85),
            "should update to highest received"
        );

        // Compute lag at primary durable LSN 100
        let lag = correlation.replica_lag(Lsn::new(100));
        assert_eq!(lag, Lsn::new(15), "lag should be 15");

        // Not caught up
        assert!(!correlation.is_caught_up(Lsn::new(100)));

        // Once replica receives LSN 100
        correlation.update_received(Lsn::new(100));
        assert!(
            correlation.is_caught_up(Lsn::new(100)),
            "replica is caught up"
        );
    }

    /// Bonus: Test segment checksum computation
    #[test]
    fn test_segment_checksum_is_deterministic() {
        let records1 = vec![
            sample_record(1, None),
            sample_record(2, Some(1)),
            sample_record(3, Some(2)),
        ];

        let records2 = vec![
            sample_record(1, None),
            sample_record(2, Some(1)),
            sample_record(3, Some(2)),
        ];

        let checksum1 = ShippingSegmentDescriptor::compute_checksum(&records1);
        let checksum2 = ShippingSegmentDescriptor::compute_checksum(&records2);

        assert_eq!(
            checksum1, checksum2,
            "same records should produce same checksum"
        );
    }

    /// Bonus: Test that corrupted segment fails validation
    #[test]
    fn test_corrupted_segment_fails_validation() {
        let records = vec![
            sample_record(1, None),
            sample_record(2, Some(1)),
            sample_record(3, Some(2)),
        ];

        let correct_checksum = ShippingSegmentDescriptor::compute_checksum(&records);

        let descriptor = ShippingSegmentDescriptor {
            segment_id: 1,
            start_lsn: Lsn::new(1),
            end_lsn: Lsn::new(3),
            record_count: 3,
            checksum: correct_checksum ^ 0xFF, // Corrupt the checksum
        };

        let envelope = ShippingSegmentEnvelope::new(descriptor, &records);

        // Act: validate checksum
        let result = envelope.validate_checksum();

        // Assert: validation fails
        assert!(result.is_err(), "corrupted checksum should fail validation");
    }
}
