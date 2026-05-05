use std::collections::HashMap;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::CertificateIdentity;

use crate::{SurfacePlane, mtls_identity::plane_to_required_surface_scope};

/// Maximum reconnect backoff accepted by the runtime-free contract.
pub const MAX_RECONNECT_BACKOFF_MS: u64 = 60_000;

/// Maximum reconnect attempts accepted by the runtime-free contract.
pub const MAX_RECONNECT_ATTEMPTS: u32 = 32;

/// Maximum idle timeout accepted by the runtime-free connection pool contract.
pub const MAX_POOL_IDLE_TIMEOUT_MS: u64 = 15 * 60 * 1_000;

/// Maximum number of live pooled connections for one server identity and plane.
pub const MAX_POOL_CONNECTIONS_PER_KEY: u16 = 128;

/// Runtime-free reconnect policy for QUIC client connections.
///
/// This type intentionally models only deterministic admission and scheduling
/// rules. Concrete timers, socket creation, TLS handshakes, and request retry
/// semantics belong to the Quinn runtime layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectPolicy {
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub max_attempts: u32,
    /// Jitter range in parts per million. The runtime may randomize each delay
    /// by at most this fraction, but the deterministic contract reports the
    /// base delay before jitter.
    pub jitter_ppm: u32,
}

impl ReconnectPolicy {
    pub const fn conservative() -> Self {
        Self {
            initial_backoff_ms: 100,
            max_backoff_ms: 5_000,
            max_attempts: 8,
            jitter_ppm: 100_000,
        }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.initial_backoff_ms == 0 {
            return Err(reconnect_error(
                "initial reconnect backoff must be non-zero",
            ));
        }
        if self.max_backoff_ms < self.initial_backoff_ms {
            return Err(reconnect_error(
                "max reconnect backoff must be >= initial backoff",
            ));
        }
        if self.max_backoff_ms > MAX_RECONNECT_BACKOFF_MS {
            return Err(reconnect_error(
                "max reconnect backoff exceeds contract maximum",
            ));
        }
        if self.max_attempts == 0 || self.max_attempts > MAX_RECONNECT_ATTEMPTS {
            return Err(reconnect_error(
                "reconnect max_attempts must be in 1..=MAX_RECONNECT_ATTEMPTS",
            ));
        }
        if self.jitter_ppm > 1_000_000 {
            return Err(reconnect_error("reconnect jitter_ppm must be <= 1_000_000"));
        }
        Ok(())
    }

    pub fn delay_for_attempt(self, attempt: u32) -> AndromedaResult<u64> {
        self.validate()?;
        if attempt == 0 || attempt > self.max_attempts {
            return Err(reconnect_error(
                "reconnect attempt must be in 1..=max_attempts",
            ));
        }

        let shift = attempt.saturating_sub(1).min(63);
        let multiplier = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
        Ok(self
            .initial_backoff_ms
            .saturating_mul(multiplier)
            .min(self.max_backoff_ms))
    }

    pub fn decision_after_failure(self, attempt: u32) -> AndromedaResult<ReconnectDecision> {
        self.validate()?;
        if attempt >= self.max_attempts {
            return Ok(ReconnectDecision::GiveUp);
        }
        Ok(ReconnectDecision::RetryAfter {
            next_attempt: attempt.saturating_add(1),
            delay_ms: self.delay_for_attempt(attempt.saturating_add(1))?,
        })
    }
}

/// Client connection state used by the reconnection contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectState {
    Connected,
    Draining,
    Disconnected,
    Reconnecting { attempt: u32 },
    Failed,
}

impl ReconnectState {
    pub const fn allows_new_requests(self) -> bool {
        matches!(self, Self::Connected)
    }

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Failed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectDecision {
    RetryAfter { next_attempt: u32, delay_ms: u64 },
    GiveUp,
}

/// Connection pool key: a server certificate identity scoped to one surface plane.
///
/// The runtime may have different socket addresses for the same logical server,
/// but pooling is keyed by authenticated server fingerprint and surface plane.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectionPoolKey {
    pub server_fingerprint: String,
    pub plane: SurfacePlane,
}

impl ConnectionPoolKey {
    pub fn new(
        server_fingerprint: impl Into<String>,
        plane: SurfacePlane,
    ) -> AndromedaResult<Self> {
        let key = Self {
            server_fingerprint: server_fingerprint.into(),
            plane,
        };
        key.validate()?;
        Ok(key)
    }

