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

mod checksum;
mod correlation;
mod fencing;
mod flow_control;
mod segment;

pub use correlation::*;
pub use fencing::*;
pub use flow_control::*;
pub use segment::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Lsn,
        write_ahead_log::record::{WalRecord, WalRecordKind},
    };
    use andromeda_error::AndromedaResult;

    fn sample_record(lsn: u64, prev: Option<u64>) -> AndromedaResult<WalRecord> {
        WalRecord::from_parts(
            WalRecordKind::PageAllocate,
            Lsn::new(lsn),
            prev.map(Lsn::new),
            None,
            vec![42u8; 64], // Dummy payload
        )
    }

    /// Test 1: Segment committed -> ShippingThread reads -> sends via QUIC
    ///
    /// Simulates:
    /// - Primary WriterThread appends records LSN 1-10 and flushes.
    /// - ShippingThread sees durable_lsn = 10.
    /// - Creates a ShippingSegmentEnvelope for LSN 1-10.
    /// - Validates structure and checksum.
    /// - (Transport would send via QUIC; we validate the envelope here.)
    #[test]
    fn test_shipping_thread_reads_committed_segments() -> AndromedaResult<()> {
        // Arrange: create a committed segment with 5 records
        let records = vec![
            sample_record(1, None)?,
            sample_record(2, Some(1))?,
            sample_record(3, Some(2))?,
            sample_record(4, Some(3))?,
            sample_record(5, Some(4))?,
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
        Ok(())
    }

    /// Test 2: Replica receives segment -> validates checksum -> appends to local WAL
    ///
    /// Simulates:
    /// - Primary sends ShippingSegmentEnvelope with LSN 6-10.
    /// - Replica ReceivingThread receives it.
    /// - Validates checksum (should pass).
    /// - Appends records to local WAL after structure and checksum validation.
    #[test]
    fn test_replica_receives_validates_appends() -> AndromedaResult<()> {
        // Arrange: create a segment for LSN 6-10 (following LSN 5)
        let records = vec![
            sample_record(6, Some(5))?,
            sample_record(7, Some(6))?,
            sample_record(8, Some(7))?,
            sample_record(9, Some(8))?,
            sample_record(10, Some(9))?,
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

        assert_eq!(
            envelope.records.first().expect("first record").header.lsn,
            Lsn::new(6)
        );
        assert_eq!(
            envelope.records.last().expect("last record").header.lsn,
            Lsn::new(10)
        );
        Ok(())
    }

    /// Test 3: Shipping backpressure - replica falls behind -> requests earlier segment
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
    fn test_segment_checksum_is_deterministic() -> AndromedaResult<()> {
        let records1 = vec![
            sample_record(1, None)?,
            sample_record(2, Some(1))?,
            sample_record(3, Some(2))?,
        ];

        let records2 = vec![
            sample_record(1, None)?,
            sample_record(2, Some(1))?,
            sample_record(3, Some(2))?,
        ];

        let checksum1 = ShippingSegmentDescriptor::compute_checksum(&records1);
        let checksum2 = ShippingSegmentDescriptor::compute_checksum(&records2);

        assert_eq!(
            checksum1, checksum2,
            "same records should produce same checksum"
        );
        Ok(())
    }

    /// Bonus: Test that corrupted segment fails validation
    #[test]
    fn test_corrupted_segment_fails_validation() -> AndromedaResult<()> {
        let records = vec![
            sample_record(1, None)?,
            sample_record(2, Some(1))?,
            sample_record(3, Some(2))?,
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
        Ok(())
    }
}
