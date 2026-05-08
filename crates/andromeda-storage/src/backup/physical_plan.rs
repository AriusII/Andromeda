//! Compatibility facade for physical backup planning DTOs.

use crate::Lsn;
use andromeda_observe::TraceId;

pub use andromeda_backup::{BackupPhase, PhysicalPageScan, SegmentPlan};
pub type BackupPhysicalPlan = andromeda_backup::BackupPhysicalPlan<Lsn, TraceId>;
