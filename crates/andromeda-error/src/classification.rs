//! P04 Doctrinal Error Classification Matrix.
//!
//! Maps each of the 12 [`AndromedaErrorKind`] variants to one of the 6 P04
//! doctrinal error classes defined in `P04_GENERIC_PROCEDURE_EXECUTION.md`.
//!
//! # Doctrinal Classes
//!
//! | Class         | Description                                               |
//! |---------------|-----------------------------------------------------------|
//! | `Business`    | User-visible procedure logic failure (validation, domain) |
//! | `Contract`    | Structural/schema violation before or during execution    |
//! | `Permission`  | Security, IAM, or authorization denial                   |
//! | `Transaction` | Transaction-layer failure (deadlock victim, timeout)      |
//! | `Resource`    | Budget, capacity, or infrastructure exhaustion            |
//! | `System`      | Internal, storage, protocol, or transport failure         |
//!
//! # Canonical Mapping
//!
//! | `AndromedaErrorKind` | Doctrinal Class |
//! |----------------------|-----------------|
//! | `Execution`          | `Business`      |
//! | `Contract`           | `Contract`      |
//! | `Catalog`            | `Contract`      |
//! | `Srpl`               | `Contract`      |
//! | `Security`           | `Permission`    |
//! | `Transaction`        | `Transaction`   |
//! | `Timeout`            | `Transaction`   |
//! | `Resource`           | `Resource`      |
//! | `Internal`           | `System`        |
//! | `Storage`            | `System`        |
//! | `Protocol`           | `System`        |
//! | `Transport`          | `System`        |
//!
//! # Deadlock vs. Generic Transaction Retryability
//!
//! `Transaction`-class errors are **not** retried at the invocation level
//! (`ErrorRetryability::classify(Transaction) == Persistent`). This is the
//! external contract: once a transaction error surfaces, the invocation is
//! terminal. However, the transaction-error routing layer internally retries
//! deadlock victims *before* surfacing the error — see
//! [`andromeda_retry::ExecutionErrorRetryability`] for that distinction.

use crate::AndromedaErrorKind;

/// The six P04 doctrinal error classes.
///
/// Each [`AndromedaErrorKind`] variant maps to exactly one class via
/// [`classify_andromeda_error`]. The mapping is total: every kind has a class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionErrorClass {
    /// User-visible procedure logic failure.
    ///
    /// Maps from: `Execution`.
    Business,

    /// Structural or schema violation before or during execution.
    ///
    /// Maps from: `Contract`, `Catalog`, `Srpl`.
    Contract,

    /// Security, IAM, or authorization denial.
    ///
    /// Maps from: `Security`.
    Permission,

    /// Transaction-layer failure (deadlock victim, timeout, WAL conflict).
    ///
    /// Maps from: `Transaction`, `Timeout`.
    ///
    /// # Note on Deadlock Retryability
    ///
    /// A `Transaction`-class error produced by a deadlock victim is retryable
    /// at the transaction-routing layer *before* it surfaces to callers. Once
    /// the error reaches `classify_andromeda_error`, it is already a terminal
    /// outcome. For deadlock-specific routing, see
    /// [`andromeda_retry::ExecutionErrorRetryability`].
    Transaction,

    /// Resource budget, capacity, or infrastructure exhaustion.
    ///
    /// Maps from: `Resource`.
    Resource,

    /// Internal infrastructure, storage, or transport failure.
    ///
    /// Maps from: `Internal`, `Storage`, `Protocol`, `Transport`.
    System,
}

