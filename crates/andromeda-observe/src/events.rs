//! Observability event contracts.
//!
//! Every emitted event is tied to a non-zero [`TraceId`] and validated through
//! [`EventEnvelope::validate`] before it enters a sink. The important invariants
//! live with the specific event modules: recovery events carry durable LSN
//! evidence, security/admission events are observable on both allow and deny
//! paths, and text fields reject obvious secret markers.

#[cfg(test)]
use crate::TraceId;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
#[cfg(test)]
use andromeda_core::{CatalogObjectId, CatalogVersion, InvocationId, TransactionId};

mod admission_audit;
mod audit;
mod backup_audit;
mod core_trace;
mod correlation;
mod decision;
mod durability;
mod durable_audit;
mod envelope;
mod family;
mod hadr_audit;
mod identity;
mod protocol;
mod protocol_rejection;
mod sequence;
mod sink;
mod transition;

pub use admission_audit::*;
pub use audit::*;
pub use backup_audit::*;
pub use core_trace::*;
pub use correlation::*;
pub use decision::*;
pub use durability::*;
pub use durable_audit::*;
pub use envelope::*;
pub use family::*;
pub use hadr_audit::*;
pub use identity::*;
pub use protocol::*;
pub use protocol_rejection::*;
pub use sequence::*;
pub use sink::*;
pub use transition::*;

pub(crate) fn observe_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Internal, message)
}

pub(super) fn non_empty_reason(reason: impl Into<String>) -> AndromedaResult<String> {
    let reason = reason.into();
    if reason.trim().is_empty() {
        return Err(observe_error(
            "observability decision evidence requires a non-empty reason",
        ));
    }

    Ok(reason)
}

pub(super) fn contains_sensitive_marker(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    [
        "-----begin",
        "private key",
        "private_key",
        "bearer ",
        "credential=",
        "password=",
        "passwd=",
        "secret=",
        "token=",
        "authorization:",
        "x-api-key",
        "payload:",
        "payload body",
    ]
    .iter()
    .any(|marker| lowered.contains(marker))
}

pub(super) fn redact_sensitive_evidence(text: &str) -> String {
    if contains_sensitive_marker(text) {
        "[redacted-sensitive-evidence]".to_string()
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests;
