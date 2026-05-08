use crate::backup::helpers::map_backup_validation;
use andromeda_core::AndromedaResult;
use std::collections::BTreeSet;

use crate::Lsn;

use super::super::{helpers::backup_error, plan::BackupManifest};
use super::{
    limits::BackupResourceLimits,
    tasks::{ExtentCopyTask, WalSegmentCopyTask},
};

/// F5 Physical Backup Execution Plan — durable artifact orchestration.
///
/// # Contract
///
/// - **Input**: Cold snapshot manifest (extent/segment layout, page ranges, checksums)
/// - **Output**: Detailed copy plan for each extent and WAL segment
/// - **Validation**:
///   - Manifest checksum and version
///   - WAL tail LSN is committed (visible)
///   - All extents are published/cold (not mutable)
///   - WAL segments form contiguous chain without gaps
///   - Resource limits are satisfied
///
/// # Parallelization (Design Only)
///
/// - Multiple threads can copy disjoint extent ranges without contention
/// - WAL segment copy is sequential (segments must be contiguous)
/// - Metadata aggregation happens after all copies complete
///
/// # Failure Modes
///
/// - **Incomplete copy**: Partial backup is invalid; metadata checksum fails or is omitted
/// - **WAL gaps**: If a WAL segment is missing, backup is invalid and restore will fail
/// - **Snapshot modification during copy**: Manifest checksum protects against this
/// - **Destination storage full**: Backup must fail cleanly with clear error; do not corrupt source
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupExecutionPlan {
    /// The backup manifest (snapshot + WAL range boundaries)
    pub manifest: BackupManifest,

    /// Extent copy tasks for the cold snapshot (can execute in parallel)
    pub extent_copy_plan: Vec<ExtentCopyTask>,

    /// WAL segment copy tasks (must execute sequentially in order)
    pub wal_segment_copy_plan: Vec<WalSegmentCopyTask>,

    /// Whether to validate checksum of backup copy (optional, resource budget dependent)
    pub validate_checksum_on_copy: bool,

    /// Resource limits for this backup execution
    pub resource_limits: BackupResourceLimits,

    /// Computed total extent bytes (for validation and reporting)
    pub total_extent_bytes: u64,

    /// Computed total WAL bytes (for validation and reporting)
    pub total_wal_bytes: u64,
}

impl BackupExecutionPlan {
    /// Validate the entire backup execution plan.
    ///
    /// This is the primary gate-keeper: if this passes, the backup is ready to execute.
    /// It verifies:
    /// 1. Manifest is valid (CRC, version, LSN bounds)
    /// 2. Extent copy plan is non-empty and all extents are durable
    /// 3. WAL segment copy plan forms a contiguous chain
    /// 4. Resource limits are satisfied
    pub fn validate(&self) -> AndromedaResult<()> {
        map_backup_validation(self.manifest.validate())?;
        self.resource_limits.validate()?;

        let computed_extent_bytes = self.validate_extent_copy_plan()?;

        if computed_extent_bytes != self.total_extent_bytes {
            return Err(backup_error(
                "backup execution plan extent bytes mismatch between plan and field",
            ));
        }

        self.resource_limits
            .can_accommodate_extents(computed_extent_bytes, self.extent_copy_plan.len())?;

        let computed_wal_bytes = self.validate_wal_copy_plan()?;

        if computed_wal_bytes != self.total_wal_bytes {
            return Err(backup_error(
                "backup execution plan WAL bytes mismatch between plan and field",
            ));
        }

        self.resource_limits
            .can_accommodate_wal(computed_wal_bytes, self.wal_segment_copy_plan.len())?;

        Ok(())
    }

    fn validate_extent_copy_plan(&self) -> AndromedaResult<u64> {
        if self.extent_copy_plan.is_empty() {
            return Err(backup_error(
                "backup execution plan must include at least one extent",
            ));
        }

        let mut total_bytes = 0_u64;
        let mut extent_ids = BTreeSet::new();

        for task in &self.extent_copy_plan {
            task.validate()?;

            total_bytes = total_bytes
                .checked_add(task.byte_count)
                .ok_or_else(|| backup_error("extent copy plan total bytes overflow"))?;

            if !extent_ids.insert(task.extent_descriptor.extent_id) {
                return Err(backup_error(
                    "backup extent copy plan must not duplicate extent ids",
                ));
            }
        }

        Ok(total_bytes)
    }

