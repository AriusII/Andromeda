//! Compatibility re-exports for strict 2PL transaction protocol validation.
//!
//! The implementation lives in `andromeda-transaction`; `andromeda-tx` keeps
//! this module as the legacy import path while extraction continues.

pub use andromeda_transaction::{TwoPhaseLocksValidator, TwoPhaseOperation};
