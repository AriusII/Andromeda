#![forbid(unsafe_code)]

//! Retry and Reconnection Semantics for Transient Error Handling
//!
//! This module implements transparent retry logic for procedure invocation with
//! exponential backoff and transient/persistent error classification.
//!
//! ## Error Classification
//!
//! Errors are classified as either:
//! - **Transient**: Network timeout, temporary resource unavailable, session unavailable
//!   - Eligible for retry with exponential backoff
//! - **Persistent**: Permission denied, contract rejected, logic error
//!   - Fail immediately without retry
//!
//! ## Retry Policy
//!
//! Specifies backoff parameters (initial delay, max delay, max attempts) with
//! exponential backoff: `delay = min(initial * 2^(attempt-1), max)`.
//!
//! Conservative default: 100ms initial, 5s max, 8 attempts (16.3s total).
//!
//! ## Audit Integration
//!
//! Every retry attempt is logged to the durable audit ledger with:
//! - Trace ID (correlation)
//! - Error kind and message
//! - Retry decision (retry after Xms or give up)
//!
//! Doctrine gates:
//! - Persistent errors never retried (logic error always fails immediately)
//! - Max retries enforced (no infinite retry)
//! - Retry is observable in audit trace

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Error classification for retry eligibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorRetryability {
    /// Error is transient; retry is eligible.
    Transient,
    /// Error is persistent; do not retry.
    Persistent,
}

impl ErrorRetryability {
    /// Classify an error based on its kind.
    pub const fn classify(kind: AndromedaErrorKind) -> Self {
        match kind {
            // Transient: may succeed on retry
            AndromedaErrorKind::Transport => Self::Transient,
            AndromedaErrorKind::Resource => Self::Transient,
            AndromedaErrorKind::Protocol => Self::Transient,

            // Persistent: will fail again; stop immediately
            AndromedaErrorKind::Security => Self::Persistent,
            AndromedaErrorKind::Contract => Self::Persistent,
            AndromedaErrorKind::Srpl => Self::Persistent,
            AndromedaErrorKind::Execution => Self::Persistent,
            AndromedaErrorKind::Catalog => Self::Persistent,
            AndromedaErrorKind::Storage => Self::Persistent,
            AndromedaErrorKind::Transaction => Self::Persistent,
            AndromedaErrorKind::Internal => Self::Persistent,

            // Timeout is Persistent: the invocation deadline has been consumed.
            // The standard RetryPolicy must not re-issue a timed-out invocation.
            // Lock-wait retries are handled internally before this error surfaces.
            // See SCOPED_INVOCATION_TIMEOUT.md section 6 (Timeout and retry interaction).
            AndromedaErrorKind::Timeout => Self::Persistent,
        }
    }

    pub const fn is_transient(self) -> bool {
        matches!(self, Self::Transient)
    }

    pub const fn is_persistent(self) -> bool {
        matches!(self, Self::Persistent)
    }
}

/// Runtime-free retry policy defining backoff and attempt bounds.
///
/// This type intentionally models only deterministic scheduling rules. Timer
/// execution, sleep, and request retry mechanics belong to the caller or
/// async runtime wiring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
    pub max_attempts: u32,
    /// Jitter range in parts per million (e.g., 100_000 = 10%).
    pub jitter_ppm: u32,
}

impl RetryPolicy {
    /// Conservative default retry policy (100ms initial, 5s max, 8 attempts).
    pub const fn conservative() -> Self {
        Self {
            initial_backoff_ms: 100,
            max_backoff_ms: 5_000,
            max_attempts: 8,
            jitter_ppm: 100_000, // 10% jitter
        }
    }

    /// Aggressive retry policy (50ms initial, 1s max, 3 attempts).
    pub const fn aggressive() -> Self {
        Self {
            initial_backoff_ms: 50,
            max_backoff_ms: 1_000,
            max_attempts: 3,
            jitter_ppm: 50_000, // 5% jitter
        }
    }

    /// Minimal retry policy (10ms initial, 100ms max, 2 attempts).
    pub const fn minimal() -> Self {
        Self {
            initial_backoff_ms: 10,
            max_backoff_ms: 100,
            max_attempts: 2,
            jitter_ppm: 0, // no jitter
        }
    }