    pub fn from_server_identity(
        identity: &CertificateIdentity,
        plane: SurfacePlane,
    ) -> AndromedaResult<Self> {
        let required_scope = plane_to_required_surface_scope(plane);
        if identity.surface != required_scope {
            return Err(pool_error(
                AndromedaErrorKind::Security,
                "server identity surface scope does not match pool plane",
            ));
        }
        Self::new(identity.fingerprint.clone(), plane)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.server_fingerprint.trim().is_empty() {
            return Err(pool_error(
                AndromedaErrorKind::Security,
                "server identity fingerprint cannot be empty",
            ));
        }
        Ok(())
    }
}

/// Runtime-free connection pool limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectionPoolPolicy {
    pub max_connections_per_key: u16,
    pub idle_timeout_ms: u64,
    pub evict_unhealthy: bool,
}

impl ConnectionPoolPolicy {
    pub const fn conservative() -> Self {
        Self {
            max_connections_per_key: 8,
            idle_timeout_ms: 30_000,
            evict_unhealthy: true,
        }
    }

    pub fn validate(self) -> AndromedaResult<()> {
        if self.max_connections_per_key == 0
            || self.max_connections_per_key > MAX_POOL_CONNECTIONS_PER_KEY
        {
            return Err(pool_error(
                AndromedaErrorKind::Resource,
                "pool max_connections_per_key must be in 1..=MAX_POOL_CONNECTIONS_PER_KEY",
            ));
        }
        if self.idle_timeout_ms == 0 || self.idle_timeout_ms > MAX_POOL_IDLE_TIMEOUT_MS {
            return Err(pool_error(
                AndromedaErrorKind::Resource,
                "pool idle_timeout_ms must be in 1..=MAX_POOL_IDLE_TIMEOUT_MS",
            ));
        }
        Ok(())
    }
}

/// Opaque runtime-free identifier for a pooled connection handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PoolConnectionId(u64);

impl PoolConnectionId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PooledConnectionHealth {
    Healthy,
    Unhealthy,
}

