//! Protocol error envelope taxonomy.

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorFamily {
    Protocol,
    Authentication,
    Authorization,
    Contract,
    Semantic,
    Execution,
    Transaction,
    Storage,
    Resource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorEnvelope {
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
    pub trace_id: Option<String>,
    pub family: ErrorFamily,
    pub code: String,
    pub message: String,
    pub transaction_effect: TransactionEffect,
    pub retry_disposition: RetryDisposition,
    pub retry_after_ms: Option<u64>,
    pub backpressure: Option<BackpressureMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionEffect {
    NoTransaction,
    RollbackRequired,
    FailStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDisposition {
    NotRetryable,
    Retryable,
    RetryAfter,
    Backpressure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackpressureMetadata {
    pub retry_after_ms: Option<u64>,
    pub capacity_percent: Option<u8>,
    pub shed_load: bool,
}

impl BackpressureMetadata {
    pub fn validate(&self) -> AndromedaResult<()> {
        if matches!(self.capacity_percent, Some(percent) if percent > 100) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "backpressure capacity percent must be <= 100",
            ));
        }

        Ok(())
    }
}

impl ErrorEnvelope {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.code.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "error envelope code must not be empty",
            ));
        }

        if self.message.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "error envelope message must not be empty",
            ));
        }

        if matches!(self.trace_id.as_deref(), Some(trace_id) if trace_id.trim().is_empty()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "error trace correlation id must not be empty when present",
            ));
        }

        if let Some(backpressure) = self.backpressure {
            backpressure.validate()?;
        }

        if self.retry_disposition == RetryDisposition::RetryAfter
            && self.retry_after_ms.is_none()
            && self
                .backpressure
                .and_then(|hint| hint.retry_after_ms)
                .is_none()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "retry-after error must include retry delay metadata",
            ));
        }

        if self.retry_disposition == RetryDisposition::Backpressure && self.backpressure.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Resource,
                "backpressure error must include backpressure metadata",
            ));
        }

        Ok(())
    }
}
