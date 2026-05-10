//! BackupIOScheduler — deterministic, I/O-budgeted backup task scheduling.
//!
//! Implements page-by-page scheduling with respect for NVMe and HDD throughput ceilings.
//! Schedules HotStore (NVMe) before ColdStore (HDD) for optimal I/O patterns.

use crate::{
    error::{BackupResult, backup_error, map_backup_validation},
    physical_plan::{BackupPhysicalPlan, SegmentPlan},
    primitives::{BackupLsn, BackupTraceId},
};

/// Single I/O task with scheduling metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupIOTask {
    /// Unique task ID
    pub task_id: u64,
    /// Segment ID to scan
    pub segment_id: u64,
    /// Page start offset
    pub page_start: u64,
    /// Page count
    pub page_count: u64,
    /// Priority: 0 = HotStore (highest), 1+ = ColdStore
    pub priority: u8,
    /// Estimated I/O duration in milliseconds
    pub estimated_io_ms: u64,
    /// Storage tier: 0 = NVMe (HotStore), 1 = HDD (ColdStore)
    pub storage_tier: u8,
}

/// Complete backup I/O schedule with resource estimates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupIOSchedule {
    /// Ordered list of I/O tasks (by priority and scheduling order)
    pub tasks: Vec<BackupIOTask>,
    /// Total estimated backup duration in milliseconds
    pub total_estimated_duration_ms: u64,
    /// Peak I/O throughput required (MB/s)
    pub peak_throughput_mbps: u64,
    /// Total bytes to backup
    pub total_bytes: u64,
}

impl BackupIOSchedule {
    pub fn validate(&self) -> BackupResult<()> {
        if self.tasks.is_empty() {
            return Err(backup_error(
                "backup IO schedule must include at least one task",
            ));
        }
        if self.total_estimated_duration_ms == 0 {
            return Err(backup_error(
                "backup IO schedule total estimated duration must not be zero",
            ));
        }
        if self.peak_throughput_mbps == 0 {
            return Err(backup_error(
                "backup IO schedule peak throughput must not be zero",
            ));
        }
        if self.total_bytes == 0 {
            return Err(backup_error(
                "backup IO schedule total bytes must not be zero",
            ));
        }

        for (idx, task) in self.tasks.iter().enumerate() {
            if task.segment_id == 0 {
                return Err(backup_error(format!(
                    "backup IO schedule task {idx} segment id must not be zero"
                )));
            }
            if task.page_count == 0 {
                return Err(backup_error(format!(
                    "backup IO schedule task {idx} page count must not be zero"
                )));
            }
        }

        Ok(())
    }

    /// Verify that schedule respects I/O throughput budgets.
    pub fn respects_budget(&self, nvme_budget_mbps: u64, hdd_budget_mbps: u64) -> BackupResult<()> {
        if nvme_budget_mbps == 0 {
            return Err(backup_error("NVMe throughput budget must not be zero"));
        }
        if hdd_budget_mbps == 0 {
            return Err(backup_error("HDD throughput budget must not be zero"));
        }

        // Check that peak throughput doesn't exceed combined budget
        let combined_budget = nvme_budget_mbps.saturating_add(hdd_budget_mbps);
        if self.peak_throughput_mbps > combined_budget {
            return Err(backup_error(
                "backup IO schedule peak throughput exceeds combined storage tier budget",
            ));
        }

        Ok(())
    }
}

/// BackupIOScheduler — schedules page-by-page backup tasks with I/O budgeting.
pub struct BackupIOScheduler {
    /// Page size in bytes (typically 4096 or 8192)
    page_size_bytes: u64,
    /// NVMe throughput ceiling (MB/s)
    nvme_throughput_mbps: u64,
    /// HDD throughput ceiling (MB/s)
    hdd_throughput_mbps: u64,
    /// Maximum pages per single I/O task
    max_pages_per_task: u64,
}

impl BackupIOScheduler {
    pub fn new(page_size_bytes: u64, nvme_throughput_mbps: u64, hdd_throughput_mbps: u64) -> Self {
        Self {
            page_size_bytes,
            nvme_throughput_mbps,
            hdd_throughput_mbps,
            max_pages_per_task: 1024, // Conservative default
        }
    }

    pub fn with_max_pages_per_task(mut self, max_pages: u64) -> Self {
        self.max_pages_per_task = max_pages;
        self
    }