impl PooledConnectionHealth {
    pub const fn is_healthy(self) -> bool {
        matches!(self, Self::Healthy)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PooledConnection {
    pub id: PoolConnectionId,
    pub key: ConnectionPoolKey,
    pub health: PooledConnectionHealth,
    pub last_used_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolAdmissionKind {
    Reused,
    Opened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolAdmission {
    pub connection_id: PoolConnectionId,
    pub kind: PoolAdmissionKind,
}

/// Runtime-free pool model for reconnect admission.
///
/// The pool stores only deterministic metadata. Quinn connections, timers, and
/// wakeups are owned by runtime wiring and projected into this contract through
/// `now_ms` timestamps and health updates.
#[derive(Debug, Clone)]
pub struct ConnectionPool {
    policy: ConnectionPoolPolicy,
    next_id: u64,
    entries: HashMap<PoolConnectionId, PooledConnection>,
}

impl ConnectionPool {
    pub fn new(policy: ConnectionPoolPolicy) -> AndromedaResult<Self> {
        policy.validate()?;
        Ok(Self {
            policy,
            next_id: 1,
            entries: HashMap::new(),
        })
    }

    pub const fn policy(&self) -> ConnectionPoolPolicy {
        self.policy
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, id: PoolConnectionId) -> bool {
        self.entries.contains_key(&id)
    }

    pub fn count_for_key(&self, key: &ConnectionPoolKey) -> usize {
        self.entries
            .values()
            .filter(|entry| &entry.key == key)
            .count()
    }

    pub fn connection(&self, id: PoolConnectionId) -> Option<&PooledConnection> {
        self.entries.get(&id)
    }

    pub fn admit_or_reuse(
        &mut self,
        key: ConnectionPoolKey,
        now_ms: u64,
    ) -> AndromedaResult<PoolAdmission> {
        key.validate()?;
        self.evict_idle(now_ms);
        if self.policy.evict_unhealthy {
            self.evict_unhealthy();
        }

        if let Some(connection_id) = self.reusable_connection_id(&key) {
            let entry = self
                .entries
                .get_mut(&connection_id)
                .expect("reusable connection id must still exist");
            entry.last_used_ms = now_ms;
            return Ok(PoolAdmission {
                connection_id,
                kind: PoolAdmissionKind::Reused,
            });
        }

        if self.count_for_key(&key) >= self.policy.max_connections_per_key as usize {
            return Err(pool_error(
                AndromedaErrorKind::Resource,
                "connection pool capacity reached for server identity and plane",
            ));
        }

        let connection_id = self.next_connection_id()?;
        self.entries.insert(
            connection_id,
            PooledConnection {
                id: connection_id,
                key,
                health: PooledConnectionHealth::Healthy,
                last_used_ms: now_ms,
            },
        );
        Ok(PoolAdmission {
            connection_id,
            kind: PoolAdmissionKind::Opened,
        })
    }

    pub fn mark_used(&mut self, id: PoolConnectionId, now_ms: u64) -> AndromedaResult<()> {
        let entry = self.entries.get_mut(&id).ok_or_else(|| {
            pool_error(
                AndromedaErrorKind::Protocol,
                "pooled connection id is not registered",
            )
        })?;
        entry.last_used_ms = now_ms;
        Ok(())
    }

    pub fn mark_unhealthy(&mut self, id: PoolConnectionId) -> AndromedaResult<()> {
        let entry = self.entries.get_mut(&id).ok_or_else(|| {
            pool_error(
                AndromedaErrorKind::Protocol,
                "pooled connection id is not registered",
            )
        })?;
        entry.health = PooledConnectionHealth::Unhealthy;
        Ok(())
    }

    pub fn evict_idle(&mut self, now_ms: u64) -> usize {
        let before = self.entries.len();
        let idle_timeout_ms = self.policy.idle_timeout_ms;
        self.entries
            .retain(|_, entry| now_ms.saturating_sub(entry.last_used_ms) < idle_timeout_ms);
        before - self.entries.len()
    }

    pub fn evict_unhealthy(&mut self) -> usize {
        let before = self.entries.len();
        self.entries
            .retain(|_, entry| entry.health == PooledConnectionHealth::Healthy);
        before - self.entries.len()
    }

    fn reusable_connection_id(&self, key: &ConnectionPoolKey) -> Option<PoolConnectionId> {
        self.entries
            .iter()
            .filter(|(_, entry)| &entry.key == key && entry.health.is_healthy())
            .min_by_key(|(_, entry)| entry.last_used_ms)
            .map(|(id, _)| *id)
    }

    fn next_connection_id(&mut self) -> AndromedaResult<PoolConnectionId> {
        if self.next_id == u64::MAX {
            return Err(pool_error(
                AndromedaErrorKind::Resource,
                "pooled connection id space exhausted",
            ));
        }
        let id = PoolConnectionId::new(self.next_id);
        self.next_id += 1;
        Ok(id)
    }
}

/// Request retry idempotency declared by the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryIdempotency {
    Idempotent,
    NonIdempotent,
}

impl RetryIdempotency {
    pub const fn is_idempotent(self) -> bool {
        matches!(self, Self::Idempotent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryAdmissionPolicy {
    pub require_idempotent: bool,
}

impl RetryAdmissionPolicy {
    pub const fn idempotent_only() -> Self {
        Self {
            require_idempotent: true,
        }
    }

    pub fn admit_after_failure(
        self,
        reconnect_state: ReconnectState,
        reconnect_policy: ReconnectPolicy,
        idempotency: RetryIdempotency,
        failed_attempt: u32,
    ) -> AndromedaResult<RetryAdmissionDecision> {
        if reconnect_state.allows_new_requests() {
            return Ok(RetryAdmissionDecision::Reject(
                RetryRejectionReason::ConnectionAvailable,
            ));
        }
        if reconnect_state.is_terminal() {
            return Ok(RetryAdmissionDecision::Reject(
                RetryRejectionReason::ConnectionTerminal,
            ));
        }
        if self.require_idempotent && !idempotency.is_idempotent() {
            return Ok(RetryAdmissionDecision::Reject(
                RetryRejectionReason::NonIdempotentRequest,
            ));
        }

        match reconnect_policy.decision_after_failure(failed_attempt)? {
            ReconnectDecision::RetryAfter {
                next_attempt,
                delay_ms,
            } => Ok(RetryAdmissionDecision::Admit {
                next_attempt,
                delay_ms,
            }),
            ReconnectDecision::GiveUp => Ok(RetryAdmissionDecision::Reject(
                RetryRejectionReason::AttemptsExhausted,
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryAdmissionDecision {
    Admit { next_attempt: u32, delay_ms: u64 },
    Reject(RetryRejectionReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryRejectionReason {
    ConnectionAvailable,
    ConnectionTerminal,
    NonIdempotentRequest,
    AttemptsExhausted,
}

fn reconnect_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

fn pool_error(kind: AndromedaErrorKind, message: &'static str) -> AndromedaError {
    AndromedaError::new(kind, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_observe::SurfaceScope;

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
                .server_fingerprint,
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
}
