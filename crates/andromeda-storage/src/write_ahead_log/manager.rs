//! Compatibility reexports for pure in-memory WAL management.
//!
//! The canonical `InMemoryWal` implementation moved to
//! `andromeda_wal::write_ahead_log::manager`. Storage keeps this shim so older
//! imports continue to resolve during migration.

pub use andromeda_wal::write_ahead_log::manager::{InMemoryWal, MemoryWal};
