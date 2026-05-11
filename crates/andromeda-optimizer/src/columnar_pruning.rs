//! Advisory columnar chunk pruning for optimizer plan paths.
//!
//! This module wires [`ColumnarSegmentDescriptor`] pruning metadata (min/max per
//! chunk) into the optimizer so that range and equality predicates can mark
//! column chunks as skippable before a scan begins.
//!
//! ## Ownership contract
//! - Pruning is **advisory only**.  If metadata is absent or the predicate is
//!   ambiguous, the chunk MUST be scanned.  Pruning never decides correctness.
//! - This path is **CPU-only**.  It must not be invoked from GPU, WAL, MVCC,
//!   commit, or recovery paths.
//! - `andromeda-decision-trace` does NOT depend on this crate.  The boundary
//!   conversion from columnar descriptor types to trace evidence lives here.
//! - `andromeda-columnar` does NOT depend on this crate.  Evaluation logic is
//!   owned by the optimizer.

use andromeda_columnar::ColumnarSegmentDescriptor;
use andromeda_decision_trace::{
    ColumnarPruningEvidence, DecisionFamily, DecisionOutcome, DecisionReasonCode, DecisionTrace,
    DecisionTraceId, VersionBinding,
};

use crate::OptimizerError;

// ── Public types ─────────────────────────────────────────────────────────────

/// A predicate evaluated against columnar chunk min/max metadata.
///
/// Advisory only — if the predicate is ambiguous or the chunk has no min/max
/// values, the chunk is always scanned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnScanPredicate {
    /// Equality predicate: a chunk can be skipped when its entire range
    /// `[chunk_min, chunk_max]` lies outside `value`.
    Equality {
        /// The equality target value.
        value: i64,
    },
    /// Closed-range predicate: a chunk can be skipped when its range
    /// `[chunk_min, chunk_max]` is disjoint from `[min_inclusive, max_inclusive]`.
    Range {
        /// Lower bound of the predicate range (inclusive).
        min_inclusive: i64,
        /// Upper bound of the predicate range (inclusive).
        max_inclusive: i64,
    },
}

/// Result of advisory columnar chunk pruning evaluation.
///
/// Reports how many chunks were determined to be outside the predicate range
/// (`skipped`) versus the total evaluated (`total`).  The trace contains
/// [`ColumnarPruningEvidence`] for observability.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnarPruningDecision {
    skipped: usize,
    total: usize,
    trace: DecisionTrace,
}

impl ColumnarPruningDecision {
    /// Chunks that were determined to be outside the predicate range and skipped.
    pub const fn skipped(&self) -> usize {
        self.skipped
    }

    /// Total chunks evaluated for the requested column ordinal.
    pub const fn total(&self) -> usize {
        self.total
    }

    /// Chunks that still need to be scanned (`total - skipped`).
    pub fn scannable(&self) -> usize {
        self.total.saturating_sub(self.skipped)
    }

    /// Returns `true` when at least one chunk was pruned.
    pub const fn any_pruned(&self) -> bool {
        self.skipped > 0
    }

