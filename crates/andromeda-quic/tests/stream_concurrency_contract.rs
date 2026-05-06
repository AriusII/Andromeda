//! Public-surface contract for stream concurrency, cancellation, and backpressure.

use andromeda_core::{AndromedaErrorKind, InvocationId};
use andromeda_quic::{
    CancellationReason, CancellationToken, StreamConcurrencyManager, StreamState,
};
use std::thread;
use std::time::Duration;

fn invocation_id(id: u64) -> InvocationId {
    InvocationId::new(id)
}

#[test]
fn test_stream_created_state_initializes_correctly() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);
    let token = mgr.create_stream(id).expect("stream creation failed");

    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Created);
    assert_eq!(mgr.total_concurrent_streams(), 1);
    assert_eq!(mgr.active_stream_count(), 1);
    assert_eq!(token.get(), 1);

    let retrieved_token = mgr.get_cancellation_token(id).unwrap();
    assert_eq!(token, retrieved_token);
}

#[test]
fn test_backpressure_blocks_new_streams_when_limit_reached() {
    let mut mgr =
        StreamConcurrencyManager::with_bounds(2, Duration::from_secs(30), Duration::from_secs(300));

    let id1 = invocation_id(1);
    let id2 = invocation_id(2);
    assert!(mgr.create_stream(id1).is_ok());
    assert!(mgr.create_stream(id2).is_ok());

    assert!(mgr.is_at_capacity());
    assert_eq!(mgr.active_stream_count(), 2);

    let bp = mgr.backpressure_status();
    assert!(bp.is_some());
    let request = bp.unwrap();
    assert_eq!(request.retry_after_millis, 100);

    let id3 = invocation_id(3);
    let err = mgr.create_stream(id3).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Resource);
}

#[test]
fn test_client_cancellation_triggers_graceful_shutdown() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);

    mgr.create_stream(id).unwrap();
    mgr.accept_first_frame(id).unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Active);

    mgr.cancel_stream(id, CancellationReason::ClientRequested)
        .unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Cancelling);

    let reason = mgr.get_cancellation_reason(id).unwrap();
    assert_eq!(reason, Some(CancellationReason::ClientRequested));

    mgr.mark_complete(id).unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Terminal);
}

#[test]
fn test_concurrent_stream_operations_are_atomic() {
    let mut mgr = StreamConcurrencyManager::new();
    let id1 = invocation_id(1);
    let id2 = invocation_id(2);
    let id3 = invocation_id(3);

    mgr.create_stream(id1).unwrap();
    mgr.create_stream(id2).unwrap();
    mgr.create_stream(id3).unwrap();

    mgr.accept_first_frame(id1).unwrap();
    assert_eq!(mgr.get_state(id1).unwrap(), StreamState::Active);

    assert_eq!(mgr.get_state(id2).unwrap(), StreamState::Created);

    mgr.mark_complete(id3).unwrap();
    assert_eq!(mgr.get_state(id3).unwrap(), StreamState::Terminal);

    assert_eq!(mgr.total_concurrent_streams(), 3);
    assert_eq!(mgr.active_stream_count(), 2);
}

#[test]
fn test_stream_timeout_cleanup() {
    let idle_timeout = Duration::from_millis(100);
    let overall_timeout = Duration::from_secs(300);
    let mut mgr = StreamConcurrencyManager::with_bounds(10, idle_timeout, overall_timeout);

    let id1 = invocation_id(1);
    let id2 = invocation_id(2);

    mgr.create_stream(id1).unwrap();
    mgr.accept_first_frame(id1).unwrap();
    mgr.create_stream(id2).unwrap();
    mgr.accept_first_frame(id2).unwrap();

    thread::sleep(Duration::from_millis(50));
    mgr.record_activity(id1).unwrap();

    thread::sleep(Duration::from_millis(60));

    let idle_streams = mgr.detect_idle_timeouts();
    assert!(idle_streams.contains(&id2), "id2 should be idle");
    assert!(!idle_streams.contains(&id1), "id1 should not be idle");

    mgr.cancel_stream(id2, CancellationReason::IdleTimeout)
        .unwrap();
    mgr.mark_complete(id2).unwrap();
    mgr.cleanup_stream(id2).unwrap();

    assert_eq!(mgr.total_concurrent_streams(), 1);
}

