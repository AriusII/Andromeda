//! Compatibility reexports for WAL segment garbage collection ownership.
//!
//! Canonical GC policy/types now live in `andromeda_wal::write_ahead_log::gc`.
//! Storage keeps this shim for stable import paths during extraction.

pub use andromeda_wal::write_ahead_log::gc::{
    ArchiveStatus, WalGarbageCollector, WalGcAuditEvent, WalGcCandidate, WalGcContext,
    WalGcScheduler, WalGcSchedulerConfig, WalGcSummary,
};

#[cfg(test)]
mod tests;
