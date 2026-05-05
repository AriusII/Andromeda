//! BackupPhysicalPlan — deterministic, crash-safe backup orchestration.
//!
//! This module defines the physical backup plan that orchestrates page-by-page
//! backup from HotStore (NVMe) and ColdStore (HDD) with deterministic scheduling,
//! I/O budgeting, and crash-safe checkpointing.
//!
//! # Design Philosophy
//!
//! The physical plan is:
//! - **Pure** (no side effects during planning)
//! - **Deterministic** (same segment layout → same plan)
//! - **LSN-bound** (plan locked to catalog_snapshot_lsn, cannot replay with different LSN)
//! - **Crash-safe** (checkpoints enable resumption without re-scanning)
//! - **I/O-budgeted** (respects storage layer throughput ceilings)

use andromeda_core::AndromedaResult;
use andromeda_observe::TraceId;

use crate::Lsn;

use super::helpers::backup_error;
use super::types::BackupId;

/// Backup phase enumeration for checkpoint resumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackupPhase {
    /// Initial catalog snapshot capture
    CatalogSnapshot,
    /// Scanning HotStore (NVMe) extents
    HotStoreScan,
    /// Scanning ColdStore (HDD) extents
    ColdStoreScan,
    /// Finalizing WAL archive
    WalArchiveFinalize,
}

impl BackupPhase {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CatalogSnapshot => "catalog_snapshot",
            Self::HotStoreScan => "hot_store_scan",
            Self::ColdStoreScan => "cold_store_scan",
            Self::WalArchiveFinalize => "wal_archive_finalize",
        }
    }
}

/// Single page scan task within a segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalPageScan {
    /// Segment ID containing these pages
    pub segment_id: u64,
    /// First page offset within segment
    pub page_start: u64,
    /// Number of consecutive pages
    pub page_count: u64,
    /// Priority: 0 = HotStore (highest), 1+ = ColdStore (lower)
    pub priority: u8,
}

impl PhysicalPageScan {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.segment_id == 0 {
            return Err(backup_error(
                "physical page scan segment id must not be zero",
            ));
        }
        if self.page_count == 0 {
            return Err(backup_error(
                "physical page scan page count must not be zero",
            ));
        }
        Ok(())
    }

    pub fn estimated_io_bytes(&self, page_size_bytes: u64) -> u64 {
        self.page_count.saturating_mul(page_size_bytes)
    }
}

/// Segment plan variant for either HotStore or ColdStore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentPlan {
    /// HotStore extent range
    HotStoreScan { extent_range: std::ops::Range<u64> },
    /// ColdStore segment with page count
    ColdStoreScan {
        cold_segment_id: u64,
        page_count: u64,
    },
}

impl SegmentPlan {
    pub fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::HotStoreScan { extent_range } => {
                if extent_range.start >= extent_range.end {
                    return Err(backup_error(
                        "hot store scan extent range must have start < end",
                    ));
                }
            }
            Self::ColdStoreScan {
                cold_segment_id,
                page_count,
            } => {
                if *cold_segment_id == 0 {
                    return Err(backup_error("cold store scan segment id must not be zero"));
                }
                if *page_count == 0 {
                    return Err(backup_error("cold store scan page count must not be zero"));
                }
            }
        }
        Ok(())
    }
}

/// BackupPhysicalPlan orchestrates page-by-page backup execution.
///
/// # Invariants
///
/// 1. **No side effects**: Pure planning; no I/O during plan creation
/// 2. **Deterministic**: Same segment layout produces identical plan
/// 3. **LSN-bound**: Plan locked to `catalog_snapshot_lsn`; cannot replay with different LSN
/// 4. **Ordered phases**: HotStore → ColdStore → WAL archive
/// 5. **Resource-aware**: I/O budgeting respects storage tier throughput ceilings
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupPhysicalPlan {
    /// Stable backup identifier
    pub backup_id: BackupId,
    /// LSN of the catalog snapshot
    pub catalog_snapshot_lsn: Lsn,
    /// Ordered list of segment plans (HotStore first, then ColdStore)
    pub segments_to_scan: Vec<SegmentPlan>,
    /// Total pages across all segments
    pub total_pages: u64,
    /// Trace ID for audit/observability
    pub trace_id: TraceId,
    /// Epoch when plan was created
    pub created_epoch: u64,
}

