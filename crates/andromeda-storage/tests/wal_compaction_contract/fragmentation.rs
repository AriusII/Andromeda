use andromeda_storage::write_ahead_log::compaction::{
    FragmentationMetrics, WalCompactionAuditEvent, identify_compaction_candidates,
};

use crate::support::MockCompactionContext;

#[test]
fn fragmentation_ratio_calculation_exact_threshold() {
    let metrics = FragmentationMetrics::new(1, 100, 30).unwrap();
    assert!((metrics.fragmentation_ratio - 0.30).abs() < 0.001);
    assert!(metrics.is_compaction_candidate());
}

#[test]
fn fragmentation_ratio_calculation_above_threshold() {
    let metrics = FragmentationMetrics::new(1, 100, 50).unwrap();
    assert!((metrics.fragmentation_ratio - 0.50).abs() < 0.001);
    assert!(metrics.is_compaction_candidate());
}

#[test]
fn fragmentation_ratio_calculation_below_threshold() {
    let metrics = FragmentationMetrics::new(1, 1000, 299).unwrap();
    assert!(metrics.fragmentation_ratio < 0.30);
    assert!(!metrics.is_compaction_candidate());

    let metrics = FragmentationMetrics::new(1, 100, 29).unwrap();
    assert!(metrics.fragmentation_ratio < 0.30);
    assert!(!metrics.is_compaction_candidate());
}

#[test]
fn identify_compaction_candidates_sorts_by_fragmentation() {
    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(1, 1000, 300).unwrap())
        .with_fragmented_segment(FragmentationMetrics::new(2, 1000, 500).unwrap())
        .with_fragmented_segment(FragmentationMetrics::new(3, 1000, 350).unwrap())
        .with_fragmented_segment(FragmentationMetrics::new(4, 1000, 200).unwrap());

    let candidates = identify_compaction_candidates(&context, 0.30).unwrap();

    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0].segment_id, 2);
    assert_eq!(candidates[1].segment_id, 3);
    assert_eq!(candidates[2].segment_id, 1);

    let events = context.get_audit_events();
    let candidate_events = events
        .iter()
        .filter(|event| matches!(event, WalCompactionAuditEvent::CandidateIdentified { .. }))
        .count();
    assert_eq!(candidate_events, 3);
}
