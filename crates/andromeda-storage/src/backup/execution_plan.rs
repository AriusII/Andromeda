//! F5 Physical Backup Execution Plan — durable artifact orchestration.
//!
//! This module defines the execution model for copying a cold snapshot + WAL archive
//! to backup storage. All data sources are durable artifacts only (no RAM shortcuts).
//!
//! # Execution Model
//!
//! 1. **Validate manifest** — snapshot identity, WAL range, CRCs
//! 2. **Build extent copy plan** — cold snapshot extents (ColdStore + HotStore)
//! 3. **Build WAL segment copy plan** — sequential WAL segments forming [wal_start_lsn, wal_end_lsn]
//! 4. **Validate parallelization** — disjoint extent ranges can copy in parallel; WAL is sequential
//! 5. **Validate resource limits** — total bytes and segment count within guardrails
//! 6. **Check WAL continuity** — no gaps, no missing segments, checksums valid
//! 7. **Execute copy** — (deferred to V1 physical layer; plan only)
//! 8. **Validate backup copy** — optional checksum validation (resource budget dependent)
//! 9. **Write backup metadata** — snapshot_id, wal_start/end_lsn, copy_timestamp, metadata checksum
//!
//! # Failure Modes
//!
//! - **Incomplete copy**: Metadata checksum is absent or fails → backup is invalid
//! - **WAL gaps**: Missing segment in range → backup is invalid, restore will fail
//! - **Snapshot modification during copy**: Manifest checksum detects this
//! - **Destination storage full**: Backup fails cleanly, source unchanged
//! - **Corrupted WAL segment**: Artifact digest checksum fails validation
//! - **Resource exhaustion**: Copy plan rejects if resource limits exceeded

use andromeda_core::AndromedaResult;

use crate::{ExtentDescriptor, ExtentState, Lsn, StorageTier, WalSegmentDescriptor};

use super::helpers::backup_error;
use super::plan::BackupManifest;

/// Single extent copy task for the physical backup.
///
/// Extent identity + storage tier determines the copy source.
/// ColdStore extents are preferred; HotStore extents may be present during backup
/// if not yet migrated to cold storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtentCopyTask {
    /// Canonical extent descriptor (contains page count, LSN bounds, segment reference)
    pub extent_descriptor: ExtentDescriptor,
    /// Source storage tier (HotStore NVMe or ColdStore HDD)
    pub source_tier: StorageTier,
    /// Byte count for this extent (page_count * page_size, approximately)
    pub byte_count: u64,
}

impl ExtentCopyTask {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.extent_descriptor.validate()?;

        // Backup must copy from durable storage only
        if matches!(self.extent_descriptor.state, ExtentState::AllocatingHot) {
            return Err(backup_error(
                "extent copy task must not reference hot/mutable extent",
            ));
        }

        if self.byte_count == 0 {
            return Err(backup_error("extent copy task byte count must not be zero"));
        }

        // ColdStore extents are immutable; HotStore may have sealed extents that are durable
        if !matches!(
            self.source_tier,
            StorageTier::ColdStore | StorageTier::HotStore
        ) {
            return Err(backup_error(
                "extent copy task source tier must be HotStore or ColdStore, not RAM",
            ));
        }

        Ok(())
    }
}

/// Single WAL segment copy task in the physical backup.
///
/// WAL segments must form a contiguous chain from `wal_start_lsn` to `wal_end_lsn`.
/// Each segment's LSN bounds and `base_previous_lsn` are validated to ensure no gaps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalSegmentCopyTask {
    /// Canonical WAL segment descriptor (first_lsn, last_lsn, base_previous_lsn, record_count)
    pub segment_descriptor: WalSegmentDescriptor,
    /// Byte count for this segment's durable records
    pub byte_count: u64,
    /// Position in the sequential WAL archive (0 = first segment)
    pub sequence_index: usize,
}

impl WalSegmentCopyTask {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.segment_descriptor.validate()?;

        if self.byte_count == 0 {
            return Err(backup_error(
                "WAL segment copy task byte count must not be zero",
            ));
        }

        Ok(())
    }
}