/// Classify an [`AndromedaErrorKind`] into its P04 doctrinal class.
///
/// This function is the single, canonical conversion matrix. It is total:
/// every variant of [`AndromedaErrorKind`] maps to exactly one
/// [`ExecutionErrorClass`].
///
/// # Examples
///
/// ```rust
/// use andromeda_error::{AndromedaErrorKind, ExecutionErrorClass, classify_andromeda_error};
///
/// assert_eq!(classify_andromeda_error(AndromedaErrorKind::Security), ExecutionErrorClass::Permission);
/// assert_eq!(classify_andromeda_error(AndromedaErrorKind::Transaction), ExecutionErrorClass::Transaction);
/// assert_eq!(classify_andromeda_error(AndromedaErrorKind::Resource), ExecutionErrorClass::Resource);
/// ```
pub const fn classify_andromeda_error(kind: AndromedaErrorKind) -> ExecutionErrorClass {
    match kind {
        // Business: user-visible logic failures
        AndromedaErrorKind::Execution => ExecutionErrorClass::Business,

        // Contract: structural/schema violations
        AndromedaErrorKind::Contract | AndromedaErrorKind::Catalog | AndromedaErrorKind::Srpl => {
            ExecutionErrorClass::Contract
        },

        // Permission: security and authorization
        AndromedaErrorKind::Security => ExecutionErrorClass::Permission,

        // Transaction: transaction-layer failures including timeouts
        // NOTE: Timeout is classified here because it represents budget exhaustion
        // at the transaction boundary. It is Persistent in ErrorRetryability.
        AndromedaErrorKind::Transaction | AndromedaErrorKind::Timeout => {
            ExecutionErrorClass::Transaction
        },

        // Resource: capacity and budget exhaustion
        AndromedaErrorKind::Resource => ExecutionErrorClass::Resource,

        // System: infrastructure, storage, protocol, transport
        AndromedaErrorKind::Internal
        | AndromedaErrorKind::Storage
        | AndromedaErrorKind::Protocol
        | AndromedaErrorKind::Transport => ExecutionErrorClass::System,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Property test: `classify_andromeda_error` is total — every variant maps
    /// to exactly one class and the function never panics.
    #[test]
    fn classification_is_total_for_all_12_kinds() {
        let all_kinds = [
            AndromedaErrorKind::Catalog,
            AndromedaErrorKind::Contract,
            AndromedaErrorKind::Execution,
            AndromedaErrorKind::Internal,
            AndromedaErrorKind::Protocol,
            AndromedaErrorKind::Resource,
            AndromedaErrorKind::Security,
            AndromedaErrorKind::Srpl,
            AndromedaErrorKind::Storage,
            AndromedaErrorKind::Timeout,
            AndromedaErrorKind::Transaction,
            AndromedaErrorKind::Transport,
        ];

        // Verify totality: all 12 variants produce a class without panic.
        for kind in all_kinds {
            let _ = classify_andromeda_error(kind);
        }
    }

    #[test]
    fn classify_business_errors() {
        assert_eq!(
            classify_andromeda_error(AndromedaErrorKind::Execution),
            ExecutionErrorClass::Business
        );
    }

    #[test]
    fn classify_contract_errors() {
        assert_eq!(
            classify_andromeda_error(AndromedaErrorKind::Contract),
            ExecutionErrorClass::Contract
        );
        assert_eq!(
            classify_andromeda_error(AndromedaErrorKind::Catalog),
            ExecutionErrorClass::Contract
        );
        assert_eq!(
            classify_andromeda_error(AndromedaErrorKind::Srpl),
            ExecutionErrorClass::Contract
        );
    }

    #[test]
    fn classify_permission_errors() {
        assert_eq!(
            classify_andromeda_error(AndromedaErrorKind::Security),
            ExecutionErrorClass::Permission
        );
    }

    #[test]
    fn classify_transaction_errors_including_timeout() {
        assert_eq!(
            classify_andromeda_error(AndromedaErrorKind::Transaction),
            ExecutionErrorClass::Transaction
        );
        // Timeout is classified as Transaction: it is budget-exhaustion at the
        // transaction boundary, not a system failure.
        assert_eq!(
            classify_andromeda_error(AndromedaErrorKind::Timeout),
            ExecutionErrorClass::Transaction
        );
    }

    #[test]
    fn classify_resource_errors() {
        assert_eq!(
            classify_andromeda_error(AndromedaErrorKind::Resource),
            ExecutionErrorClass::Resource
        );
    }

    #[test]
    fn classify_system_errors() {
        for kind in [
            AndromedaErrorKind::Internal,
            AndromedaErrorKind::Storage,
            AndromedaErrorKind::Protocol,
            AndromedaErrorKind::Transport,
        ] {
            assert_eq!(
                classify_andromeda_error(kind),
                ExecutionErrorClass::System,
                "expected System class for {kind:?}"
            );
        }
    }

    /// Property: no doctrinal class is empty — every class is reachable from
    /// at least one `AndromedaErrorKind`.
    #[test]
    fn all_six_doctrinal_classes_are_reachable() {
        let all_kinds = [
            AndromedaErrorKind::Catalog,
            AndromedaErrorKind::Contract,
            AndromedaErrorKind::Execution,
            AndromedaErrorKind::Internal,
            AndromedaErrorKind::Protocol,
            AndromedaErrorKind::Resource,
            AndromedaErrorKind::Security,
            AndromedaErrorKind::Srpl,
            AndromedaErrorKind::Storage,
            AndromedaErrorKind::Timeout,
            AndromedaErrorKind::Transaction,
            AndromedaErrorKind::Transport,
        ];

        let classes: HashSet<String> = all_kinds
            .iter()
            .map(|k| format!("{:?}", classify_andromeda_error(*k)))
            .collect();

        assert_eq!(
            classes.len(),
            6,
            "expected all 6 doctrinal classes to be reachable, got {:?}",
            classes
        );
    }
}
