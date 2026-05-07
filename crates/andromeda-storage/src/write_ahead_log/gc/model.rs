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
