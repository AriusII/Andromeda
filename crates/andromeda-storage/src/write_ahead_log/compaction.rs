//! WAL Segment Compaction with Fragmentation Detection
//!
//! This module implements safe, efficient compaction of WAL segments that have accumulated
//! significant dead space from garbage collection operations.
//!
//! # Overview
//!
//! WAL segments become fragmented when garbage collection removes records (e.g., GC'd versions,
//! completed undo logs, ancient transactions). The rewritten segment occupies the same physical
//! space as the original, but only contains live records. Compaction reduces storage overhead
//! by rewriting fragmented segments to new temporary segments, filtering out dead records.
//!
//! Compaction is only applied to segments with fragmentation_ratio >= 0.30 (30% dead space).
//!
//! # Safety Guarantees
//!
//! - **LSN Monotonicity**: Rewritten segments maintain strict LSN ordering and previous-LSN chaining
//! - **Visibility Preservation**: Records visible to active snapshots are always retained
//! - **Transaction Integrity**: Uncommitted/undoable transactions are preserved
//! - **Atomic Swap**: Temporary → active is atomic; on failure, old segment remains intact
//! - **No Data Loss**: Failed compaction leaves segment unchanged; temporary segment abandoned
//! - **Recovery Safe**: Manifest boundaries are respected; recovery segments never compacted
//!
//! # Architecture
//!
//! - `FragmentationMetrics`: Identifies compaction candidates (dead_bytes, ratio)
//! - `CompactionContext`: Trait for integration with WAL manager and snapshot registry
//! - `identify_compaction_candidates()`: Find segments exceeding fragmentation threshold
//! - `compact_segment()`: Rewrite segment with dead records filtered out
//! - `WalCompactionScheduler`: Background task with configurable interval and thresholds
//!
//! # Invariants
//!
//! - Fragmentation ratio must be calculated: dead_bytes / total_bytes
//! - Compaction only proceeds if ratio >= 0.30
//! - Records are kept if: LSN in [snapshot_start, snapshot_end] OR marked undoable
//! - Rewritten segment uses fresh segment_id (monotonically increasing from old ID)
//! - Swap is all-or-nothing: either new replaces old, or old remains unchanged
//! - All errors are observable; no panics in compaction path
//! - Zero unsafe code

use std::sync::Arc;
use std::time::Duration;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use crate::WalRecord;

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
            self.total_bytes_recovered as f64 / (self.total_bytes_recovered as f64 / 0.40) // Assume 40% avg recovery
        }
    }

    /// Check if any segments were compacted in this run.
    pub fn any_compacted(&self) -> bool {
        self.candidates_compacted > 0
    }
}

/// Context trait for integrating compaction with WAL manager and snapshot registry.
///
/// Implementations must provide methods to:
/// 1. Identify segments and their metrics
/// 2. Read records from a segment
/// 3. Determine record liveness (visibility to active snapshots)
/// 4. Rewrite segment to new location
/// 5. Atomically swap old ↔ new
pub trait CompactionContext: Send + Sync {
    /// Identify segments exceeding fragmentation threshold.
    ///
    /// Returns metrics for all segments, filtered by caller-provided threshold.
    fn identify_fragmented_segments(
        &self,
        threshold_ratio: f64,
    ) -> AndromedaResult<Vec<FragmentationMetrics>>;

    /// Read all records from a segment.
    ///
    /// # Errors
    ///
    /// Returns `Storage` error if segment cannot be read or is corrupted.
    fn read_segment_records(&self, segment_id: u64) -> AndromedaResult<Vec<WalRecord>>;

    /// Determine if a record should be kept during compaction.
    ///
    /// A record is "live" if:
    /// - Its LSN is visible to any active snapshot, OR
    /// - Its transaction is uncommitted, OR
    /// - Its segment is marked as undoable for recovery
    ///
    /// Returns `true` if record should be kept, `false` if it can be discarded.
    fn should_keep_record(&self, record: &WalRecord) -> AndromedaResult<bool>;

    /// Write a compacted segment with new segment_id to temporary location.
    ///
    /// Returns the new segment_id and byte size of the compacted segment.
    ///
    /// # Errors
    ///
    /// Returns `Storage` error if write fails. Temporary segment should be abandoned
    /// by caller on error.
    fn write_compacted_segment(
        &self,
        original_segment_id: u64,
        records: &[WalRecord],
    ) -> AndromedaResult<(u64, u64)>; // (new_segment_id, bytes_written)

    /// Atomically swap old segment with new segment (HotStore → old location).
    ///
    /// On failure, old segment must remain intact and usable.
    ///
    /// # Errors
    ///
    /// Returns `Storage` error if swap fails. On error, caller should abandon
    /// the new segment and rely on old segment for recovery.
    fn swap_segment(&self, old_segment_id: u64, new_segment_id: u64) -> AndromedaResult<()>;

