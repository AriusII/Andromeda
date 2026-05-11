//! Integration test: optimizer columnar pruning wires ColumnarSegmentDescriptor
//! min/max metadata into a DecisionTrace with ColumnarPruningEvidence.
//!
//! Golden assertion: chunks outside predicate range are skipped and the trace
//! carries verifiable ColumnarPruningEvidence.

#![forbid(unsafe_code)]

use std::num::{NonZeroU16, NonZeroU64};

use andromeda_columnar::{
    ColumnChunkDescriptor, ColumnChunkPruningMetadata, ColumnarAccelerationPolicy,
    ColumnarConsumer, ColumnarLayoutDescriptor, ColumnarSegmentDescriptor, ColumnarSnapshotBinding,
    ColumnarVersionBinding,
};
use andromeda_decision_trace::DecisionTraceId;
use andromeda_optimizer::{ColumnScanPredicate, evaluate_columnar_pruning};

/// Helper: build a minimal ColumnarLayoutDescriptor.
fn layout() -> ColumnarLayoutDescriptor {
    ColumnarLayoutDescriptor::new(
        ColumnarConsumer::Analytics,
        NonZeroU16::new(1).unwrap(),
        ColumnarVersionBinding::new(
            NonZeroU64::new(1).unwrap(),
            NonZeroU64::new(1).unwrap(),
            NonZeroU64::new(1).unwrap(),
            None,
        ),
        ColumnarAccelerationPolicy::CpuOnly,
    )
}

/// Helper: build a ColumnarSnapshotBinding.
fn snapshot() -> ColumnarSnapshotBinding {
    ColumnarSnapshotBinding::new(NonZeroU64::new(42).unwrap())
}

/// Golden test: chunks outside the predicate range are skipped; the trace
/// carries [`ColumnarPruningEvidence`] confirming `skipped` and `total`.
///
/// Segment layout (column 0, four chunks):
///
/// | chunk | min  | max  | predicate [20, 30] | verdict  |
/// |-------|------|------|--------------------|----------|
/// | 0     | 1    | 10   | disjoint (below)   | SKIP     |
/// | 1     | 11   | 20   | touches boundary   | SCAN     |
/// | 2     | 31   | 100  | disjoint (above)   | SKIP     |
/// | 3     | none | none | no metadata        | SCAN     |
#[test]
fn optimizer_columnar_pruning_skips_chunks_outside_predicate_range() {
    let chunks = vec![
        // chunk 0: [1, 10] → disjoint with [20, 30] → SKIP
        ColumnChunkDescriptor::new(
            0,
            NonZeroU64::new(1_000).unwrap(),
            ColumnChunkPruningMetadata::new(true, true, false).with_min_max_values(1, 10),
        ),
        // chunk 1: [11, 20] → boundary touches predicate min=20 → SCAN
        ColumnChunkDescriptor::new(
            0,
            NonZeroU64::new(1_000).unwrap(),
            ColumnChunkPruningMetadata::new(true, true, false).with_min_max_values(11, 20),
        ),
        // chunk 2: [31, 100] → disjoint with [20, 30] → SKIP
        ColumnChunkDescriptor::new(
            0,
            NonZeroU64::new(1_000).unwrap(),
            ColumnChunkPruningMetadata::new(true, true, false).with_min_max_values(31, 100),
        ),
        // chunk 3: no actual min/max values → CPU fallback → SCAN
        ColumnChunkDescriptor::new(
            0,
            NonZeroU64::new(500).unwrap(),
            ColumnChunkPruningMetadata::new(false, false, false),
        ),
    ];

    let descriptor = ColumnarSegmentDescriptor::new(layout(), snapshot(), chunks);

    let predicate = ColumnScanPredicate::Range {
        min_inclusive: 20,
        max_inclusive: 30,
    };

    let decision = evaluate_columnar_pruning(
        &descriptor,
        0, // column_ordinal
        predicate,
        DecisionTraceId::new(1).expect("trace id must be non-zero"),
    )
    .expect("columnar pruning evaluation must not fail");

    // ── Golden: chunk count assertions ───────────────────────────────────────
    assert_eq!(
        decision.total(),
        4,
        "four chunks were registered for column 0"
    );
    assert_eq!(
        decision.skipped(),
        2,
        "chunks [1,10] and [31,100] are disjoint with predicate [20,30]"
    );
    assert_eq!(
        decision.scannable(),
        2,
        "chunks touching boundary or lacking metadata must be scanned"
    );
    assert!(decision.any_pruned(), "at least one chunk must be pruned");

    // ── Golden: DecisionTrace evidence emitted ───────────────────────────────
    let trace = decision.trace();

    // The trace must be observable (non-empty reason code + explanation).
    assert!(
        trace.is_observable(),
        "pruning trace must be observable: {trace}"
    );

    // Columnar pruning evidence must be attached to the trace.
    let evidence = trace
        .columnar_pruning()
        .expect("ColumnarPruningEvidence must be present in the trace");

    assert_eq!(
        evidence.skipped, 2,
        "trace evidence skipped count must match decision"
    );
    assert_eq!(
        evidence.total, 4,
        "trace evidence total count must match decision"
    );
    assert_eq!(
        evidence.scannable(),
        2,
        "trace evidence scannable count must be consistent"
    );
    assert!(evidence.any_pruned(), "trace evidence must report pruning");
}

