use andromeda_core::{InvocationId, TransactionId};

/// Derive a `TransactionId` from an `InvocationId` by direct value reuse.
///
/// **Deprecated.** This helper conflates two unrelated identifier domains and
/// is incompatible with recovery: after restart the WAL recovery driver
/// cannot guarantee monotonicity if transaction ids are minted from the
/// invocation namespace.
///
/// Production code must use [`andromeda_transaction::TransactionManager`] (which owns
/// a recovery-safe [`andromeda_transaction::TransactionIdAllocator`]) to begin
/// transactions and obtain ids. As of the V0 exec migration the runtime
/// (`LocalVerticalRuntime` and the inventory demo V0 wrapper)
/// allocates ids exclusively via `TransactionManager::begin`, so the only
/// remaining call sites for this shim are:
///
/// * the legacy [`crate::services::CompletionRecoveryExpectation::for_invocation`]
///   convenience constructor, retained for compatibility with byte-stable
///   recovery test fixtures and gated behind `#[allow(deprecated)]`; and
/// * `#[cfg(test)]` modules in `services::completion` that hand-craft WAL
///   records keyed by InvocationId-derived TransactionIds to exercise
///   reconciliation without spinning up a manager.
///
/// New production code paths must not call this function and must instead
/// use [`andromeda_transaction::TransactionManager::begin`] (or
/// [`crate::services::CompletionRecoveryExpectation::for_invocation_with_transaction`]
/// for explicit recovery expectations).
#[deprecated(
    since = "0.1.0",
    note = "use andromeda_transaction::TransactionManager::begin to allocate \
            recovery-safe TransactionIds; deriving them from InvocationId \
            breaks monotonicity across restarts"
)]
pub const fn transaction_id_for_invocation(invocation_id: InvocationId) -> TransactionId {
    TransactionId::new(invocation_id.get())
}
