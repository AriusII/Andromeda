//! Compatibility re-export for transaction-log LSNs.
//!
//! The boundary type lives in `andromeda-transaction-log`; `andromeda-tx` keeps
//! this module as the legacy import path while extraction continues.

pub use andromeda_transaction_log::Lsn;
