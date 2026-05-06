use super::ConnectionPoolKey;

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