#[test]
fn test_orphan_stream_detection() {
    let idle_timeout = Duration::from_millis(50);
    let overall_timeout = Duration::from_millis(200);
    let mut mgr = StreamConcurrencyManager::with_bounds(10, idle_timeout, overall_timeout);

    let id1 = invocation_id(1);
    let id2 = invocation_id(2);
    let id3 = invocation_id(3);

    mgr.create_stream(id1).unwrap();
    mgr.accept_first_frame(id1).unwrap();

    thread::sleep(Duration::from_millis(60));
    mgr.create_stream(id2).unwrap();
    mgr.accept_first_frame(id2).unwrap();

    thread::sleep(Duration::from_millis(60));
    mgr.create_stream(id3).unwrap();
    mgr.accept_first_frame(id3).unwrap();

    thread::sleep(Duration::from_millis(50));

    let orphaned = mgr.detect_orphaned_streams();
    assert!(
        !orphaned.is_empty(),
        "should detect at least one orphaned stream"
    );
}

#[test]
fn test_race_simultaneous_cancel_and_completion() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);

    mgr.create_stream(id).unwrap();
    mgr.accept_first_frame(id).unwrap();

    mgr.cancel_stream(id, CancellationReason::ClientRequested)
        .unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Cancelling);

    mgr.mark_complete(id).unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Terminal);

    let reason = mgr.get_cancellation_reason(id).unwrap();
    assert_eq!(reason, Some(CancellationReason::ClientRequested));
}

#[test]
fn test_backpressure_recovery_on_stream_completion() {
    let mut mgr =
        StreamConcurrencyManager::with_bounds(2, Duration::from_secs(30), Duration::from_secs(300));

    let id1 = invocation_id(1);
    let id2 = invocation_id(2);
    let id3 = invocation_id(3);

    mgr.create_stream(id1).unwrap();
    mgr.create_stream(id2).unwrap();
    assert!(mgr.is_at_capacity());
    assert!(mgr.backpressure_status().is_some());

    assert!(mgr.create_stream(id3).is_err());

    mgr.mark_complete(id1).unwrap();
    mgr.cleanup_stream(id1).unwrap();

    assert!(!mgr.is_at_capacity());
    assert!(mgr.backpressure_status().is_none());

    assert!(mgr.create_stream(id3).is_ok());
    assert_eq!(mgr.active_stream_count(), 2);
}

#[test]
fn test_cancellation_token_deterministic_and_replay_safe() {
    let mut mgr1 = StreamConcurrencyManager::new();

    let id = invocation_id(42);

    let token1 = mgr1.create_stream(id).unwrap();
    let token_direct = CancellationToken::from_invocation_id(id);

    assert_eq!(token1, token_direct);
    assert_eq!(token1.get(), 42);
}

#[test]
fn test_invalid_state_transitions_rejected() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);

    let err = mgr.accept_first_frame(id).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transport);

    mgr.create_stream(id).unwrap();

    mgr.accept_first_frame(id).unwrap();
    let err = mgr.accept_first_frame(id).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transport);

    mgr.mark_complete(id).unwrap();
    let err = mgr.mark_complete(id).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transport);

    let err = mgr
        .cancel_stream(id, CancellationReason::ClientRequested)
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transport);
}

#[test]
fn test_duplicate_invocation_ids_rejected() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);

    mgr.create_stream(id).unwrap();

    let err = mgr.create_stream(id).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Transport);
}

#[test]
fn test_server_graceful_shutdown_cancellation() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);

    mgr.create_stream(id).unwrap();
    mgr.accept_first_frame(id).unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Active);

    mgr.cancel_stream(id, CancellationReason::ServerGracefulShutdown)
        .unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Cancelling);

    let reason = mgr.get_cancellation_reason(id).unwrap();
    assert_eq!(reason, Some(CancellationReason::ServerGracefulShutdown));

    mgr.mark_complete(id).unwrap();
    assert_eq!(mgr.get_state(id).unwrap(), StreamState::Terminal);
}

#[test]
fn test_overall_timeout_detection() {
    let idle_timeout = Duration::from_secs(10);
    let overall_timeout = Duration::from_millis(100);
    let mut mgr = StreamConcurrencyManager::with_bounds(10, idle_timeout, overall_timeout);

    let id = invocation_id(1);

    mgr.create_stream(id).unwrap();

    thread::sleep(Duration::from_millis(150));

    let timed_out = mgr.detect_overall_timeouts();
    assert!(
        timed_out.contains(&id),
        "stream should exceed overall timeout"
    );
}

