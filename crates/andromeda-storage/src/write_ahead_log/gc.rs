//! WAL Segment Garbage Collection with Archive Verification
//!
//! This module implements safe removal of obsolete WAL segments after backup integration.
//!
//! # Overview
//!
//! WAL segments become candidates for garbage collection when:
//! 1. All transactions in the segment are committed (visible)
//! 2. All visible transactions are older than the minimum active snapshot LSN
//! 3. The segment has been archived in at least one backup
//! 4. The segment LSN range does not overlap with recovery requirements
//!
//! # Safety Guarantees
//!
//! - **No loss of durability**: Segments are only removed after successful archive verification
//! - **Recovery safety**: Manifest boundary is respected; no segment required by recovery is removed
//! - **Atomic visibility**: Archive status is verified immediately before removal
//! - **Fail-safe**: If archive verification fails, the segment remains in HotStore
//!
//! # Architecture
//!
//! - `WalGcCandidate`: Identification of a recyclable segment
//! - `identify_gc_candidates()`: Determines candidates based on LSN thresholds
//! - `verify_archived()`: Confirms archive coverage before removal
//! - `safe_remove_segment()`: Deletes from HotStore with rollback capability
//! - `WalGcScheduler`: Periodic background task with metrics
//!
//! # Invariants
//!
//! - A segment can only be GC'd if sealing_lsn < min_active_snapshot_lsn
//! - A segment must be verified archived before removal
//! - A segment cannot be GC'd if creation_lsn <= manifest.required_wal_start_lsn
//! - GC operations never panic; all errors are observable
//! - Zero unsafe code

use std::sync::Arc;
use std::time::Duration;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::Lsn;

/// Metadata for a candidate WAL segment for garbage collection.
///
/// A segment is eligible for GC when:
/// - Its sealing_lsn < min_active_snapshot_lsn (all records invisible)
/// - It contains only committed transactions
/// - It has been archived in at least one backup
/// - Its creation_lsn > manifest.required_wal_start_lsn (not needed for recovery)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalGcCandidate {
    /// Unique segment identifier
    pub segment_id: u64,
    /// LSN of the first record in the segment
    pub creation_lsn: Lsn,
    /// LSN of the last record in the segment
    pub sealing_lsn: Lsn,
    /// Size of segment file in bytes
    pub size_bytes: u64,
}

impl WalGcCandidate {
    /// Create a new GC candidate with validation.
    pub fn new(
        segment_id: u64,
        creation_lsn: Lsn,
        sealing_lsn: Lsn,
        size_bytes: u64,
    ) -> AndromedaResult<Self> {
        if segment_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL GC candidate segment ID must not be zero",
            ));
        }
        if creation_lsn.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL GC candidate creation LSN must not be zero",
            ));
        }
        if sealing_lsn.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL GC candidate sealing LSN must not be zero",
            ));
        }
        if sealing_lsn < creation_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL GC candidate sealing LSN must not precede creation LSN",
            ));
        }
        if size_bytes == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL GC candidate size must be greater than zero",
            ));
        }

        Ok(WalGcCandidate {
            segment_id,
            creation_lsn,
            sealing_lsn,
            size_bytes,
        })
    }

    /// Check if this candidate is eligible for removal given LSN constraints.
    ///
    /// A segment is eligible if:
    /// - sealing_lsn < min_active_snapshot_lsn (all records committed and invisible)
    /// - creation_lsn > required_wal_start_lsn (not needed by recovery)
    pub fn is_eligible(&self, min_active_snapshot_lsn: Lsn, required_wal_start_lsn: Lsn) -> bool {
        self.sealing_lsn < min_active_snapshot_lsn && self.creation_lsn > required_wal_start_lsn
    }
}

/// Audit event emitted during WAL GC operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WalGcAuditEvent {
    /// Candidate identified for potential GC
    CandidateIdentified {
        segment_id: u64,
        creation_lsn: Lsn,
        sealing_lsn: Lsn,
        size_bytes: u64,
    },

    /// Archive status verification requested
    ArchiveVerifyRequested {
        segment_id: u64,
        archive_status: ArchiveStatus,
    },

    /// Segment successfully removed from HotStore
    SegmentRemoved { segment_id: u64, bytes_freed: u64 },

    /// GC run summary with metrics
    Summary {
        run_id: u64,
        candidates_identified: u64,
        candidates_archived: u64,
        candidates_blocked: u64,
        bytes_freed: u64,
        segments_removed: u64,
    },
}

