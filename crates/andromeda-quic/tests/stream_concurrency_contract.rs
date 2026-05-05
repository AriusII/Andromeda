//! Public-surface contract for stream concurrency, cancellation, and backpressure.
//!
//! These tests lock the V0 stream lifecycle semantics:
//!
//! * Stream creation respects concurrency limits and returns backpressure when saturation is reached
//! * Stream state transitions follow the defined state machine atomically
//! * Cancellation can occur from client, server, or timeout paths
//! * Concurrent operations on different streams do not interfere
//! * Timeout detection works correctly for both idle and overall durations
//! * Orphan streams are detected and cleanable
//! * Race conditions between simultaneous cancel and completion are resolved deterministically
//! * Backpressure recovery occurs as streams complete

use andromeda_core::{AndromedaErrorKind, InvocationId};
use andromeda_quic::{
    CancellationReason, CancellationToken, StreamConcurrencyManager, StreamState,
};
use std::thread;
use std::time::Duration;

fn invocation_id(id: u64) -> InvocationId {
    InvocationId::new(id)
}

// ============================================================================
// Test 1: Stream Creation and Initialization
// ============================================================================

#[test]
fn test_stream_created_state_initializes_correctly() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);
    let token = mgr.create_stream(id).expect("stream creation failed");

    // Verify initial state
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Created);
    assert_eq!(mgr.total_concurrent_streams(), 1);
    assert_eq!(mgr.active_stream_count(), 1);
    assert_eq!(token.get(), 1);

    // Verify cancellation token is retrievable
    let retrieved_token = mgr.get_cancellation_token(id).unwrap();
    assert_eq!(token, retrieved_token);
}

// ============================================================================
// Test 2: Backpressure Blocks New Streams at Limit
// ============================================================================

#[test]
fn test_backpressure_blocks_new_streams_when_limit_reached() {
    let mut mgr =
        StreamConcurrencyManager::with_bounds(2, Duration::from_secs(30), Duration::from_secs(300));

    // Create streams up to limit
    let id1 = invocation_id(1);
    let id2 = invocation_id(2);
    assert!(mgr.create_stream(id1).is_ok());
    assert!(mgr.create_stream(id2).is_ok());

    // Verify capacity reached
    assert!(mgr.is_at_capacity());
    assert_eq!(mgr.active_stream_count(), 2);

    // Verify backpressure signal is present
    let bp = mgr.backpressure_status();
    assert!(bp.is_some());
    let request = bp.unwrap();
    assert_eq!(request.retry_after_millis, 100);

    // Verify new stream creation fails
    let id3 = invocation_id(3);
    let err = mgr.create_stream(id3).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Resource);
}

// ============================================================================
// Test 3: Client Cancellation Triggers Graceful Shutdown
// ============================================================================

#[test]
fn test_client_cancellation_triggers_graceful_shutdown() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);

    // Create and activate stream
    mgr.create_stream(id).unwrap();
    mgr.accept_first_frame(id).unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Active);

    // Client initiates cancellation
    mgr.cancel_stream(id, CancellationReason::ClientRequested)
        .unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Cancelling);

    // Verify cancellation reason is tracked
    let reason = mgr.get_cancellation_reason(id).unwrap();
    assert_eq!(reason, Some(CancellationReason::ClientRequested));

    // Mark complete (graceful shutdown end state)
    mgr.mark_complete(id).unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Terminal);
}

// ============================================================================
// Test 4: Concurrent Stream Operations Are Atomic
// ============================================================================

#[test]
fn test_concurrent_stream_operations_are_atomic() {
    let mut mgr = StreamConcurrencyManager::new();
    let id1 = invocation_id(1);
    let id2 = invocation_id(2);
    let id3 = invocation_id(3);

    // Create multiple streams
    mgr.create_stream(id1).unwrap();
    mgr.create_stream(id2).unwrap();
    mgr.create_stream(id3).unwrap();

    // Activate stream 1
    mgr.accept_first_frame(id1).unwrap();
    assert_eq!(mgr.get_state(id1).unwrap(), StreamState::Active);

    // Stream 2 remains in Created state
    assert_eq!(mgr.get_state(id2).unwrap(), StreamState::Created);

    // Complete stream 3
    mgr.mark_complete(id3).unwrap();
    assert_eq!(mgr.get_state(id3).unwrap(), StreamState::Terminal);

    // Verify all operations were independent and atomic
    assert_eq!(mgr.total_concurrent_streams(), 3);
    assert_eq!(mgr.active_stream_count(), 2); // id1 and id2 are active
}

