use std::collections::HashMap;

use andromeda_core::{AndromedaErrorKind, AndromedaResult};

use super::{
    ConnectionPoolKey, ConnectionPoolPolicy, PoolAdmission, PoolAdmissionKind, PoolConnectionId,
    PooledConnection, PooledConnectionHealth, pool_error,
};

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