    /// The [`DecisionTrace`] emitted for this pruning evaluation.
    pub const fn trace(&self) -> &DecisionTrace {
        &self.trace
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Evaluate advisory columnar chunk pruning for a given column and predicate.
///
/// For each chunk whose `column_ordinal` matches the requested one:
/// - If the chunk has both `min_value` and `max_value` metadata **and** they are
///   disjoint from `predicate` → the chunk is marked as skipped (pruned).
/// - Otherwise → the chunk must be scanned (CPU fallback).
///
/// A [`DecisionTrace`] carrying [`ColumnarPruningEvidence`] is always produced so
/// that the optimizer decision is observable.
///
/// # Errors
/// Returns [`OptimizerError::TraceBuildFailed`] when trace construction fails.
///
/// # Panics
/// Never panics; the function is fully bounded.
pub fn evaluate_columnar_pruning(
    descriptor: &ColumnarSegmentDescriptor,
    column_ordinal: u16,
    predicate: ColumnScanPredicate,
    trace_id: DecisionTraceId,
) -> Result<ColumnarPruningDecision, OptimizerError> {
    // Collect all chunks for the requested column.
    let relevant: Vec<_> = descriptor
        .chunks()
        .iter()
        .filter(|chunk| chunk.column_ordinal == column_ordinal)
        .collect();

    let total = relevant.len();
    let mut skipped: usize = 0;

    for chunk in &relevant {
        // Advisory: only prune when BOTH actual min/max values are present.
        // Missing metadata → must scan (CPU fallback).
        if let Some((chunk_min, chunk_max)) = chunk.pruning.effective_min_max()
            && is_disjoint(predicate, chunk_min, chunk_max)
        {
            skipped += 1;
        }
    }

    let (outcome, reason_str) = if skipped > 0 {
        (DecisionOutcome::Accepted, "columnar-pruning-applied")
    } else {
        (DecisionOutcome::Fallback, "columnar-pruning-unavailable")
    };

    let explanation = format!(
        "columnar-pruning column_ordinal={column_ordinal} predicate={} \
         skipped={skipped} total={total} advisory_only=true cpu_only=true",
        predicate_label(predicate),
    );

    let pruning_evidence = ColumnarPruningEvidence { skipped, total };

    let trace = DecisionTrace::new(
        trace_id,
        DecisionFamily::OptimizerPlan,
        outcome,
        DecisionReasonCode::new(reason_str).map_err(|_| OptimizerError::TraceBuildFailed)?,
        explanation,
        VersionBinding::empty(),
    )
    .map_err(|_| OptimizerError::TraceBuildFailed)?
    .with_columnar_pruning(pruning_evidence);

    Ok(ColumnarPruningDecision {
        skipped,
        total,
        trace,
    })
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Returns `true` when the predicate range and the chunk range are disjoint,
/// meaning no row in this chunk can satisfy the predicate.
///
/// Conservative: any ambiguity (inverted ranges) returns `false` (must scan).
fn is_disjoint(predicate: ColumnScanPredicate, chunk_min: i64, chunk_max: i64) -> bool {
    // An inverted chunk range is always inconclusive → must scan.
    if chunk_min > chunk_max {
        return false;
    }

    match predicate {
        ColumnScanPredicate::Equality { value } => {
            // Prune when the single target value falls entirely outside [chunk_min, chunk_max].
            value < chunk_min || value > chunk_max
        },
        ColumnScanPredicate::Range {
            min_inclusive,
            max_inclusive,
        } => {
            // Ambiguous (inverted) predicate range → must scan.
            if min_inclusive > max_inclusive {
                return false;
            }
            // Prune when ranges do not overlap:
            //   predicate ends before chunk starts, OR predicate starts after chunk ends.
            max_inclusive < chunk_min || min_inclusive > chunk_max
        },
    }
}

fn predicate_label(predicate: ColumnScanPredicate) -> String {
    match predicate {
        ColumnScanPredicate::Equality { value } => format!("eq({value})"),
        ColumnScanPredicate::Range {
            min_inclusive,
            max_inclusive,
        } => format!("range([{min_inclusive},{max_inclusive}])"),
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equality_prune_when_value_below_chunk_min() {
        assert!(is_disjoint(
            ColumnScanPredicate::Equality { value: 5 },
            10,
            20
        ));
    }

    #[test]
    fn equality_prune_when_value_above_chunk_max() {
        assert!(is_disjoint(
            ColumnScanPredicate::Equality { value: 25 },
            10,
            20
        ));
    }

    #[test]
    fn equality_no_prune_when_value_within_range() {
        assert!(!is_disjoint(
            ColumnScanPredicate::Equality { value: 15 },
            10,
            20
        ));
    }

    #[test]
    fn equality_no_prune_at_boundaries() {
        assert!(!is_disjoint(
            ColumnScanPredicate::Equality { value: 10 },
            10,
            20
        ));
        assert!(!is_disjoint(
            ColumnScanPredicate::Equality { value: 20 },
            10,
            20
        ));
    }

    #[test]
    fn range_prune_when_predicate_entirely_below_chunk() {
        assert!(is_disjoint(
            ColumnScanPredicate::Range {
                min_inclusive: 1,
                max_inclusive: 9
            },
            10,
            20
        ));
    }

    #[test]
    fn range_prune_when_predicate_entirely_above_chunk() {
        assert!(is_disjoint(
            ColumnScanPredicate::Range {
                min_inclusive: 21,
                max_inclusive: 30
            },
            10,
            20
        ));
    }

    #[test]
    fn range_no_prune_when_predicate_overlaps_chunk() {
        assert!(!is_disjoint(
            ColumnScanPredicate::Range {
                min_inclusive: 15,
                max_inclusive: 25
            },
            10,
            20
        ));
    }

    #[test]
    fn range_no_prune_at_touching_boundaries() {
        assert!(!is_disjoint(
            ColumnScanPredicate::Range {
                min_inclusive: 20,
                max_inclusive: 30
            },
            10,
            20
        ));
        assert!(!is_disjoint(
            ColumnScanPredicate::Range {
                min_inclusive: 1,
                max_inclusive: 10
            },
            10,
            20
        ));
    }

    #[test]
    fn inverted_chunk_range_is_always_scannable() {
        // chunk_min > chunk_max → conservative, must scan.
        assert!(!is_disjoint(
            ColumnScanPredicate::Equality { value: 100 },
            20,
            10
        ));
    }

    #[test]
    fn inverted_predicate_range_is_always_scannable() {
        assert!(!is_disjoint(
            ColumnScanPredicate::Range {
                min_inclusive: 30,
                max_inclusive: 5
            },
            10,
            20
        ));
    }
}
