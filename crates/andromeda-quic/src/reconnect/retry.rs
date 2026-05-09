use andromeda_error::AndromedaResult;

use super::{ReconnectDecision, ReconnectPolicy, ReconnectState};

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