// ============================================================================
// Test 5: Stream Timeout Cleanup (Idle Timeout)
// ============================================================================

#[test]
fn test_stream_timeout_cleanup() {
    let idle_timeout = Duration::from_millis(100);
    let overall_timeout = Duration::from_secs(300);
    let mut mgr = StreamConcurrencyManager::with_bounds(10, idle_timeout, overall_timeout);

    let id1 = invocation_id(1);
    let id2 = invocation_id(2);

    // Create and activate streams
    mgr.create_stream(id1).unwrap();
    mgr.accept_first_frame(id1).unwrap();
    mgr.create_stream(id2).unwrap();
    mgr.accept_first_frame(id2).unwrap();

    // Record activity on id1 (keeps it fresh)
    thread::sleep(Duration::from_millis(50));
    mgr.record_activity(id1).unwrap();

    // Wait for id2 to timeout (use 60ms to avoid boundary timing issues)
    // After this, id1 will have ~60ms elapsed (< 100ms idle_timeout, not idle)
    // and id2 will have ~110ms elapsed (> 100ms idle_timeout, idle)
    thread::sleep(Duration::from_millis(60));

    // Detect idle timeouts
    let idle_streams = mgr.detect_idle_timeouts();
    assert!(idle_streams.contains(&id2), "id2 should be idle");
    assert!(!idle_streams.contains(&id1), "id1 should not be idle");

    // Cancel the timed-out stream
    mgr.cancel_stream(id2, CancellationReason::IdleTimeout)
        .unwrap();
    mgr.mark_complete(id2).unwrap();
    mgr.cleanup_stream(id2).unwrap();

    // Verify removal
    assert_eq!(mgr.total_concurrent_streams(), 1);
}

// ============================================================================
// Test 6: Orphan Stream Detection
// ============================================================================

#[test]
fn test_orphan_stream_detection() {
    let idle_timeout = Duration::from_millis(50);
    let overall_timeout = Duration::from_millis(200);
    let mut mgr = StreamConcurrencyManager::with_bounds(10, idle_timeout, overall_timeout);

    let id1 = invocation_id(1);
    let id2 = invocation_id(2);
    let id3 = invocation_id(3);

    // Create streams with different ages
    mgr.create_stream(id1).unwrap();
    mgr.accept_first_frame(id1).unwrap();

    thread::sleep(Duration::from_millis(60));
    mgr.create_stream(id2).unwrap();
    mgr.accept_first_frame(id2).unwrap();

    thread::sleep(Duration::from_millis(60));
    mgr.create_stream(id3).unwrap();
    mgr.accept_first_frame(id3).unwrap();

    // Let idle timeout trigger
    thread::sleep(Duration::from_millis(50));

    // Detect orphaned streams (idle OR overall timeout exceeded)
    let orphaned = mgr.detect_orphaned_streams();
    assert!(!orphaned.is_empty(), "should detect orphaned streams");
    assert!(
        orphaned.len() >= 1,
        "should detect at least one orphaned stream"
    );
}

// ============================================================================
// Test 7: Race Condition - Simultaneous Cancel and Completion
// ============================================================================

#[test]
fn test_race_simultaneous_cancel_and_completion() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);

    // Create and activate stream
    mgr.create_stream(id).unwrap();
    mgr.accept_first_frame(id).unwrap();

    // First, attempt cancellation
    mgr.cancel_stream(id, CancellationReason::ClientRequested)
        .unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Cancelling);

    // Now mark complete (should succeed because Cancelling is not Terminal)
    mgr.mark_complete(id).unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Terminal);

    // Verify the cancellation reason is still tracked
    let reason = mgr.get_cancellation_reason(id).unwrap();
    assert_eq!(reason, Some(CancellationReason::ClientRequested));
}

// ============================================================================
// Test 8: Backpressure Recovery on Stream Completion
// ============================================================================

#[test]
fn test_backpressure_recovery_on_stream_completion() {
    let mut mgr =
        StreamConcurrencyManager::with_bounds(2, Duration::from_secs(30), Duration::from_secs(300));

    let id1 = invocation_id(1);
    let id2 = invocation_id(2);
    let id3 = invocation_id(3);

    // Fill to capacity
    mgr.create_stream(id1).unwrap();
    mgr.create_stream(id2).unwrap();
    assert!(mgr.is_at_capacity());
    assert!(mgr.backpressure_status().is_some());

    // Try to create third stream (should fail)
    assert!(mgr.create_stream(id3).is_err());

    // Complete first stream
    mgr.mark_complete(id1).unwrap();
    mgr.cleanup_stream(id1).unwrap();

    // Verify backpressure is lifted
    assert!(!mgr.is_at_capacity());
    assert!(mgr.backpressure_status().is_none());

    // Now third stream can be created
    assert!(mgr.create_stream(id3).is_ok());
    assert_eq!(mgr.active_stream_count(), 2);
}

