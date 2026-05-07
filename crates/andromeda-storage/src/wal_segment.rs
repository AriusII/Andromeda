//! Legacy storage WAL segment compatibility surface.
//!
//! WAL segment descriptors and bundled segment values now live in
//! `andromeda_wal::wal_segment`. Storage keeps these reexports for existing
//! `andromeda_storage::WalSegment*` paths during migration.

pub use andromeda_wal::wal_segment::{WalSegment, WalSegmentDescriptor};
