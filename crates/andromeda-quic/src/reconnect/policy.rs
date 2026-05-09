use andromeda_error::AndromedaResult;

use super::{ReconnectDecision, reconnect_error};

/// Maximum reconnect backoff accepted by the runtime-free contract.
pub const MAX_RECONNECT_BACKOFF_MS: u64 = 60_000;

/// Maximum reconnect attempts accepted by the runtime-free contract.
pub const MAX_RECONNECT_ATTEMPTS: u32 = 32;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReconnectAttemptTrace {
    pub attempt_number: u32,
    pub delay_ms: u64,
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

    pub fn attempt_trace(self, attempt: u32) -> AndromedaResult<ReconnectAttemptTrace> {
        Ok(ReconnectAttemptTrace {
            attempt_number: attempt,
            delay_ms: self.delay_for_attempt(attempt)?,
            jitter_ppm: self.jitter_ppm,
        })
    }

    pub fn jitter_bounds_for_attempt(self, attempt: u32) -> AndromedaResult<(u64, u64)> {
        let base_delay = self.delay_for_attempt(attempt)?;
        let jitter = base_delay.saturating_mul(self.jitter_ppm as u64) / 1_000_000;
        Ok((base_delay, base_delay.saturating_add(jitter)))
    }

    pub fn cumulative_retry_budget_ms(self) -> AndromedaResult<u64> {
        self.validate()?;
        let mut total = 0u64;
        for attempt in 2..=self.max_attempts {
            total = total.saturating_add(self.delay_for_attempt(attempt)?);
        }
        Ok(total)
    }
}