/// Resource guardrails for a physical backup execution.
///
/// Observed bytes are computed from durable artifact sizes (never from RAM page cache).
/// Backup fails if any limit is exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupResourceLimits {
    /// Maximum bytes for cold snapshot (all extents combined)
    pub max_total_extent_bytes: u64,
    /// Maximum bytes for WAL archive (all segments combined)
    pub max_total_wal_bytes: u64,
    /// Maximum parallel extent copy tasks (design: no implementation constraint here)
    pub max_parallel_extent_tasks: usize,
    /// Maximum WAL segments in archive
    pub max_wal_segment_count: usize,
}

impl BackupResourceLimits {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.max_total_extent_bytes == 0
            || self.max_total_wal_bytes == 0
            || self.max_parallel_extent_tasks == 0
            || self.max_wal_segment_count == 0
        {
            return Err(backup_error("backup resource limits must not be zero"));
        }

        Ok(())
    }

    /// Check if an extent plan fits within resource limits.
    pub fn can_accommodate_extents(&self, total_bytes: u64, count: usize) -> AndromedaResult<()> {
        if total_bytes > self.max_total_extent_bytes {
            return Err(backup_error(
                "extent copy plan exceeds max snapshot bytes limit",
            ));
        }

        if count > self.max_parallel_extent_tasks {
            return Err(backup_error(
                "extent copy plan count exceeds max parallel tasks limit",
            ));
        }

        Ok(())
    }

    /// Check if a WAL segment plan fits within resource limits.
    pub fn can_accommodate_wal(
        &self,
        total_bytes: u64,
        segment_count: usize,
    ) -> AndromedaResult<()> {
        if total_bytes > self.max_total_wal_bytes {
            return Err(backup_error(
                "WAL segment copy plan exceeds max WAL bytes limit",
            ));
        }

        if segment_count > self.max_wal_segment_count {
            return Err(backup_error(
                "WAL segment copy plan exceeds max segment count limit",
            ));
        }

        Ok(())
    }
}

/// Failure mode enumeration for physical backup execution.
///
/// These represent the categorical reasons a backup execution plan may be invalid
/// or a backup copy may fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupExecutionFailureMode {
    /// Manifest validation failed (CRC, identity, WAL range)
    ManifestInvalid,
    /// Extent descriptor is missing or mutable (not sealed/published)
    ExtentNotDurable,
    /// Extent copy plan exceeds resource limits
    ExtentResourceExhausted,
    /// WAL segment list is empty or has gaps (LSN discontinuity)
    WalSegmentChainInvalid,
    /// WAL segment missing from archive range
    WalSegmentMissing,
    /// WAL segment CRC/checksum failed validation
    WalSegmentCorrupted,
    /// WAL copy plan exceeds resource limits
    WalResourceExhausted,
    /// Destination storage full or write failed
    DestinationStorageFailure,
    /// Backup copy incomplete (partial write)
    IncompleteBackupCopy,
    /// Metadata checksum absent or invalid
    BackupMetadataInvalid,
}