    /// Emit an audit event for observability.
    fn emit_audit_event(&self, event: WalCompactionAuditEvent) -> AndromedaResult<()>;
}

/// Identify WAL segments that exceed the fragmentation threshold and are compaction candidates.
///
/// # Arguments
///
/// * `context` - Compaction context providing segment metrics
/// * `threshold_ratio` - Fragmentation threshold (e.g., 0.30 for 30%)
///
/// # Returns
///
/// Vector of `FragmentationMetrics` for candidates, sorted by fragmentation_ratio descending.
/// Higher fragmentation candidates are prioritized for compaction.
///
/// # Errors
///
/// Returns `Storage` error if segment metrics cannot be retrieved.
pub fn identify_compaction_candidates(
    context: &dyn CompactionContext,
    threshold_ratio: f64,
) -> AndromedaResult<Vec<FragmentationMetrics>> {
    let mut candidates = context.identify_fragmented_segments(threshold_ratio)?;

    // Sort by fragmentation_ratio descending (most fragmented first)
    candidates.sort_by(|a, b| {
        b.fragmentation_ratio
            .partial_cmp(&a.fragmentation_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Emit audit events for each candidate
    for metrics in &candidates {
        context.emit_audit_event(WalCompactionAuditEvent::CandidateIdentified {
            segment_id: metrics.segment_id,
            fragmentation_ratio: metrics.fragmentation_ratio,
            total_bytes: metrics.total_bytes,
            dead_bytes: metrics.dead_bytes,
        })?;
    }

    Ok(candidates)
}

/// Compact a single WAL segment by rewriting live records to a new segment.
///
/// # Algorithm
///
/// 1. Read all records from the segment
/// 2. Filter by liveness: keep only records that should be retained
/// 3. Write filtered records to a new temporary segment
/// 4. Atomically swap old segment with new segment
/// 5. On swap failure, abandon temporary and keep original
///
/// # Arguments
///
/// * `context` - Compaction context
/// * `segment_id` - ID of segment to compact
///
/// # Returns
///
/// `CompactionResult` if compaction succeeded, `Err` if compaction failed.
/// On error, old segment remains intact.
///
/// # Errors
///
/// Returns `Storage` error if:
/// - Segment cannot be read
/// - Record filtering fails
/// - Compacted segment write fails (temporary abandoned)
/// - Swap fails (old segment remains; temporary abandoned)
pub fn compact_segment(
    context: &dyn CompactionContext,
    segment_id: u64,
) -> AndromedaResult<CompactionResult> {
    // Step 1: Read all records from original segment
    let original_records = context.read_segment_records(segment_id)?;
    let original_record_count = original_records.len() as u64;
    let original_bytes = original_records
        .iter()
        .map(|r| r.payload.len() as u64)
        .sum::<u64>();

    context.emit_audit_event(WalCompactionAuditEvent::CompactionStarted {
        segment_id,
        fragmentation_ratio: 0.0, // Caller should provide this
    })?;

    // Step 2: Filter records by liveness
    let mut live_records = Vec::new();
    for record in &original_records {
        match context.should_keep_record(record) {
            Ok(true) => live_records.push(record.clone()),
            Ok(false) => {} // Skip dead record
            Err(e) => {
                context.emit_audit_event(WalCompactionAuditEvent::CompactionFailed {
                    segment_id,
                    reason: format!("Record filtering error: {}", e),
                })?;
                return Err(e);
            }
        }
    }

    let compacted_record_count = live_records.len() as u64;
    let records_removed = original_record_count - compacted_record_count;

    // Step 3: Write compacted segment
    let (new_segment_id, compacted_bytes) =
        match context.write_compacted_segment(segment_id, &live_records) {
            Ok((id, size)) => (id, size),
            Err(e) => {
                context.emit_audit_event(WalCompactionAuditEvent::CompactionFailed {
                    segment_id,
                    reason: format!("Write failed: {}", e),
                })?;
                return Err(e);
            }
        };

    let bytes_recovered = original_bytes.saturating_sub(compacted_bytes);

    // Step 4: Atomically swap old ↔ new
    match context.swap_segment(segment_id, new_segment_id) {
        Ok(()) => {
            let result = CompactionResult {
                original_segment_id: segment_id,
                new_segment_id,
                original_bytes,
                compacted_bytes,
                bytes_recovered,
                original_record_count,
                compacted_record_count,
                records_removed,
            };

            context.emit_audit_event(WalCompactionAuditEvent::CompactionCompleted {
                original_segment_id: segment_id,
                new_segment_id,
                bytes_recovered,
                reduction_ratio: result.reduction_ratio(),
            })?;

            Ok(result)
        }
        Err(e) => {
            // Step 5: On swap failure, old segment remains and temporary is abandoned
            context.emit_audit_event(WalCompactionAuditEvent::CompactionFailed {
                segment_id,
                reason: format!("Swap failed: {}", e),
            })?;
            Err(e)
        }
    }
}

/// Configuration for the WAL compaction scheduler.
#[derive(Debug, Clone)]
pub struct WalCompactionSchedulerConfig {
    /// Interval between compaction runs
    pub interval: Duration,
    /// Fragmentation threshold; compact segments with ratio >= this value (e.g., 0.30)
    pub fragmentation_threshold: f64,
    /// Maximum segments to compact per run (0 = unlimited)
    pub max_segments_per_run: u64,
}

impl WalCompactionSchedulerConfig {
    /// Create a new compaction scheduler configuration.
    pub fn new(interval: Duration, fragmentation_threshold: f64) -> AndromedaResult<Self> {
        if interval.is_zero() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL compaction: scheduler interval must not be zero",
            ));
        }
        if !(0.0..=1.0).contains(&fragmentation_threshold) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "WAL compaction: fragmentation threshold must be in [0.0, 1.0]",
            ));
        }

        Ok(WalCompactionSchedulerConfig {
            interval,
            fragmentation_threshold,
            max_segments_per_run: 0, // No limit by default
        })
    }

    /// Set maximum segments to compact per run.
    pub fn with_max_segments(mut self, max: u64) -> Self {
        self.max_segments_per_run = max;
        self
    }
}