impl BackupPhysicalPlan {
    pub fn new(
        backup_id: BackupId,
        catalog_snapshot_lsn: Lsn,
        segments_to_scan: Vec<SegmentPlan>,
        total_pages: u64,
        trace_id: TraceId,
        created_epoch: u64,
    ) -> Self {
        Self {
            backup_id,
            catalog_snapshot_lsn,
            segments_to_scan,
            total_pages,
            trace_id,
            created_epoch,
        }
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.backup_id.is_zero() {
            return Err(backup_error(
                "backup physical plan backup id must not be zero",
            ));
        }
        if self.catalog_snapshot_lsn.is_zero() {
            return Err(backup_error(
                "backup physical plan catalog snapshot LSN must not be zero",
            ));
        }
        if self.segments_to_scan.is_empty() {
            return Err(backup_error(
                "backup physical plan must include at least one segment",
            ));
        }
        if self.total_pages == 0 {
            return Err(backup_error(
                "backup physical plan total pages must not be zero",
            ));
        }
        if self.trace_id.is_zero() {
            return Err(backup_error(
                "backup physical plan trace id must not be zero",
            ));
        }
        if self.created_epoch == 0 {
            return Err(backup_error(
                "backup physical plan created epoch must not be zero",
            ));
        }

        // Validate each segment plan
        for (idx, segment) in self.segments_to_scan.iter().enumerate() {
            segment.validate().map_err(|e| {
                backup_error(format!(
                    "backup physical plan segment {idx} validation failed: {e}"
                ))
            })?;
        }

        // Verify phases are ordered: HotStore before ColdStore
        let mut seen_cold = false;
        for segment in &self.segments_to_scan {
            match segment {
                SegmentPlan::HotStoreScan { .. } => {
                    if seen_cold {
                        return Err(backup_error(
                            "backup physical plan must schedule HotStore before ColdStore",
                        ));
                    }
                }
                SegmentPlan::ColdStoreScan { .. } => {
                    seen_cold = true;
                }
            }
        }

        Ok(())
    }

    pub fn hot_store_page_count(&self) -> u64 {
        self.segments_to_scan
            .iter()
            .filter_map(|seg| {
                if matches!(seg, SegmentPlan::HotStoreScan { .. }) {
                    // For HotStore, we'd need to know pages per extent
                    // For now, estimate based on segment count
                    Some(1)
                } else {
                    None
                }
            })
            .count() as u64
    }

    pub fn cold_store_page_count(&self) -> u64 {
        self.segments_to_scan
            .iter()
            .filter_map(|seg| {
                if let SegmentPlan::ColdStoreScan { page_count, .. } = seg {
                    Some(*page_count)
                } else {
                    None
                }
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_physical_plan_validates_required_fields() {
        let zero_backup = BackupPhysicalPlan::new(
            BackupId::new(0),
            Lsn::new(1),
            vec![SegmentPlan::ColdStoreScan {
                cold_segment_id: 1,
                page_count: 100,
            }],
            100,
            TraceId::new(1),
            1,
        );
        assert!(zero_backup.validate().is_err());

        let zero_lsn = BackupPhysicalPlan::new(
            BackupId::new(1),
            Lsn::new(0),
            vec![SegmentPlan::ColdStoreScan {
                cold_segment_id: 1,
                page_count: 100,
            }],
            100,
            TraceId::new(1),
            1,
        );
        assert!(zero_lsn.validate().is_err());
    }

    #[test]
    fn test_backup_physical_plan_ordering_enforcement() {
        let bad_order = BackupPhysicalPlan::new(
            BackupId::new(1),
            Lsn::new(1),
            vec![
                SegmentPlan::ColdStoreScan {
                    cold_segment_id: 1,
                    page_count: 100,
                },
                SegmentPlan::HotStoreScan {
                    extent_range: 0..10,
                },
            ],
            100,
            TraceId::new(1),
            1,
        );
        assert!(bad_order.validate().is_err());

        let good_order = BackupPhysicalPlan::new(
            BackupId::new(1),
            Lsn::new(1),
            vec![
                SegmentPlan::HotStoreScan {
                    extent_range: 0..10,
                },
                SegmentPlan::ColdStoreScan {
                    cold_segment_id: 1,
                    page_count: 100,
                },
            ],
            100,
            TraceId::new(1),
            1,
        );
        assert!(good_order.validate().is_ok());
    }
}
