//! V0 backup metadata and Point-In-Time Recovery target validation.
//!
//! Modules:
//! - `physical_plan`: BackupPhysicalPlan — deterministic, pure planning
//! - `scheduler`: BackupIOScheduler — I/O-budgeted task scheduling
//! - `checkpoint_manager`: BackupCheckpointManager — crash-safe resumption
//! - `wal_archive_integration`: WAL archive integration with manifest finalization

mod artifacts;
mod checkpoint_manager;
mod execution_plan;
mod helpers;
mod physical_plan;
mod plan;
mod scheduler;
mod types;
mod wal_archive_integration;

// Re-export from parent crate for convenience
pub use crate::Lsn;

pub use artifacts::*;
pub use checkpoint_manager::*;
pub use execution_plan::*;
pub use physical_plan::*;
pub use scheduler::*;
pub use types::*;
pub use wal_archive_integration::*;

// Selective exports from plan to avoid ambiguity with physical_plan::BackupPhysicalPlan
pub use plan::{BackupManifest, PitrTarget};
