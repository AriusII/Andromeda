mod decision;
mod error;
mod policy;
mod pool;
mod pool_key;
mod pool_policy;
mod pooled_connection;
mod retry;
mod state;

pub use decision::ReconnectDecision;
pub(crate) use error::{pool_error, reconnect_error};
pub use policy::{ReconnectAttemptTrace, ReconnectPolicy};
pub use pool::ConnectionPool;
pub use pool_key::ConnectionPoolKey;
pub use pool_policy::{
    ConnectionPoolPolicy, MAX_POOL_CONNECTIONS_PER_KEY, MAX_POOL_IDLE_TIMEOUT_MS,
};
pub use pooled_connection::{
    PoolAdmission, PoolAdmissionKind, PoolConnectionId, PooledConnection, PooledConnectionHealth,
};
pub use retry::{
    RetryAdmissionDecision, RetryAdmissionPolicy, RetryIdempotency, RetryRejectionReason,
};
pub use state::ReconnectState;

#[cfg(test)]
use policy::MAX_RECONNECT_ATTEMPTS;

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_observe::{CertificateIdentity, SurfaceScope};

    use crate::SurfacePlane;

    #[test]
    fn conservative_policy_is_bounded() {
        let policy = ReconnectPolicy::conservative();

        policy.validate().unwrap();
        assert_eq!(policy.delay_for_attempt(1).unwrap(), 100);
        assert_eq!(policy.delay_for_attempt(2).unwrap(), 200);
        assert_eq!(policy.delay_for_attempt(7).unwrap(), 5_000);
    }

    #[test]
    fn policy_rejects_unbounded_values() {
        assert!(
            ReconnectPolicy {
                initial_backoff_ms: 0,
                ..ReconnectPolicy::conservative()
            }
            .validate()
            .is_err()
        );
        assert!(
            ReconnectPolicy {
                max_backoff_ms: 99,
                ..ReconnectPolicy::conservative()
            }
            .validate()
            .is_err()
        );
        assert!(
            ReconnectPolicy {
                max_attempts: MAX_RECONNECT_ATTEMPTS + 1,
                ..ReconnectPolicy::conservative()
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn decision_gives_up_after_last_attempt() {
        let policy = ReconnectPolicy {
            max_attempts: 3,
            ..ReconnectPolicy::conservative()
        };

        assert_eq!(
            policy.decision_after_failure(1).unwrap(),
            ReconnectDecision::RetryAfter {
                next_attempt: 2,
                delay_ms: 200
            }
        );
        assert_eq!(
            policy.decision_after_failure(3).unwrap(),
            ReconnectDecision::GiveUp
        );
    }

    #[test]
    fn reconnect_state_blocks_requests_until_connected() {
        assert!(ReconnectState::Connected.allows_new_requests());
        assert!(!ReconnectState::Draining.allows_new_requests());
        assert!(!ReconnectState::Disconnected.allows_new_requests());
        assert!(!ReconnectState::Reconnecting { attempt: 1 }.allows_new_requests());
        assert!(ReconnectState::Failed.is_terminal());
    }

    #[test]
    fn pool_key_requires_server_identity_and_matching_plane_scope() {
        let identity =
            CertificateIdentity::new("server-fp", "server-a", SurfaceScope::Application).unwrap();

        assert_eq!(
            ConnectionPoolKey::from_server_identity(&identity, SurfacePlane::Application)
                .unwrap()
                .server_fingerprint(),
            "server-fp"
        );
        assert!(
            ConnectionPoolKey::from_server_identity(&identity, SurfacePlane::Administration)
                .is_err()
        );
        assert!(ConnectionPoolKey::new("  ", SurfacePlane::Application).is_err());
    }

    #[test]
    fn pool_reuses_only_matching_key_and_honors_idle_eviction() {
        let mut pool = ConnectionPool::new(ConnectionPoolPolicy {
            max_connections_per_key: 2,
            idle_timeout_ms: 100,
            evict_unhealthy: true,
        })
        .unwrap();
        let app_key = ConnectionPoolKey::new("server-fp", SurfacePlane::Application).unwrap();
        let admin_key = ConnectionPoolKey::new("server-fp", SurfacePlane::Administration).unwrap();

        let first = pool.admit_or_reuse(app_key.clone(), 0).unwrap();
        assert_eq!(first.kind, PoolAdmissionKind::Opened);
        let reused = pool.admit_or_reuse(app_key.clone(), 10).unwrap();
        assert_eq!(reused.kind, PoolAdmissionKind::Reused);
        assert_eq!(reused.connection_id, first.connection_id);

        let admin = pool.admit_or_reuse(admin_key, 20).unwrap();
        assert_eq!(admin.kind, PoolAdmissionKind::Opened);
        assert_ne!(admin.connection_id, first.connection_id);

        assert_eq!(pool.evict_idle(109), 0);
        assert_eq!(pool.evict_idle(110), 1);
        assert!(!pool.contains(first.connection_id));
    }

    #[test]
    fn unhealthy_connections_are_evicted_before_admission() {
        let mut pool = ConnectionPool::new(ConnectionPoolPolicy {
            max_connections_per_key: 1,
            idle_timeout_ms: 1_000,
            evict_unhealthy: true,
        })
        .unwrap();
        let key = ConnectionPoolKey::new("server-fp", SurfacePlane::Application).unwrap();

        let first = pool.admit_or_reuse(key.clone(), 0).unwrap();
        pool.mark_unhealthy(first.connection_id).unwrap();
        let replacement = pool.admit_or_reuse(key, 10).unwrap();

        assert_eq!(replacement.kind, PoolAdmissionKind::Opened);
        assert_ne!(replacement.connection_id, first.connection_id);
        assert!(!pool.contains(first.connection_id));
    }

    #[test]
    fn pool_capacity_is_scoped_to_server_identity_and_plane() {
        let mut pool = ConnectionPool::new(ConnectionPoolPolicy {
            max_connections_per_key: 1,
            idle_timeout_ms: 1_000,
            evict_unhealthy: false,
        })
        .unwrap();
        let app_key = ConnectionPoolKey::new("server-fp", SurfacePlane::Application).unwrap();
        let admin_key = ConnectionPoolKey::new("server-fp", SurfacePlane::Administration).unwrap();

        let opened = pool.admit_or_reuse(app_key.clone(), 0).unwrap();
        pool.mark_unhealthy(opened.connection_id).unwrap();
        assert!(pool.admit_or_reuse(app_key, 10).is_err());
        assert!(pool.admit_or_reuse(admin_key, 10).is_ok());
    }

    #[test]
    fn retry_admission_requires_idempotency_and_reconnect_budget() {
        let policy = RetryAdmissionPolicy::idempotent_only();
        let reconnect = ReconnectPolicy {
            max_attempts: 2,
            ..ReconnectPolicy::conservative()
        };

        assert_eq!(
            policy
                .admit_after_failure(
                    ReconnectState::Disconnected,
                    reconnect,
                    RetryIdempotency::NonIdempotent,
                    1
                )
                .unwrap(),
            RetryAdmissionDecision::Reject(RetryRejectionReason::NonIdempotentRequest)
        );
        assert_eq!(
            policy
                .admit_after_failure(
                    ReconnectState::Disconnected,
                    reconnect,
                    RetryIdempotency::Idempotent,
                    1
                )
                .unwrap(),
            RetryAdmissionDecision::Admit {
                next_attempt: 2,
                delay_ms: 200
            }
        );
        assert_eq!(
            policy
                .admit_after_failure(
                    ReconnectState::Disconnected,
                    reconnect,
                    RetryIdempotency::Idempotent,
                    2
                )
                .unwrap(),
            RetryAdmissionDecision::Reject(RetryRejectionReason::AttemptsExhausted)
        );
    }

    #[test]
    fn first_retry_uses_200ms_for_attempt_two() {
        let policy = ReconnectPolicy::conservative();
        assert_eq!(policy.delay_for_attempt(2).unwrap(), 200);
    }

    #[test]
    fn cumulative_retry_budget_is_16_2_seconds_for_conservative_policy() {
        let policy = ReconnectPolicy::conservative();
        assert_eq!(policy.cumulative_retry_budget_ms().unwrap(), 16_200);
    }

    #[test]
    fn attempts_six_and_seven_hit_five_second_plateau() {
        let policy = ReconnectPolicy::conservative();
        assert_eq!(policy.delay_for_attempt(6).unwrap(), 3_200);
        assert_eq!(policy.delay_for_attempt(7).unwrap(), 5_000);
        assert_eq!(policy.delay_for_attempt(8).unwrap(), 5_000);
    }

    #[test]
    fn attempt_eight_exhausts_retry_budget() {
        let policy = ReconnectPolicy::conservative();
        assert_eq!(
            policy.decision_after_failure(8).unwrap(),
            ReconnectDecision::GiveUp
        );
    }

    #[test]
    fn jitter_bounds_wrap_base_delay() {
        let policy = ReconnectPolicy::conservative();
        let (min_delay, max_delay) = policy.jitter_bounds_for_attempt(3).unwrap();
        assert_eq!(min_delay, 400);
        assert_eq!(max_delay, 440);
    }

    #[test]
    fn attempt_trace_includes_attempt_delay_and_jitter() {
        let policy = ReconnectPolicy::conservative();
        let trace = policy.attempt_trace(4).unwrap();
        assert_eq!(trace.attempt_number, 4);
        assert_eq!(trace.delay_ms, 800);
        assert_eq!(trace.jitter_ppm, policy.jitter_ppm);
    }
}