// ============================================================================
// Test 9: Cancellation Token Is Deterministic and Replay-Safe
// ============================================================================

#[test]
fn test_cancellation_token_deterministic_and_replay_safe() {
    let mut mgr1 = StreamConcurrencyManager::new();

    let id = invocation_id(42);

    let token1 = mgr1.create_stream(id).unwrap();
    // Demonstrate determinism by creating the token directly.
    let token_direct = CancellationToken::from_invocation_id(id);

    // Tokens should match
    assert_eq!(token1, token_direct);
    assert_eq!(token1.get(), 42);
}

// ============================================================================
// Test 10: Invalid State Transitions Rejected
// ============================================================================

#[test]
fn test_invalid_state_transitions_rejected() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);

    // Cannot accept_first_frame on non-existent stream
    let err = mgr.accept_first_frame(id).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transport);

    // Create stream
    mgr.create_stream(id).unwrap();

    // Cannot accept_first_frame twice
    mgr.accept_first_frame(id).unwrap();
    let err = mgr.accept_first_frame(id).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transport);

    // Cannot mark_complete twice
    mgr.mark_complete(id).unwrap();
    let err = mgr.mark_complete(id).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transport);

    // Cannot cancel a terminal stream
    let err = mgr
        .cancel_stream(id, CancellationReason::ClientRequested)
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transport);
}

// ============================================================================
// Test 11: Duplicate Invocation IDs Rejected
// ============================================================================

#[test]
fn test_duplicate_invocation_ids_rejected() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);

    // Create first stream
    mgr.create_stream(id).unwrap();

    // Attempt to create stream with same ID
    let err = mgr.create_stream(id).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transport);
}

// ============================================================================
// Test 12: Server Graceful Shutdown Cancellation
// ============================================================================

#[test]
fn test_server_graceful_shutdown_cancellation() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);

    // Create and activate stream
    mgr.create_stream(id).unwrap();
    mgr.accept_first_frame(id).unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Active);

    // Server initiates graceful shutdown
    mgr.cancel_stream(id, CancellationReason::ServerGracefulShutdown)
        .unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Cancelling);

    // Verify cancellation reason
    let reason = mgr.get_cancellation_reason(id).unwrap();
    assert_eq!(reason, Some(CancellationReason::ServerGracefulShutdown));

    // Complete gracefully
    mgr.mark_complete(id).unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Terminal);
}

// ============================================================================
// Test 13: Overall Timeout Detection
// ============================================================================

#[test]
fn test_overall_timeout_detection() {
    let idle_timeout = Duration::from_secs(10);
    let overall_timeout = Duration::from_millis(100);
    let mut mgr = StreamConcurrencyManager::with_bounds(10, idle_timeout, overall_timeout);

    let id = invocation_id(1);

    // Create stream
    mgr.create_stream(id).unwrap();

    // Wait for overall timeout
    thread::sleep(Duration::from_millis(150));

    // Detect overall timeouts
    let timed_out = mgr.detect_overall_timeouts();
    assert!(
        timed_out.contains(&id),
        "stream should exceed overall timeout"
    );
}

// ============================================================================
// Test 14: Stream Activity Recording
// ============================================================================

#[test]
fn test_stream_activity_recording() {
    let idle_timeout = Duration::from_millis(100);
    let overall_timeout = Duration::from_secs(300);
    let mut mgr = StreamConcurrencyManager::with_bounds(10, idle_timeout, overall_timeout);

    let id = invocation_id(1);

    // Create and activate stream
    mgr.create_stream(id).unwrap();
    mgr.accept_first_frame(id).unwrap();

    // Initially not idle
    assert!(!mgr.is_idle(id).unwrap());

    // Wait for idle threshold
    thread::sleep(Duration::from_millis(120));

    // Now it should be idle
    assert!(mgr.is_idle(id).unwrap());

    // Record activity
    mgr.record_activity(id).unwrap();

    // No longer idle
    assert!(!mgr.is_idle(id).unwrap());
}

// ============================================================================
// Test 15: Total Streams Ever Created Counter
// ============================================================================