/// Advisory fallback: when NO chunks have min/max metadata, the trace reports
/// zero skipped and outcome is fallback (full scan required).
#[test]
fn optimizer_columnar_pruning_falls_back_when_no_metadata_available() {
    let chunks = vec![
        ColumnChunkDescriptor::new(
            0,
            NonZeroU64::new(2_000).unwrap(),
            ColumnChunkPruningMetadata::new(false, false, false),
        ),
        ColumnChunkDescriptor::new(
            0,
            NonZeroU64::new(2_000).unwrap(),
            ColumnChunkPruningMetadata::new(false, false, false),
        ),
    ];

    let descriptor = ColumnarSegmentDescriptor::new(layout(), snapshot(), chunks);

    let decision = evaluate_columnar_pruning(
        &descriptor,
        0,
        ColumnScanPredicate::Equality { value: 99 },
        DecisionTraceId::new(2).expect("trace id"),
    )
    .expect("evaluation must succeed even without pruning metadata");

    assert_eq!(decision.skipped(), 0, "no metadata → no pruning");
    assert_eq!(decision.total(), 2);
    assert!(!decision.any_pruned());

    let evidence = decision
        .trace()
        .columnar_pruning()
        .expect("trace must carry pruning evidence even on fallback");
    assert_eq!(evidence.skipped, 0);
    assert_eq!(evidence.total, 2);
    assert!(!evidence.any_pruned());
}

/// Boundary invariant: chunks belonging to a different column ordinal are
/// NOT evaluated (they are not in scope for this column's predicate).
#[test]
fn optimizer_columnar_pruning_only_evaluates_requested_column_ordinal() {
    let chunks = vec![
        // column 0 — should be evaluated
        ColumnChunkDescriptor::new(
            0,
            NonZeroU64::new(100).unwrap(),
            ColumnChunkPruningMetadata::new(true, true, false).with_min_max_values(1, 5),
        ),
        // column 1 — must be ignored
        ColumnChunkDescriptor::new(
            1,
            NonZeroU64::new(100).unwrap(),
            ColumnChunkPruningMetadata::new(true, true, false).with_min_max_values(1, 5),
        ),
    ];

    let descriptor = ColumnarSegmentDescriptor::new(layout(), snapshot(), chunks);

    let decision = evaluate_columnar_pruning(
        &descriptor,
        0, // only column 0
        ColumnScanPredicate::Range {
            min_inclusive: 50,
            max_inclusive: 100,
        },
        DecisionTraceId::new(3).expect("trace id"),
    )
    .expect("evaluation must succeed");

    // Only the column-0 chunk is counted; column-1 chunk is invisible.
    assert_eq!(decision.total(), 1, "only column-0 chunks are in scope");
    assert_eq!(
        decision.skipped(),
        1,
        "column-0 chunk [1,5] is disjoint with [50,100]"
    );
}

/// Advisory contract: equality predicate prunes chunks that do not contain the
/// target value.
#[test]
fn optimizer_columnar_pruning_equality_predicate_prunes_non_matching_chunks() {
    let chunks = vec![
        // contains value 50 → SCAN
        ColumnChunkDescriptor::new(
            0,
            NonZeroU64::new(100).unwrap(),
            ColumnChunkPruningMetadata::new(true, true, false).with_min_max_values(40, 60),
        ),
        // does not contain value 50 → SKIP
        ColumnChunkDescriptor::new(
            0,
            NonZeroU64::new(100).unwrap(),
            ColumnChunkPruningMetadata::new(true, true, false).with_min_max_values(70, 90),
        ),
        // does not contain value 50 → SKIP
        ColumnChunkDescriptor::new(
            0,
            NonZeroU64::new(100).unwrap(),
            ColumnChunkPruningMetadata::new(true, true, false).with_min_max_values(1, 30),
        ),
    ];

    let descriptor = ColumnarSegmentDescriptor::new(layout(), snapshot(), chunks);

    let decision = evaluate_columnar_pruning(
        &descriptor,
        0,
        ColumnScanPredicate::Equality { value: 50 },
        DecisionTraceId::new(4).expect("trace id"),
    )
    .expect("evaluation must succeed");

    assert_eq!(decision.total(), 3);
    assert_eq!(decision.skipped(), 2, "two chunks do not contain value 50");
    assert_eq!(decision.scannable(), 1);
}
