//! Compatibility facade for the native WAL LSN type.
//!
//! The canonical `Lsn` newtype now lives in `andromeda_wal`. Storage keeps
//! this module so historical `andromeda_storage::Lsn` imports continue to
//! resolve during the migration.

pub use andromeda_wal::Lsn;