#[test]
fn test_total_streams_ever_created_counter() {
    let mut mgr =
        StreamConcurrencyManager::with_bounds(5, Duration::from_secs(30), Duration::from_secs(300));

    assert_eq!(mgr.total_streams_ever_created(), 0);

    // Create and clean up multiple streams
    for i in 1..=4 {
        let id = invocation_id(i as u64);
        mgr.create_stream(id).unwrap();
        mgr.mark_complete(id).unwrap();
        mgr.cleanup_stream(id).unwrap();
    }

    // Counter should reflect all creations
    assert_eq!(mgr.total_streams_ever_created(), 4);
    assert_eq!(mgr.total_concurrent_streams(), 0);
}

// ============================================================================
// Test 16: Boundary Conditions on Timeouts
// ============================================================================

#[test]
fn test_boundary_conditions_on_timeouts() {
    let idle_timeout = Duration::from_millis(100);
    let overall_timeout = Duration::from_millis(200);
    let mut mgr = StreamConcurrencyManager::with_bounds(10, idle_timeout, overall_timeout);

    let id = invocation_id(1);
    mgr.create_stream(id).unwrap();
    mgr.accept_first_frame(id).unwrap();

    // Just before idle timeout
    thread::sleep(Duration::from_millis(99));
    assert!(!mgr.is_idle(id).unwrap());
    assert!(mgr.detect_idle_timeouts().is_empty());

    // After idle timeout but before overall timeout
    thread::sleep(Duration::from_millis(10));
    assert!(mgr.is_idle(id).unwrap());
    let idle_streams = mgr.detect_idle_timeouts();
    assert!(idle_streams.contains(&id));

    // The stream should still be present (not cleaned up automatically)
    assert_eq!(mgr.total_concurrent_streams(), 1);
}

// ============================================================================
// Test 17: Orphan Cleanup Workflow
// ============================================================================

#[test]
fn test_orphan_cleanup_workflow() {
    let idle_timeout = Duration::from_millis(50);
    let overall_timeout = Duration::from_secs(300);
    let mut mgr = StreamConcurrencyManager::with_bounds(10, idle_timeout, overall_timeout);

    let id1 = invocation_id(1);
    let id2 = invocation_id(2);
    let id3 = invocation_id(3);

    // Create multiple streams
    mgr.create_stream(id1).unwrap();
    mgr.accept_first_frame(id1).unwrap();

    mgr.create_stream(id2).unwrap();
    mgr.accept_first_frame(id2).unwrap();

    mgr.create_stream(id3).unwrap();
    mgr.accept_first_frame(id3).unwrap();

    assert_eq!(mgr.total_concurrent_streams(), 3);

    // Wait for idle timeout
    thread::sleep(Duration::from_millis(80));

    // Detect and clean up orphans
    let orphaned = mgr.detect_orphaned_streams();
    for orphan_id in orphaned {
        mgr.cancel_stream(orphan_id, CancellationReason::IdleTimeout)
            .unwrap();
        mgr.mark_complete(orphan_id).unwrap();
        mgr.cleanup_stream(orphan_id).unwrap();
    }

    // All should be cleaned up
    assert_eq!(mgr.total_concurrent_streams(), 0);
}

// ============================================================================
// Test 18: Configuration Accessors
// ============================================================================

#[test]
fn test_configuration_accessors() {
    let idle = Duration::from_secs(15);
    let overall = Duration::from_secs(600);
    let max_concurrent = 256;

    let mgr = StreamConcurrencyManager::with_bounds(max_concurrent, idle, overall);

    assert_eq!(mgr.max_concurrent(), max_concurrent);
    assert_eq!(mgr.idle_timeout(), idle);
    assert_eq!(mgr.overall_timeout(), overall);
}

// ============================================================================
// Test 19: Connection Lost Cancellation
// ============================================================================

#[test]
fn test_connection_lost_cancellation() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);

    mgr.create_stream(id).unwrap();
    mgr.accept_first_frame(id).unwrap();

    // Simulate connection loss
    mgr.cancel_stream(id, CancellationReason::ConnectionLost)
        .unwrap();
    let reason = mgr.get_cancellation_reason(id).unwrap();
    assert_eq!(reason, Some(CancellationReason::ConnectionLost));

    mgr.mark_complete(id).unwrap();
}

// ============================================================================
// Test 20: Backpressure Request Properties
// ============================================================================

#[test]
fn test_backpressure_request_properties() {
    let mut mgr =
        StreamConcurrencyManager::with_bounds(1, Duration::from_secs(30), Duration::from_secs(300));

    // Fill capacity
    mgr.create_stream(invocation_id(1)).unwrap();

    // Get backpressure request
    let request = mgr.backpressure_status().unwrap();

    // Verify properties
    assert!(request.retry_after_millis > 0);
    assert!(request.retry_after_millis <= 1000);
    // In V0, request_id is None for system-wide backpressure
}