    fn validate_wal_copy_plan(&self) -> AndromedaResult<u64> {
        if self.wal_segment_copy_plan.is_empty() {
            return Err(backup_error(
                "backup execution plan must include at least one WAL segment",
            ));
        }

        self.validate_wal_segment_chain()?;

        self.wal_segment_copy_plan
            .iter()
            .try_fold(0_u64, |total_bytes, task| {
                task.validate()?;
                total_bytes
                    .checked_add(task.byte_count)
                    .ok_or_else(|| backup_error("WAL segment copy plan total bytes overflow"))
            })
    }

    /// Validate WAL segment chain continuity.
    ///
    /// Ensures:
    /// - First segment starts at manifest WAL archive start LSN
    /// - Each segment is contiguous with the previous (LSN chaining)
    /// - Last segment ends at manifest WAL archive end LSN
    /// - No segment ID duplicates
    fn validate_wal_segment_chain(&self) -> AndromedaResult<()> {
        let archive = self.manifest.wal_archive;

        let first = self
            .wal_segment_copy_plan
            .first()
            .ok_or_else(|| backup_error("backup WAL segment copy plan must not be empty"))?;

        if first.segment_descriptor.first_lsn != archive.start {
            return Err(backup_error(
                "backup WAL first segment must start at archive start LSN",
            ));
        }

        let mut expected_first = archive.start;
        let mut previous_last = None;
        let mut segment_ids = BTreeSet::new();

        for (index, task) in self.wal_segment_copy_plan.iter().enumerate() {
            let desc = &task.segment_descriptor;

            desc.validate()?;

            if desc.first_lsn != expected_first {
                return Err(backup_error("backup WAL segment copy plan has LSN gap"));
            }

            if desc.base_previous_lsn != previous_last {
                return Err(backup_error(
                    "backup WAL segment base previous LSN does not chain to prior segment",
                ));
            }

            if !segment_ids.insert(desc.segment_id) {
                return Err(backup_error(
                    "backup WAL segment copy plan must not duplicate segment ids",
                ));
            }

            if task.sequence_index != index {
                return Err(backup_error(
                    "backup WAL segment copy task sequence index mismatch",
                ));
            }

            previous_last = Some(desc.last_lsn);
            expected_first = desc.last_lsn.try_next()?;
        }

        let last = self
            .wal_segment_copy_plan
            .last()
            .ok_or_else(|| backup_error("backup WAL segment copy plan must not be empty"))?;

        if last.segment_descriptor.last_lsn != archive.end_inclusive {
            return Err(backup_error(
                "backup WAL last segment must end at archive end LSN",
            ));
        }

        Ok(())
    }

    /// Get the total extent count.
    pub fn extent_count(&self) -> usize {
        self.extent_copy_plan.len()
    }

    /// Get the total WAL segment count.
    pub fn wal_segment_count(&self) -> usize {
        self.wal_segment_copy_plan.len()
    }

    /// Get the estimated total bytes (extent + WAL).
    pub fn total_bytes(&self) -> AndromedaResult<u64> {
        self.total_extent_bytes
            .checked_add(self.total_wal_bytes)
            .ok_or_else(|| backup_error("backup execution plan total bytes overflow"))
    }

    /// Check if a given LSN is covered by this backup's WAL archive.
    pub fn covers_wal_lsn(&self, lsn: Lsn) -> bool {
        self.manifest.wal_archive.contains(lsn)
    }

    /// Check if a given LSN is covered by this backup's snapshot.
    pub fn covers_snapshot_lsn(&self, lsn: Lsn) -> bool {
        lsn <= self.manifest.snapshot.base_checkpoint_lsn
    }
}
