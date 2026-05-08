//! Compatibility facade for physical backup planning DTOs.

use andromeda_observe::TraceId;

use crate::Lsn;

pub use super::backup_owner::{BackupPhase, PhysicalPageScan, SegmentPlan};

pub type BackupPhysicalPlan = super::backup_owner::BackupPhysicalPlan<Lsn, TraceId>;