    /// Validate the policy against Andromeda retry contract.
    pub fn validate(self) -> AndromedaResult<()> {
        if self.initial_backoff_ms == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "retry initial_backoff_ms must be non-zero",
            ));
        }
        if self.max_backoff_ms < self.initial_backoff_ms {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "retry max_backoff_ms must be >= initial_backoff_ms",
            ));
        }
        if self.max_attempts == 0 || self.max_attempts > 32 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "retry max_attempts must be in 1..=32",
            ));
        }
        if self.jitter_ppm > 1_000_000 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "retry jitter_ppm must be <= 1_000_000",
            ));
        }
        Ok(())
    }

    /// Compute base backoff delay for a given attempt number (1-indexed).
    pub fn delay_for_attempt(self, attempt: u32) -> AndromedaResult<u64> {
        self.validate()?;
        if attempt == 0 || attempt > self.max_attempts {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "retry attempt must be in 1..=max_attempts",
            ));
        }

        // Exponential backoff: 2^(attempt-1) multiplier, capped at max
        let shift = attempt.saturating_sub(1).min(63);
        let multiplier = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
        Ok(self
            .initial_backoff_ms
            .saturating_mul(multiplier)
            .min(self.max_backoff_ms))
    }

    /// Decision after a transient error on a given attempt.
    pub fn decision_after_failure(self, attempt: u32) -> AndromedaResult<RetryDecision> {
        self.validate()?;
        if attempt >= self.max_attempts {
            return Ok(RetryDecision::GiveUp);
        }
        Ok(RetryDecision::RetryAfter {
            next_attempt: attempt.saturating_add(1),
            delay_ms: self.delay_for_attempt(attempt.saturating_add(1))?,
        })
    }
}

/// Decision made after a transient error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Attempt another retry after this delay.
    RetryAfter { next_attempt: u32, delay_ms: u64 },
    /// Give up; treat as terminal error.
    GiveUp,
}

impl RetryDecision {
    pub const fn is_retry(self) -> bool {
        matches!(self, Self::RetryAfter { .. })
    }

    pub const fn is_give_up(self) -> bool {
        matches!(self, Self::GiveUp)
    }
}

/// Retry attempt tracking and audit data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryAttempt {
    pub trace_id: andromeda_observe::TraceId,
    pub attempt_number: u32,
    pub error_kind: AndromedaErrorKind,
    pub error_message: String,
    pub next_delay_ms: Option<u64>,
    pub decision: RetryDecision,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_retryability_classify_transient() {
        assert_eq!(
            ErrorRetryability::classify(AndromedaErrorKind::Transport),
            ErrorRetryability::Transient
        );
        assert_eq!(
            ErrorRetryability::classify(AndromedaErrorKind::Resource),
            ErrorRetryability::Transient
        );
        assert_eq!(
            ErrorRetryability::classify(AndromedaErrorKind::Protocol),
            ErrorRetryability::Transient
        );
    }

    #[test]
    fn error_retryability_classify_persistent() {
        assert_eq!(
            ErrorRetryability::classify(AndromedaErrorKind::Security),
            ErrorRetryability::Persistent
        );
        assert_eq!(
            ErrorRetryability::classify(AndromedaErrorKind::Contract),
            ErrorRetryability::Persistent
        );
        assert_eq!(
            ErrorRetryability::classify(AndromedaErrorKind::Srpl),
            ErrorRetryability::Persistent
        );
    }

    #[test]
    fn retry_policy_conservative() {
        let policy = RetryPolicy::conservative();
        assert_eq!(policy.initial_backoff_ms, 100);
        assert_eq!(policy.max_backoff_ms, 5_000);
        assert_eq!(policy.max_attempts, 8);
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn retry_policy_exponential_backoff() {
        let policy = RetryPolicy::conservative();
        assert_eq!(policy.delay_for_attempt(1).unwrap(), 100);
        assert_eq!(policy.delay_for_attempt(2).unwrap(), 200);
        assert_eq!(policy.delay_for_attempt(3).unwrap(), 400);
        assert_eq!(policy.delay_for_attempt(4).unwrap(), 800);
        assert_eq!(policy.delay_for_attempt(5).unwrap(), 1600);
        assert_eq!(policy.delay_for_attempt(6).unwrap(), 3200);
        assert_eq!(policy.delay_for_attempt(7).unwrap(), 5000); // capped
        assert_eq!(policy.delay_for_attempt(8).unwrap(), 5000); // capped
    }

    #[test]
    fn retry_policy_max_attempts_enforced() {
        let policy = RetryPolicy::conservative();
        let decision = policy.decision_after_failure(8).unwrap();
        assert_eq!(decision, RetryDecision::GiveUp);

        let decision = policy.decision_after_failure(7).unwrap();
        assert!(matches!(
            decision,
            RetryDecision::RetryAfter {
                next_attempt: 8,
                ..
            }
        ));
    }

    #[test]
    fn retry_policy_validation_initial_zero() {
        let policy = RetryPolicy {
            initial_backoff_ms: 0,
            max_backoff_ms: 1000,
            max_attempts: 3,
            jitter_ppm: 0,
        };
        assert!(policy.validate().is_err());
    }

    #[test]
    fn retry_policy_validation_max_less_than_initial() {
        let policy = RetryPolicy {
            initial_backoff_ms: 1000,
            max_backoff_ms: 500,
            max_attempts: 3,
            jitter_ppm: 0,
        };
        assert!(policy.validate().is_err());
    }

    #[test]
    fn retry_decision_is_retry() {
        let decision = RetryDecision::RetryAfter {
            next_attempt: 2,
            delay_ms: 100,
        };
        assert!(decision.is_retry());
        assert!(!decision.is_give_up());
    }

    #[test]
    fn retry_decision_is_give_up() {
        let decision = RetryDecision::GiveUp;
        assert!(!decision.is_retry());
        assert!(decision.is_give_up());
    }
}
