//! Compatibility facade for retry ownership.
//!
//! `andromeda-retry` owns retry classification, policies, decisions, and
//! attempt evidence. `andromeda-exec` reexports those types so existing callers
//! can migrate without duplicating retry logic in the execution crate.

pub use andromeda_retry::{ErrorRetryability, RetryAttempt, RetryDecision, RetryPolicy};
