//! Compatibility re-exports for transaction state.
//!
//! The implementation lives in `andromeda-transaction`; `andromeda-tx` keeps
//! this module as the legacy import path while extraction continues.

pub use andromeda_transaction::{TransactionEvent, TransactionState, TransactionStateMachine};