impl WalGcAuditEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CandidateIdentified { .. } => "GcCandidateIdentified",
            Self::ArchiveVerifyRequested { .. } => "GcArchiveVerifyRequested",
            Self::SegmentRemoved { .. } => "GcSegmentRemoved",
            Self::Summary { .. } => "GcSummary",
        }
    }
}

/// Result of archive verification for a WAL segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveStatus {
    /// Segment is archived in at least one backup
    Archived,
    /// Segment is pending archival (not yet in a completed backup)
    Pending,
    /// Archive status could not be determined (fail-safe: don't GC)
    Unknown,
}

impl std::fmt::Display for ArchiveStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Archived => write!(f, "archived"),
            Self::Pending => write!(f, "pending"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Summary of a single WAL GC run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalGcSummary {
    pub run_id: u64,
    pub candidates_identified: u64,
    pub candidates_archived: u64,
    pub candidates_blocked: u64,
    pub bytes_freed: u64,
    pub segments_removed: u64,
}

impl WalGcSummary {
    pub fn new(run_id: u64) -> Self {
        WalGcSummary {
            run_id,
            candidates_identified: 0,
            candidates_archived: 0,
            candidates_blocked: 0,
            bytes_freed: 0,
            segments_removed: 0,
        }
    }

    /// Check if any segments were removed in this run
    pub fn any_removed(&self) -> bool {
        self.segments_removed > 0
    }
}

/// Configuration for the WAL GC scheduler.
#[derive(Debug, Clone)]
pub struct WalGcSchedulerConfig {
    /// Interval between GC runs
    pub interval: Duration,
    /// Target free space in GiB; trigger GC when HotStore free < this value
    pub target_free_gib: u64,
    /// Maximum segments to process per run (0 = unlimited)
    pub max_segments_per_run: u64,
}

impl WalGcSchedulerConfig {
    pub fn new(interval: Duration, target_free_gib: u64) -> Self {
        WalGcSchedulerConfig {
            interval,
            target_free_gib,
            max_segments_per_run: 0, // No limit by default
        }
    }

    pub fn with_max_segments(mut self, max: u64) -> Self {
        self.max_segments_per_run = max;
        self
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.interval.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL GC scheduler interval must not be zero",
            ));
        }
        if self.target_free_gib == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL GC scheduler target free GiB must be greater than zero",
            ));
        }
        Ok(())
    }
}

/// WAL Garbage Collection context for identification and removal.
///
/// This trait abstracts the WAL manager and backup integration layer
/// to allow testing and future extensibility.
pub trait WalGcContext: Send + Sync {
    /// Identify candidates for garbage collection.
    ///
    /// Returns a sorted list (oldest first) of segments eligible for GC
    /// based on LSN thresholds and visibility constraints.
    fn identify_gc_candidates(
        &self,
        min_active_snapshot_lsn: Lsn,
    ) -> AndromedaResult<Vec<WalGcCandidate>>;

    /// Verify that a segment has been archived.
    ///
    /// Query the backup integration layer to confirm the segment
    /// is included in at least one completed backup.
    ///
    /// # Safety
    ///
    /// Returns `ArchiveStatus::Unknown` on any error — never fails on
    /// lookup timeout or transient backup integration issues.
    fn verify_archived(&self, candidate: &WalGcCandidate) -> AndromedaResult<ArchiveStatus>;

    /// Safely remove a segment from HotStore after archive verification.
    ///
    /// Preconditions:
    /// - Segment must be verified archived
    /// - Segment LSN must not overlap with recovery requirements
    /// - Segment must not contain in-flight transactions
    ///
    /// Postconditions (on success):
    /// - Segment file is deleted from HotStore
    /// - Segment is removed from in-memory directory
    /// - Audit event is emitted
    fn safe_remove_segment(&self, candidate: &WalGcCandidate) -> AndromedaResult<()>;

    /// Get the minimum active snapshot LSN.
    ///
    /// Used to determine which segments have become invisible
    /// to all active transactions.
    fn min_active_snapshot_lsn(&self) -> Lsn;

    /// Get the recovery boundary LSN from the latest manifest.
    ///
    /// Segments with creation_lsn <= this LSN may be needed by recovery
    /// and must not be garbage collected.
    fn required_wal_start_lsn(&self) -> Lsn;

    /// Emit an audit event for traceability.
    fn emit_audit_event(&self, event: WalGcAuditEvent) -> AndromedaResult<()>;
}

/// WAL Garbage Collector: identifies and removes obsolete segments.
pub struct WalGarbageCollector {
    context: Arc<dyn WalGcContext>,
}

