use super::*;
use andromeda_error::AndromedaErrorKind;
use std::time::Duration;

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
