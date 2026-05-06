use andromeda_core::{AndromedaErrorKind, AndromedaResult};

use super::pool_error;

/// Maximum idle timeout accepted by the runtime-free connection pool contract.
pub const MAX_POOL_IDLE_TIMEOUT_MS: u64 = 15 * 60 * 1_000;

/// Maximum number of live pooled connections for one server identity and plane.
pub const MAX_POOL_CONNECTIONS_PER_KEY: u16 = 128;

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
