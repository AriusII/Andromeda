//! Compatibility re-exports for transaction transition tracing.
//!
//! The implementation lives in `andromeda-transaction`; `andromeda-tx` keeps
//! this module as the legacy import path while extraction continues.

pub use andromeda_transaction::{
    TransactionTrace, TransactionTransitionCorrelation, transaction_phase_code,
};