impl BackupExecutionFailureMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ManifestInvalid => "backup manifest failed validation",
            Self::ExtentNotDurable => "extent copy references non-durable extent",
            Self::ExtentResourceExhausted => "extent copy plan exceeds resource limits",
            Self::WalSegmentChainInvalid => "WAL segment chain has gaps or missing segments",
            Self::WalSegmentMissing => "WAL segment missing from archive range",
            Self::WalSegmentCorrupted => "WAL segment artifact checksum validation failed",
            Self::WalResourceExhausted => "WAL copy plan exceeds resource limits",
            Self::DestinationStorageFailure => "destination storage full or write failed",
            Self::IncompleteBackupCopy => "backup copy incomplete (partial write)",
            Self::BackupMetadataInvalid => "backup metadata checksum absent or invalid",
        }
    }
}

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
        // Step 1: Validate manifest
        self.manifest.validate()?;

        // Step 2: Validate resource limits
        self.resource_limits.validate()?;

        // Step 3: Validate extent copy plan
        if self.extent_copy_plan.is_empty() {
            return Err(backup_error(
                "backup execution plan must include at least one extent",
            ));
        }

        let mut computed_extent_bytes = 0_u64;
        for (index, task) in self.extent_copy_plan.iter().enumerate() {
            task.validate()?;

            computed_extent_bytes = computed_extent_bytes
                .checked_add(task.byte_count)
                .ok_or_else(|| backup_error("extent copy plan total bytes overflow"))?;

            // Ensure no duplicate extent ids
            if self.extent_copy_plan[..index]
                .iter()
                .any(|prev| prev.extent_descriptor.extent_id == task.extent_descriptor.extent_id)
            {
                return Err(backup_error(
                    "backup extent copy plan must not duplicate extent ids",
                ));
            }
        }

        if computed_extent_bytes != self.total_extent_bytes {
            return Err(backup_error(
                "backup execution plan extent bytes mismatch between plan and field",
            ));
        }

        self.resource_limits
            .can_accommodate_extents(computed_extent_bytes, self.extent_copy_plan.len())?;

        // Step 4: Validate WAL segment copy plan
        if self.wal_segment_copy_plan.is_empty() {
            return Err(backup_error(
                "backup execution plan must include at least one WAL segment",
            ));
        }

        self.validate_wal_segment_chain()?;

        let mut computed_wal_bytes = 0_u64;
        for task in &self.wal_segment_copy_plan {
            task.validate()?;

            computed_wal_bytes = computed_wal_bytes
                .checked_add(task.byte_count)
                .ok_or_else(|| backup_error("WAL segment copy plan total bytes overflow"))?;
        }

        if computed_wal_bytes != self.total_wal_bytes {
            return Err(backup_error(
                "backup execution plan WAL bytes mismatch between plan and field",
            ));
        }

        self.resource_limits
            .can_accommodate_wal(computed_wal_bytes, self.wal_segment_copy_plan.len())?;

        Ok(())
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

            // Ensure no duplicate segment ids
            if self.wal_segment_copy_plan[..index]
                .iter()
                .any(|prev| prev.segment_descriptor.segment_id == desc.segment_id)
            {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AllocationId, ObjectId, PageId, PageSize, WalArchiveRange};

    fn sample_extent(extent_id: u64, first_page: u64, page_count: u32) -> ExtentDescriptor {
        ExtentDescriptor {
            extent_id: crate::ExtentId::new(extent_id),
            object_id: ObjectId::new(1),
            allocation_id: AllocationId::new(1),
            first_page_id: PageId::new(first_page),
            page_count,
            page_size: PageSize::KiB16,
            state: ExtentState::PublishedCold,
            segment_id: Some(crate::SegmentId::new(extent_id)),
            file_offset: 0,
            allocated_on_disk: false,
        }
    }

    fn sample_wal_segment(
        segment_id: u64,
        first_lsn: u64,
        last_lsn: u64,
        base_previous_lsn: Option<u64>,
    ) -> WalSegmentDescriptor {
        WalSegmentDescriptor {
            format_version: 1,
            segment_id,
            first_lsn: Lsn::new(first_lsn),
            last_lsn: Lsn::new(last_lsn),
            base_previous_lsn: base_previous_lsn.map(Lsn::new),
            record_count: (last_lsn - first_lsn + 1) as usize,
        }
    }

    fn sample_manifest() -> BackupManifest {
        BackupManifest {
            backup_id: crate::BackupId::new(1),
            database_id: 1,
            created_epoch: 1000,
            snapshot: crate::ColdSnapshotBoundary {
                snapshot_id: 1,
                snapshot_descriptor_hash: [1; 32],
                base_checkpoint_lsn: Lsn::new(100),
                required_wal_start_lsn: Lsn::new(101),
            },
            wal_archive: WalArchiveRange::new(Lsn::new(101), Lsn::new(200)),
            manifest_crc: 999,
        }
    }

    fn sample_resource_limits() -> BackupResourceLimits {
        BackupResourceLimits {
            max_total_extent_bytes: 1_000_000,
            max_total_wal_bytes: 500_000,
            max_parallel_extent_tasks: 8,
            max_wal_segment_count: 100,
        }
    }

    #[test]
    fn backup_execution_plan_validates_manifest() {
        let mut manifest = sample_manifest();
        manifest.backup_id = crate::BackupId::new(0);

        let extent = sample_extent(1, 100, 16);
        let wal_segment = sample_wal_segment(1, 101, 200, None);

        let plan = BackupExecutionPlan {
            manifest,
            extent_copy_plan: vec![ExtentCopyTask {
                extent_descriptor: extent,
                source_tier: StorageTier::ColdStore,
                byte_count: 64 * 1024,
            }],
            wal_segment_copy_plan: vec![WalSegmentCopyTask {
                segment_descriptor: wal_segment,
                byte_count: 100 * 1024,
                sequence_index: 0,
            }],
            validate_checksum_on_copy: false,
            resource_limits: sample_resource_limits(),
            total_extent_bytes: 64 * 1024,
            total_wal_bytes: 100 * 1024,
        };

        let result = plan.validate();
        assert!(result.is_err(), "plan should reject zero backup_id");
    }

    #[test]
    fn backup_execution_plan_validates_extent_continuity() {
        let manifest = sample_manifest();
        let extent = sample_extent(1, 100, 16);

        // Duplicate extent IDs should fail
        let plan = BackupExecutionPlan {
            manifest,
            extent_copy_plan: vec![
                ExtentCopyTask {
                    extent_descriptor: extent,
                    source_tier: StorageTier::ColdStore,
                    byte_count: 64 * 1024,
                },
                ExtentCopyTask {
                    extent_descriptor: extent,
                    source_tier: StorageTier::ColdStore,
                    byte_count: 64 * 1024,
                },
            ],
            wal_segment_copy_plan: vec![WalSegmentCopyTask {
                segment_descriptor: sample_wal_segment(1, 101, 200, None),
                byte_count: 100 * 1024,
                sequence_index: 0,
            }],
            validate_checksum_on_copy: false,
            resource_limits: sample_resource_limits(),
            total_extent_bytes: 128 * 1024,
            total_wal_bytes: 100 * 1024,
        };

        let result = plan.validate();
        assert!(result.is_err(), "plan should reject duplicate extent ids");
    }

    #[test]
    fn backup_execution_plan_validates_wal_chain() {
        let manifest = sample_manifest();
        let extent = sample_extent(1, 100, 16);

        // WAL segments with gap should fail
        let plan = BackupExecutionPlan {
            manifest,
            extent_copy_plan: vec![ExtentCopyTask {
                extent_descriptor: extent,
                source_tier: StorageTier::ColdStore,
                byte_count: 64 * 1024,
            }],
            wal_segment_copy_plan: vec![
                WalSegmentCopyTask {
                    segment_descriptor: sample_wal_segment(1, 101, 150, None),
                    byte_count: 50 * 1024,
                    sequence_index: 0,
                },
                WalSegmentCopyTask {
                    segment_descriptor: sample_wal_segment(2, 152, 200, Some(150)),
                    byte_count: 50 * 1024,
                    sequence_index: 1,
                },
            ],
            validate_checksum_on_copy: false,
            resource_limits: sample_resource_limits(),
            total_extent_bytes: 64 * 1024,
            total_wal_bytes: 100 * 1024,
        };

        let result = plan.validate();
        assert!(
            result.is_err(),
            "plan should reject WAL segment chain with gap (expected LSN 151, got 152)"
        );
    }

    #[test]
    fn backup_execution_plan_validates_resource_limits() {
        let manifest = sample_manifest();
        let extent = sample_extent(1, 100, 16);

        let limits = BackupResourceLimits {
            max_total_extent_bytes: 50 * 1024, // Too small!
            max_total_wal_bytes: 500_000,
            max_parallel_extent_tasks: 8,
            max_wal_segment_count: 100,
        };

        let plan = BackupExecutionPlan {
            manifest,
            extent_copy_plan: vec![ExtentCopyTask {
                extent_descriptor: extent,
                source_tier: StorageTier::ColdStore,
                byte_count: 64 * 1024,
            }],
            wal_segment_copy_plan: vec![WalSegmentCopyTask {
                segment_descriptor: sample_wal_segment(1, 101, 200, None),
                byte_count: 100 * 1024,
                sequence_index: 0,
            }],
            validate_checksum_on_copy: false,
            resource_limits: limits,
            total_extent_bytes: 64 * 1024,
            total_wal_bytes: 100 * 1024,
        };

        let result = plan.validate();
        assert!(
            result.is_err(),
            "plan should reject extent bytes exceeding resource limit"
        );
    }
}
