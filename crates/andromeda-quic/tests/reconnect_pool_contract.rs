//! Public contract for runtime-free reconnect pool and retry admission.

use andromeda_core::AndromedaErrorKind;
use andromeda_quic::{
    ConnectionPool, ConnectionPoolKey, ConnectionPoolPolicy, PoolAdmissionKind, ReconnectPolicy,
    ReconnectState, RetryAdmissionDecision, RetryAdmissionPolicy, RetryIdempotency,
    RetryRejectionReason, SurfacePlane,
};

fn fp(ch: char) -> String {
    ch.to_string().repeat(64)
}

#[test]
fn pool_key_separates_same_server_identity_by_surface_plane() {
    let mut pool = ConnectionPool::new(ConnectionPoolPolicy {
        max_connections_per_key: 1,
        idle_timeout_ms: 1_000,
        evict_unhealthy: true,
    })
    .unwrap();

    let app_key = ConnectionPoolKey::new(fp('a'), SurfacePlane::Application).unwrap();
    let hadr_key = ConnectionPoolKey::new(fp('a'), SurfacePlane::HighAvailability).unwrap();

    let app = pool.admit_or_reuse(app_key.clone(), 0).unwrap();
    let app_reused = pool.admit_or_reuse(app_key, 10).unwrap();
    let hadr = pool.admit_or_reuse(hadr_key, 10).unwrap();

    assert_eq!(app.kind, PoolAdmissionKind::Opened);
    assert_eq!(app_reused.kind, PoolAdmissionKind::Reused);
    assert_eq!(app_reused.connection_id, app.connection_id);
    assert_eq!(hadr.kind, PoolAdmissionKind::Opened);
    assert_ne!(hadr.connection_id, app.connection_id);
}

#[test]
fn pool_capacity_blocks_new_connection_until_unhealthy_eviction() {
    let mut pool = ConnectionPool::new(ConnectionPoolPolicy {
        max_connections_per_key: 1,
        idle_timeout_ms: 1_000,
        evict_unhealthy: false,
    })
    .unwrap();
    let key = ConnectionPoolKey::new(fp('a'), SurfacePlane::Application).unwrap();

    let admitted = pool.admit_or_reuse(key.clone(), 0).unwrap();
    pool.mark_unhealthy(admitted.connection_id).unwrap();

    let err = pool.admit_or_reuse(key.clone(), 1).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Resource);

    assert_eq!(pool.evict_unhealthy(), 1);
    let replacement = pool.admit_or_reuse(key, 2).unwrap();
    assert_eq!(replacement.kind, PoolAdmissionKind::Opened);
    assert_ne!(replacement.connection_id, admitted.connection_id);
}

#[test]
fn idle_timeout_evicts_only_connections_past_policy_boundary() {
    let mut pool = ConnectionPool::new(ConnectionPoolPolicy {
        max_connections_per_key: 2,
        idle_timeout_ms: 100,
        evict_unhealthy: true,
    })
    .unwrap();
    let key = ConnectionPoolKey::new(fp('a'), SurfacePlane::Monitoring).unwrap();

    let admitted = pool.admit_or_reuse(key, 5).unwrap();

    assert_eq!(pool.evict_idle(104), 0);
    assert!(pool.contains(admitted.connection_id));

    assert_eq!(pool.evict_idle(105), 1);
    assert!(!pool.contains(admitted.connection_id));
}

#[test]
fn retry_admission_is_idempotent_only_and_bounded_by_reconnect_policy() {
    let retry = RetryAdmissionPolicy::idempotent_only();
    let reconnect = ReconnectPolicy {
        max_attempts: 2,
        ..ReconnectPolicy::conservative()
    };

    assert_eq!(
        retry
            .admit_after_failure(
                ReconnectState::Disconnected,
                reconnect,
                RetryIdempotency::NonIdempotent,
                1,
            )
            .unwrap(),
        RetryAdmissionDecision::Reject(RetryRejectionReason::NonIdempotentRequest)
    );

    assert_eq!(
        retry
            .admit_after_failure(
                ReconnectState::Reconnecting { attempt: 1 },
                reconnect,
                RetryIdempotency::Idempotent,
                1,
            )
            .unwrap(),
        RetryAdmissionDecision::Admit {
            next_attempt: 2,
            delay_ms: 200,
        }
    );

    assert_eq!(
        retry
            .admit_after_failure(
                ReconnectState::Disconnected,
                reconnect,
                RetryIdempotency::Idempotent,
                2,
            )
            .unwrap(),
        RetryAdmissionDecision::Reject(RetryRejectionReason::AttemptsExhausted)
    );
}

#[test]
fn reconnect_state_exposes_attempt_for_observability() {
    assert_eq!(
        ReconnectState::Reconnecting { attempt: 3 }.reconnect_attempt(),
        Some(3)
    );
    assert_eq!(ReconnectState::Disconnected.reconnect_attempt(), None);
    assert_eq!(ReconnectState::Failed.reconnect_attempt(), None);
}
