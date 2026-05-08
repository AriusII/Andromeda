use super::{
    error::{BackupResult, backup_error},
    primitives::{BackupLsn, BackupTraceId},
    types::BackupId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackupPhase {
    CatalogSnapshot,
    HotStoreScan,
    ColdStoreScan,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalPageScan {
    pub segment_id: u64,
    pub page_start: u64,
    pub page_count: u64,
    pub priority: u8,
}

impl PhysicalPageScan {
    pub fn validate(&self) -> BackupResult<()> {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentPlan {
    HotStoreScan {
        extent_range: std::ops::Range<u64>,
    },
    ColdStoreScan {
        cold_segment_id: u64,
        page_count: u64,
    },
}

impl SegmentPlan {
    pub fn validate(&self) -> BackupResult<()> {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupPhysicalPlan<L = super::Lsn, T = super::TraceId> {
    pub backup_id: BackupId,
    pub catalog_snapshot_lsn: L,
    pub segments_to_scan: Vec<SegmentPlan>,
    pub total_pages: u64,
    pub trace_id: T,
    pub created_epoch: u64,
}

impl<L, T> BackupPhysicalPlan<L, T> {
    pub fn new(
        backup_id: BackupId,
        catalog_snapshot_lsn: L,
        segments_to_scan: Vec<SegmentPlan>,
        total_pages: u64,
        trace_id: T,
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
}

impl<L: BackupLsn, T: BackupTraceId> BackupPhysicalPlan<L, T> {
    pub fn validate(&self) -> BackupResult<()> {
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

        for (idx, segment) in self.segments_to_scan.iter().enumerate() {
            segment.validate().map_err(|error| {
                backup_error(format!(
                    "backup physical plan segment {idx} validation failed: {error}"
                ))
            })?;
        }

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
            .filter(|segment| matches!(segment, SegmentPlan::HotStoreScan { .. }))
            .count() as u64
    }

    pub fn cold_store_page_count(&self) -> u64 {
        self.segments_to_scan
            .iter()
            .filter_map(|segment| {
                if let SegmentPlan::ColdStoreScan { page_count, .. } = segment {
                    Some(*page_count)
                } else {
                    None
                }
            })
            .sum()
    }
}