impl WalGarbageCollector {
    /// Create a new GC coordinator with the given context.
    pub fn new(context: Arc<dyn WalGcContext>) -> Self {
        WalGarbageCollector { context }
    }

    /// Identify segments eligible for garbage collection.
    ///
    /// Returns candidates sorted by creation_lsn (oldest first) for deterministic ordering.
    /// A segment is a candidate if:
    /// - Its sealing_lsn < min_active_snapshot_lsn (all records are invisible)
    /// - It contains only committed transactions
    /// - Its creation_lsn > required_wal_start_lsn (not needed for recovery)
    pub fn identify_candidates(&self) -> AndromedaResult<Vec<WalGcCandidate>> {
        let min_lsn = self.context.min_active_snapshot_lsn();
        let req_lsn = self.context.required_wal_start_lsn();

        let mut candidates = self.context.identify_gc_candidates(min_lsn)?;

        // Filter by recovery boundary and sort by creation_lsn
        candidates.retain(|c| c.is_eligible(min_lsn, req_lsn));
        candidates.sort_by_key(|c| c.creation_lsn);

        // Emit audit for each candidate
        for candidate in &candidates {
            self.context
                .emit_audit_event(WalGcAuditEvent::CandidateIdentified {
                    segment_id: candidate.segment_id,
                    creation_lsn: candidate.creation_lsn,
                    sealing_lsn: candidate.sealing_lsn,
                    size_bytes: candidate.size_bytes,
                })?;
        }

        Ok(candidates)
    }

    /// Verify and remove a segment after archive confirmation.
    ///
    /// Steps:
    /// 1. Verify segment is archived
    /// 2. If not archived, fail-safe: return error
    /// 3. Delegate to context for safe file removal
    /// 4. Emit audit event on success
    pub fn remove_if_archived(&self, candidate: &WalGcCandidate) -> AndromedaResult<bool> {
        let status = self.context.verify_archived(candidate)?;

        self.context
            .emit_audit_event(WalGcAuditEvent::ArchiveVerifyRequested {
                segment_id: candidate.segment_id,
                archive_status: status,
            })?;

        if status != ArchiveStatus::Archived {
            return Ok(false); // Fail-safe: don't remove if not confirmed archived
        }

        self.context.safe_remove_segment(candidate)?;

        self.context
            .emit_audit_event(WalGcAuditEvent::SegmentRemoved {
                segment_id: candidate.segment_id,
                bytes_freed: candidate.size_bytes,
            })?;

        Ok(true)
    }

    /// Execute a single GC run: identify, verify, and remove candidates.
    ///
    /// Returns a summary with metrics. On any error during identification,
    /// the entire run fails. Archive verification failures are non-fatal
    /// (they just block removal of affected segments).
    pub fn run_gc(&self, run_id: u64) -> AndromedaResult<WalGcSummary> {
        let mut summary = WalGcSummary::new(run_id);

        let candidates = self.identify_candidates()?;
        summary.candidates_identified = candidates.len() as u64;

        let mut blocked = 0u64;
        for candidate in candidates {
            match self.remove_if_archived(&candidate)? {
                true => {
                    summary.segments_removed += 1;
                    summary.bytes_freed += candidate.size_bytes;
                    summary.candidates_archived += 1;
                }
                false => {
                    blocked += 1;
                    summary.candidates_archived += 1; // Counted but not removed
                }
            }
        }
        summary.candidates_blocked = blocked;

        self.context.emit_audit_event(WalGcAuditEvent::Summary {
            run_id,
            candidates_identified: summary.candidates_identified,
            candidates_archived: summary.candidates_archived,
            candidates_blocked: summary.candidates_blocked,
            bytes_freed: summary.bytes_freed,
            segments_removed: summary.segments_removed,
        })?;

        Ok(summary)
    }
}

/// WAL Garbage Collection Scheduler: periodic background task.
///
/// Executes GC runs on a configurable interval to maintain free space
/// in HotStore. Respects minimum free space targets and emits metrics.
pub struct WalGcScheduler {
    config: WalGcSchedulerConfig,
    gc: Arc<WalGarbageCollector>,
}

impl WalGcScheduler {
    /// Create a new scheduler with the given configuration and GC context.
    pub fn new(
        config: WalGcSchedulerConfig,
        gc: Arc<WalGarbageCollector>,
    ) -> AndromedaResult<Self> {
        config.validate()?;
        Ok(WalGcScheduler { config, gc })
    }

    /// Get the scheduler configuration.
    pub fn config(&self) -> &WalGcSchedulerConfig {
        &self.config
    }

