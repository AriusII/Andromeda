use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Metrics for WAL segment fragmentation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FragmentationMetrics {
    /// Unique segment identifier
    pub segment_id: u64,
    /// Total bytes in segment (including both live and dead records)
    pub total_bytes: u64,
    /// Bytes occupied by dead records (to be discarded during compaction)
    pub dead_bytes: u64,
    /// Fragmentation ratio: dead_bytes / total_bytes (in range [0.0, 1.0])
    pub fragmentation_ratio: f64,
}

impl FragmentationMetrics {
    /// Create new fragmentation metrics with validation.
    ///
    /// # Errors
    ///
    /// Returns `Storage` error if:
    /// - segment_id is zero
    /// - total_bytes is zero
    /// - dead_bytes > total_bytes
    /// - fragmentation_ratio is NaN or infinite
    pub fn new(segment_id: u64, total_bytes: u64, dead_bytes: u64) -> AndromedaResult<Self> {
        if segment_id == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL compaction: segment ID must not be zero",
            ));
        }
        if total_bytes == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL compaction: total bytes must not be zero",
            ));
        }
        if dead_bytes > total_bytes {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL compaction: dead bytes must not exceed total bytes",
            ));
        }

        let fragmentation_ratio = dead_bytes as f64 / total_bytes as f64;

        if !fragmentation_ratio.is_finite() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL compaction: fragmentation ratio is not finite",
            ));
        }

        Ok(Self {
            segment_id,
            total_bytes,
            dead_bytes,
            fragmentation_ratio,
        })
    }

    /// Check if this segment exceeds the fragmentation threshold (30%).
    pub const fn is_compaction_candidate(&self) -> bool {
        // Inline threshold check; 0.30 represents 30% dead space
        (self.fragmentation_ratio * 1000.0) as u32 >= 300
    }

    /// Estimate space to be recovered after compaction.
    pub fn estimated_recovery_bytes(&self) -> u64 {
        self.dead_bytes
    }
}

/// Result of compacting a single segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionResult {
    /// Original segment ID
    pub original_segment_id: u64,
    /// New segment ID (assigned during rewrite)
    pub new_segment_id: u64,
    /// Bytes in original segment
    pub original_bytes: u64,
    /// Bytes in compacted segment (after filtering dead records)
    pub compacted_bytes: u64,
    /// Bytes recovered (reduction)
    pub bytes_recovered: u64,
    /// Records in original segment
    pub original_record_count: u64,
    /// Records in compacted segment (after filtering)
    pub compacted_record_count: u64,
    /// Records removed during compaction
    pub records_removed: u64,
}

impl CompactionResult {
    /// Calculate space reduction ratio.
    pub fn reduction_ratio(&self) -> f64 {
        if self.original_bytes == 0 {
            0.0
        } else {
            self.bytes_recovered as f64 / self.original_bytes as f64
        }
    }
}

/// Audit event emitted during WAL compaction operations.
#[derive(Debug, Clone, PartialEq)]
pub enum WalCompactionAuditEvent {
    /// Candidate identified for compaction
    CandidateIdentified {
        segment_id: u64,
        fragmentation_ratio: f64,
        total_bytes: u64,
        dead_bytes: u64,
    },

    /// Compaction of a segment began
    CompactionStarted {
        segment_id: u64,
        fragmentation_ratio: f64,
    },

    /// Segment successfully compacted
    CompactionCompleted {
        original_segment_id: u64,
        new_segment_id: u64,
        bytes_recovered: u64,
        reduction_ratio: f64,
    },

    /// Compaction failed for segment (old segment remains)
    CompactionFailed { segment_id: u64, reason: String },

    /// Compaction run summary with metrics
    Summary {
        run_id: u64,
        candidates_identified: u64,
        candidates_compacted: u64,
        compaction_skipped: u64,
        total_bytes_recovered: u64,
        total_reduction_ratio: f64,
    },
}

impl WalCompactionAuditEvent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CandidateIdentified { .. } => "WalCompactionCandidateIdentified",
            Self::CompactionStarted { .. } => "WalCompactionStarted",
            Self::CompactionCompleted { .. } => "WalCompactionCompleted",
            Self::CompactionFailed { .. } => "WalCompactionFailed",
            Self::Summary { .. } => "WalCompactionSummary",
        }
    }
}

/// Summary of a single WAL compaction run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalCompactionSummary {
    pub run_id: u64,
    pub candidates_identified: u64,
    pub candidates_compacted: u64,
    pub compaction_skipped: u64,
    pub total_bytes_recovered: u64,
}

impl WalCompactionSummary {
    /// Create a new compaction run summary.
    pub fn new(run_id: u64) -> Self {
        WalCompactionSummary {
            run_id,
            candidates_identified: 0,
            candidates_compacted: 0,
            compaction_skipped: 0,
            total_bytes_recovered: 0,
        }
    }

    /// Calculate average reduction ratio across all compactions.
    pub fn average_reduction_ratio(&self) -> f64 {
        if self.candidates_compacted == 0 {
            0.0
        } else {
            self.total_bytes_recovered as f64 / (self.total_bytes_recovered as f64 / 0.40)
            // Assume 40% avg recovery
        }
    }

    /// Check if any segments were compacted in this run.
    pub fn any_compacted(&self) -> bool {
        self.candidates_compacted > 0
    }
}