#[test]
fn test_stream_activity_recording() {
    let idle_timeout = Duration::from_millis(100);
    let overall_timeout = Duration::from_secs(300);
    let mut mgr = StreamConcurrencyManager::with_bounds(10, idle_timeout, overall_timeout);

    let id = invocation_id(1);

    mgr.create_stream(id).unwrap();
    mgr.accept_first_frame(id).unwrap();

    assert!(!mgr.is_idle(id).unwrap());

    thread::sleep(Duration::from_millis(120));

    assert!(mgr.is_idle(id).unwrap());

    mgr.record_activity(id).unwrap();

    assert!(!mgr.is_idle(id).unwrap());
}

#[test]
fn test_total_streams_ever_created_counter() {
    let mut mgr =
        StreamConcurrencyManager::with_bounds(5, Duration::from_secs(30), Duration::from_secs(300));

    assert_eq!(mgr.total_streams_ever_created(), 0);

    for i in 1..=4 {
        let id = invocation_id(i as u64);
        mgr.create_stream(id).unwrap();
        mgr.mark_complete(id).unwrap();
        mgr.cleanup_stream(id).unwrap();
    }

    assert_eq!(mgr.total_streams_ever_created(), 4);
    assert_eq!(mgr.total_concurrent_streams(), 0);
}

#[test]
fn test_boundary_conditions_on_timeouts() {
    let idle_timeout = Duration::from_millis(100);
    let overall_timeout = Duration::from_secs(1);
    let mut mgr = StreamConcurrencyManager::with_bounds(10, idle_timeout, overall_timeout);

    let id = invocation_id(1);
    mgr.create_stream(id).unwrap();
    mgr.accept_first_frame(id).unwrap();

    thread::sleep(Duration::from_millis(25));
    assert!(!mgr.is_idle(id).unwrap());
    assert!(mgr.detect_idle_timeouts().is_empty());

    thread::sleep(Duration::from_millis(125));
    assert!(mgr.is_idle(id).unwrap());
    let idle_streams = mgr.detect_idle_timeouts();
    assert!(idle_streams.contains(&id));

    assert_eq!(mgr.total_concurrent_streams(), 1);
}

#[test]
fn test_orphan_cleanup_workflow() {
    let idle_timeout = Duration::from_millis(50);
    let overall_timeout = Duration::from_secs(300);
    let mut mgr = StreamConcurrencyManager::with_bounds(10, idle_timeout, overall_timeout);

    let id1 = invocation_id(1);
    let id2 = invocation_id(2);
    let id3 = invocation_id(3);

    mgr.create_stream(id1).unwrap();
    mgr.accept_first_frame(id1).unwrap();

    mgr.create_stream(id2).unwrap();
    mgr.accept_first_frame(id2).unwrap();

    mgr.create_stream(id3).unwrap();
    mgr.accept_first_frame(id3).unwrap();

    assert_eq!(mgr.total_concurrent_streams(), 3);

    thread::sleep(Duration::from_millis(80));

    let orphaned = mgr.detect_orphaned_streams();
    for orphan_id in orphaned {
        mgr.cancel_stream(orphan_id, CancellationReason::IdleTimeout)
            .unwrap();
        mgr.mark_complete(orphan_id).unwrap();
        mgr.cleanup_stream(orphan_id).unwrap();
    }

    assert_eq!(mgr.total_concurrent_streams(), 0);
}

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

#[test]
fn test_connection_lost_cancellation() {
    let mut mgr = StreamConcurrencyManager::new();
    let id = invocation_id(1);

    mgr.create_stream(id).unwrap();
    mgr.accept_first_frame(id).unwrap();

    mgr.cancel_stream(id, CancellationReason::ConnectionLost)
        .unwrap();
    let reason = mgr.get_cancellation_reason(id).unwrap();
    assert_eq!(reason, Some(CancellationReason::ConnectionLost));

    mgr.mark_complete(id).unwrap();
}

#[test]
fn test_backpressure_request_properties() {
    let mut mgr =
        StreamConcurrencyManager::with_bounds(1, Duration::from_secs(30), Duration::from_secs(300));

    mgr.create_stream(invocation_id(1)).unwrap();

    let request = mgr.backpressure_status().unwrap();

    assert!(request.retry_after_millis > 0);
    assert!(request.retry_after_millis <= 1000);
    assert!(request.request_id.is_none());
}