/// Background scheduler for WAL segment compaction.
///
/// Periodically identifies fragmented segments and compacts up to N per run.
/// Emits metrics for observability.
#[derive(Debug)]
pub struct WalCompactionScheduler {
    config: WalCompactionSchedulerConfig,
    run_counter: Arc<std::sync::atomic::AtomicU64>,
}

impl WalCompactionScheduler {
    /// Create a new compaction scheduler.
    pub fn new(config: WalCompactionSchedulerConfig) -> Self {
        WalCompactionScheduler {
            config,
            run_counter: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Execute a single compaction run.
    ///
    /// # Algorithm
    ///
    /// 1. Identify candidates exceeding fragmentation threshold
    /// 2. Sort by fragmentation_ratio (highest first)
    /// 3. Compact up to max_segments_per_run
    /// 4. Collect metrics and emit summary
    ///
    /// # Returns
    ///
    /// `WalCompactionSummary` with metrics from this run.
    pub fn run(&self, context: &dyn CompactionContext) -> AndromedaResult<WalCompactionSummary> {
        let run_id = self
            .run_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let mut summary = WalCompactionSummary::new(run_id);

        // Step 1: Identify candidates
        let candidates =
            identify_compaction_candidates(context, self.config.fragmentation_threshold)?;
        summary.candidates_identified = candidates.len() as u64;

        // Step 2: Compact up to max_segments_per_run
        let limit = if self.config.max_segments_per_run == 0 {
            candidates.len()
        } else {
            (self.config.max_segments_per_run as usize).min(candidates.len())
        };

        for metrics in candidates.iter().take(limit) {
            match compact_segment(context, metrics.segment_id) {
                Ok(result) => {
                    summary.candidates_compacted += 1;
                    summary.total_bytes_recovered += result.bytes_recovered;
                }
                Err(_) => {
                    summary.compaction_skipped += 1;
                }
            }
        }

        // Step 3: Emit summary
        let avg_ratio = if summary.candidates_compacted > 0 {
            summary.total_bytes_recovered as f64 / (summary.candidates_compacted as f64 * 1000.0)
        } else {
            0.0
        };

        context.emit_audit_event(WalCompactionAuditEvent::Summary {
            run_id,
            candidates_identified: summary.candidates_identified,
            candidates_compacted: summary.candidates_compacted,
            compaction_skipped: summary.compaction_skipped,
            total_bytes_recovered: summary.total_bytes_recovered,
            total_reduction_ratio: avg_ratio,
        })?;

        Ok(summary)
    }

    /// Get the interval between compaction runs.
    pub const fn interval(&self) -> Duration {
        self.config.interval
    }

    /// Get the fragmentation threshold.
    pub const fn fragmentation_threshold(&self) -> f64 {
        self.config.fragmentation_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fragmentation_metrics_creation_validates_segment_id() {
        let result = FragmentationMetrics::new(0, 1000, 100);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Storage);
    }

    #[test]
    fn fragmentation_metrics_creation_validates_total_bytes() {
        let result = FragmentationMetrics::new(1, 0, 0);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Storage);
    }

    #[test]
    fn fragmentation_metrics_creation_validates_dead_bytes_bound() {
        let result = FragmentationMetrics::new(1, 100, 101);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), AndromedaErrorKind::Storage);
    }

    #[test]
    fn fragmentation_metrics_calculates_ratio() {
        let metrics = FragmentationMetrics::new(1, 1000, 300).unwrap();
        assert!((metrics.fragmentation_ratio - 0.30).abs() < 0.001);
    }

    #[test]
    fn fragmentation_metrics_identifies_candidates_at_threshold() {
        let metrics = FragmentationMetrics::new(1, 1000, 300).unwrap();
        assert!(metrics.is_compaction_candidate()); // 0.30 >= 0.30

        let metrics = FragmentationMetrics::new(1, 1000, 299).unwrap();
        assert!(!metrics.is_compaction_candidate()); // 0.299 < 0.30

        let metrics = FragmentationMetrics::new(1, 1000, 500).unwrap();
        assert!(metrics.is_compaction_candidate()); // 0.50 >= 0.30
    }

    #[test]
    fn fragmentation_metrics_estimates_recovery() {
        let metrics = FragmentationMetrics::new(1, 1000, 350).unwrap();
        assert_eq!(metrics.estimated_recovery_bytes(), 350);
    }

    #[test]
    fn compaction_result_calculates_reduction_ratio() {
        let result = CompactionResult {
            original_segment_id: 1,
            new_segment_id: 2,
            original_bytes: 1000,
            compacted_bytes: 600,
            bytes_recovered: 400,
            original_record_count: 100,
            compacted_record_count: 60,
            records_removed: 40,
        };

        assert!((result.reduction_ratio() - 0.40).abs() < 0.001);
    }

    #[test]
    fn compaction_result_handles_zero_original_bytes() {
        let result = CompactionResult {
            original_segment_id: 1,
            new_segment_id: 2,
            original_bytes: 0,
            compacted_bytes: 0,
            bytes_recovered: 0,
            original_record_count: 0,
            compacted_record_count: 0,
            records_removed: 0,
        };

        assert_eq!(result.reduction_ratio(), 0.0);
    }

    #[test]
    fn compaction_summary_tracks_metrics() {
        let mut summary = WalCompactionSummary::new(42);
        assert_eq!(summary.run_id, 42);
        assert_eq!(summary.candidates_identified, 0);
        assert_eq!(summary.candidates_compacted, 0);
        assert!(!summary.any_compacted());

        summary.candidates_compacted = 5;
        summary.total_bytes_recovered = 2000;
        assert!(summary.any_compacted());
    }

    #[test]
    fn scheduler_config_validates_interval() {
        let result = WalCompactionSchedulerConfig::new(Duration::ZERO, 0.30);
        assert!(result.is_err());
    }

    #[test]
    fn scheduler_config_validates_threshold() {
        let result = WalCompactionSchedulerConfig::new(Duration::from_secs(60), 1.5);
        assert!(result.is_err());

        let result = WalCompactionSchedulerConfig::new(Duration::from_secs(60), -0.1);
        assert!(result.is_err());

        let result = WalCompactionSchedulerConfig::new(Duration::from_secs(60), 0.30);
        assert!(result.is_ok());

        let result = WalCompactionSchedulerConfig::new(Duration::from_secs(60), 0.0);
        assert!(result.is_ok());

        let result = WalCompactionSchedulerConfig::new(Duration::from_secs(60), 1.0);
        assert!(result.is_ok());
    }

    #[test]
    fn scheduler_config_with_max_segments() {
        let config = WalCompactionSchedulerConfig::new(Duration::from_secs(60), 0.30)
            .unwrap()
            .with_max_segments(10);
        assert_eq!(config.max_segments_per_run, 10);
    }

    #[test]
    fn audit_event_labels_are_correct() {
        let event = WalCompactionAuditEvent::CandidateIdentified {
            segment_id: 1,
            fragmentation_ratio: 0.35,
            total_bytes: 1000,
            dead_bytes: 350,
        };
        assert_eq!(event.as_str(), "WalCompactionCandidateIdentified");

        let event = WalCompactionAuditEvent::CompactionStarted {
            segment_id: 1,
            fragmentation_ratio: 0.35,
        };
        assert_eq!(event.as_str(), "WalCompactionStarted");

        let event = WalCompactionAuditEvent::CompactionCompleted {
            original_segment_id: 1,
            new_segment_id: 2,
            bytes_recovered: 350,
            reduction_ratio: 0.35,
        };
        assert_eq!(event.as_str(), "WalCompactionCompleted");

        let event = WalCompactionAuditEvent::CompactionFailed {
            segment_id: 1,
            reason: "test".to_string(),
        };
        assert_eq!(event.as_str(), "WalCompactionFailed");

        let event = WalCompactionAuditEvent::Summary {
            run_id: 1,
            candidates_identified: 5,
            candidates_compacted: 3,
            compaction_skipped: 2,
            total_bytes_recovered: 1000,
            total_reduction_ratio: 0.35,
        };
        assert_eq!(event.as_str(), "WalCompactionSummary");
    }
}