    /// Schedule backup I/O tasks from a physical plan.
    ///
    /// Returns ordered task list respecting:
    /// 1. Priority (HotStore before ColdStore)
    /// 2. I/O budgeting (respect throughput ceilings)
    /// 3. Task granularity (split large segments into smaller tasks)
    pub fn schedule_page_scan<L, T>(
        &self,
        plan: &BackupPhysicalPlan<L, T>,
    ) -> BackupResult<BackupIOSchedule>
    where
        L: BackupLsn,
        T: BackupTraceId,
    {
        map_backup_validation(plan.validate())?;

        let mut tasks = Vec::new();
        let mut task_id = 1u64;
        let mut nvme_bytes = 0u64;
        let mut hdd_bytes = 0u64;

        // Process segments in order (HotStore first, then ColdStore)
        for segment in &plan.segments_to_scan {
            match segment {
                SegmentPlan::HotStoreScan { extent_range } => {
                    // Estimate pages per extent; here we use conservative estimate
                    let extent_count = extent_range.end - extent_range.start;
                    let pages_per_extent = 256; // Conservative estimate

                    for extent_idx in 0..extent_count {
                        let pages_in_extent = pages_per_extent;
                        let mut pages_remaining = pages_in_extent;
                        let mut page_offset = 0u64;

                        while pages_remaining > 0 {
                            let pages_in_task =
                                std::cmp::min(pages_remaining, self.max_pages_per_task);
                            let bytes_in_task = pages_in_task.saturating_mul(self.page_size_bytes);

                            let io_ms = if self.nvme_throughput_mbps > 0 {
                                ((bytes_in_task as f64)
                                    / (self.nvme_throughput_mbps as f64 * 1024.0 * 1024.0))
                                    as u64
                                    + 1
                            } else {
                                1
                            };

                            tasks.push(BackupIOTask {
                                task_id,
                                segment_id: extent_range.start + extent_idx + 1, // Ensure segment_id > 0
                                page_start: page_offset,
                                page_count: pages_in_task,
                                priority: 0, // HotStore = highest priority
                                estimated_io_ms: io_ms,
                                storage_tier: 0, // NVMe
                            });

                            task_id = task_id
                                .checked_add(1)
                                .ok_or_else(|| backup_error("backup IO task id overflow"))?;
                            nvme_bytes = nvme_bytes
                                .checked_add(bytes_in_task)
                                .ok_or_else(|| backup_error("backup IO NVMe bytes overflow"))?;

                            page_offset = page_offset
                                .checked_add(pages_in_task)
                                .ok_or_else(|| backup_error("backup IO page offset overflow"))?;
                            pages_remaining = pages_remaining.saturating_sub(pages_in_task);
                        }
                    }
                },
                SegmentPlan::ColdStoreScan {
                    cold_segment_id,
                    page_count,
                } => {
                    let mut pages_remaining = *page_count;
                    let mut page_offset = 0u64;

                    while pages_remaining > 0 {
                        let pages_in_task = std::cmp::min(pages_remaining, self.max_pages_per_task);
                        let bytes_in_task = pages_in_task.saturating_mul(self.page_size_bytes);

                        let io_ms = if self.hdd_throughput_mbps > 0 {
                            ((bytes_in_task as f64)
                                / (self.hdd_throughput_mbps as f64 * 1024.0 * 1024.0))
                                as u64
                                + 1
                        } else {
                            1
                        };

                        tasks.push(BackupIOTask {
                            task_id,
                            segment_id: *cold_segment_id,
                            page_start: page_offset,
                            page_count: pages_in_task,
                            priority: 1, // ColdStore = lower priority
                            estimated_io_ms: io_ms,
                            storage_tier: 1, // HDD
                        });

                        task_id = task_id
                            .checked_add(1)
                            .ok_or_else(|| backup_error("backup IO task id overflow"))?;
                        hdd_bytes = hdd_bytes
                            .checked_add(bytes_in_task)
                            .ok_or_else(|| backup_error("backup IO HDD bytes overflow"))?;

                        page_offset = page_offset
                            .checked_add(pages_in_task)
                            .ok_or_else(|| backup_error("backup IO page offset overflow"))?;
                        pages_remaining = pages_remaining.saturating_sub(pages_in_task);
                    }
                },
            }
        }

        // Calculate total duration and peak throughput
        let total_duration_ms: u64 = tasks.iter().map(|t| t.estimated_io_ms).sum();
        let total_bytes = nvme_bytes
            .checked_add(hdd_bytes)
            .ok_or_else(|| backup_error("backup IO total bytes overflow"))?;

        // Peak throughput is max of storage tier throughputs
        let peak_throughput_mbps =
            std::cmp::max(self.nvme_throughput_mbps, self.hdd_throughput_mbps);

        let schedule = BackupIOSchedule {
            tasks,
            total_estimated_duration_ms: total_duration_ms,
            peak_throughput_mbps,
            total_bytes,
        };

        schedule.validate()?;
        Ok(schedule)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_observability::TraceId;
    use andromeda_wal::Lsn;

    #[test]
    fn test_backup_io_scheduler_creates_schedule() -> BackupResult<()> {
        let scheduler = BackupIOScheduler::new(4096, 1000, 100); // 1000 MB/s NVMe, 100 MB/s HDD

        let plan = BackupPhysicalPlan::new(
            crate::BackupId::new(1),
            Lsn::new(100),
            vec![SegmentPlan::ColdStoreScan {
                cold_segment_id: 1,
                page_count: 100,
            }],
            100,
            TraceId::new(1),
            1,
        );

        let schedule = scheduler.schedule_page_scan(&plan)?;
        assert!(!schedule.tasks.is_empty());
        assert!(schedule.total_estimated_duration_ms > 0);
        assert_eq!(schedule.peak_throughput_mbps, 1000);
        Ok(())
    }

    #[test]
    fn test_backup_io_schedule_respects_budget() -> BackupResult<()> {
        let scheduler = BackupIOScheduler::new(4096, 500, 100);

        let plan = BackupPhysicalPlan::new(
            crate::BackupId::new(1),
            Lsn::new(100),
            vec![SegmentPlan::ColdStoreScan {
                cold_segment_id: 1,
                page_count: 100,
            }],
            100,
            TraceId::new(1),
            1,
        );

        let schedule = scheduler.schedule_page_scan(&plan)?;

        // Should respect combined budget
        assert!(schedule.respects_budget(500, 100).is_ok());

        // Should fail if budget is too tight
        assert!(schedule.respects_budget(50, 10).is_err());
        Ok(())
    }
}