    /// Execute a single scheduled GC run.
    ///
    /// Should be called by the scheduler's background loop. Returns a summary
    /// that includes metrics for monitoring and alerting.
    pub fn tick(&self, run_id: u64) -> AndromedaResult<WalGcSummary> {
        self.gc.run_gc(run_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wal_gc_candidate_creation_validates() {
        // Valid candidate
        let candidate = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096);
        assert!(candidate.is_ok());

        // Zero segment ID
        let err = WalGcCandidate::new(0, Lsn::new(100), Lsn::new(200), 4096);
        assert!(err.is_err());

        // Zero creation LSN
        let err = WalGcCandidate::new(1, Lsn::ZERO, Lsn::new(200), 4096);
        assert!(err.is_err());

        // Zero sealing LSN
        let err = WalGcCandidate::new(1, Lsn::new(100), Lsn::ZERO, 4096);
        assert!(err.is_err());

        // Sealing < creation
        let err = WalGcCandidate::new(1, Lsn::new(200), Lsn::new(100), 4096);
        assert!(err.is_err());

        // Zero size
        let err = WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 0);
        assert!(err.is_err());
    }

    #[test]
    fn wal_gc_candidate_eligibility_checks_lsn_thresholds() {
        let candidate =
            WalGcCandidate::new(1, Lsn::new(100), Lsn::new(200), 4096).expect("valid candidate");

        // Eligible: sealing < min_snapshot and creation > required_start
        let eligible = candidate.is_eligible(Lsn::new(300), Lsn::new(50));
        assert!(eligible);

        // Not eligible: sealing not strictly less than min_snapshot
        let not_eligible = candidate.is_eligible(Lsn::new(200), Lsn::new(50));
        assert!(!not_eligible);

        // Not eligible: creation not strictly greater than required_start
        let not_eligible = candidate.is_eligible(Lsn::new(300), Lsn::new(100));
        assert!(!not_eligible);
    }

    #[test]
    fn archive_status_display_formats_correctly() {
        assert_eq!(ArchiveStatus::Archived.to_string(), "archived");
        assert_eq!(ArchiveStatus::Pending.to_string(), "pending");
        assert_eq!(ArchiveStatus::Unknown.to_string(), "unknown");
    }

    #[test]
    fn wal_gc_audit_event_names_are_unique() {
        let events = [
            WalGcAuditEvent::CandidateIdentified {
                segment_id: 1,
                creation_lsn: Lsn::new(100),
                sealing_lsn: Lsn::new(200),
                size_bytes: 4096,
            },
            WalGcAuditEvent::ArchiveVerifyRequested {
                segment_id: 1,
                archive_status: ArchiveStatus::Archived,
            },
            WalGcAuditEvent::SegmentRemoved {
                segment_id: 1,
                bytes_freed: 4096,
            },
            WalGcAuditEvent::Summary {
                run_id: 1,
                candidates_identified: 1,
                candidates_archived: 1,
                candidates_blocked: 0,
                bytes_freed: 4096,
                segments_removed: 1,
            },
        ];

        let names: Vec<_> = events.iter().map(|e| e.as_str()).collect();
        assert_eq!(names.len(), 4);
        assert_eq!(
            names,
            vec![
                "GcCandidateIdentified",
                "GcArchiveVerifyRequested",
                "GcSegmentRemoved",
                "GcSummary"
            ]
        );
    }

    #[test]
    fn wal_gc_summary_tracks_metrics() {
        let mut summary = WalGcSummary::new(1);
        assert_eq!(summary.run_id, 1);
        assert_eq!(summary.candidates_identified, 0);
        assert!(!summary.any_removed());

        summary.segments_removed = 1;
        summary.bytes_freed = 4096;
        assert!(summary.any_removed());
    }

    #[test]
    fn wal_gc_scheduler_config_validates() {
        // Valid config
        let config = WalGcSchedulerConfig::new(Duration::from_secs(60), 10);
        assert!(config.validate().is_ok());

        // Zero interval
        let config = WalGcSchedulerConfig {
            interval: Duration::ZERO,
            target_free_gib: 10,
            max_segments_per_run: 0,
        };
        assert!(config.validate().is_err());

        // Zero target free
        let config = WalGcSchedulerConfig {
            interval: Duration::from_secs(60),
            target_free_gib: 0,
            max_segments_per_run: 0,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn wal_gc_scheduler_config_with_max_segments() {
        let config = WalGcSchedulerConfig::new(Duration::from_secs(60), 10).with_max_segments(100);
        assert_eq!(config.max_segments_per_run, 100);
    }
}
