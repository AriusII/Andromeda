//! Retry semantics and transient error classification tests.
//!
//! This integration crate validates transient/persistent error classification,
//! deterministic exponential backoff, bounded retry attempts, and observable
//! audit logging of retry failures.

#[path = "retry_semantics/audit_ledger.rs"]
mod audit_ledger;
#[path = "retry_semantics/classification.rs"]
mod classification;
#[path = "retry_semantics/policy.rs"]
mod policy;
#[path = "retry_semantics/scenarios.rs"]
mod scenarios;
#[path = "retry_semantics/support.rs"]
mod support;
