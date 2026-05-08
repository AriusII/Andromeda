//! Compatibility re-exports for MVCC active snapshot tracking.
//!
//! The implementation lives in `andromeda-mvcc`; `andromeda-tx` keeps this
//! module as the legacy import path while extraction continues.

pub use andromeda_mvcc::{ActiveSnapshotRegistry, GcError, SnapshotHandle};
