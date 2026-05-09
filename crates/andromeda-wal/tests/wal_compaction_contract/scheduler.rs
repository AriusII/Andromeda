use std::time::Duration;

use andromeda_wal::write_ahead_log::compaction::{
    FragmentationMetrics, WalCompactionAuditEvent, WalCompactionScheduler,
    WalCompactionSchedulerConfig,
};

use crate::support::{MockCompactionContext, test_record};

#[test]
fn scheduler_identifies_and_compacts_candidates() {
    let mut records_set1 = vec![];
    for lsn in 1..=5 {
        records_set1.push(test_record(lsn));
    }

    let mut records_set2 = vec![];
    for lsn in 100..=110 {
        records_set2.push(test_record(lsn));
    }

    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(1, 1000, 350).unwrap())
        .with_fragmented_segment(FragmentationMetrics::new(2, 1500, 400).unwrap())
        .with_fragmented_segment(FragmentationMetrics::new(3, 500, 100).unwrap())
        .with_segment_records(1, records_set1)
        .with_segment_records(2, records_set2);

    let config = WalCompactionSchedulerConfig::new(Duration::from_secs(60), 0.30).unwrap();
    let scheduler = WalCompactionScheduler::new(config);

    let summary = scheduler.run(&context).unwrap();

    assert_eq!(summary.candidates_identified, 1);
    assert!(summary.candidates_compacted > 0 || summary.compaction_skipped > 0);
    assert_eq!(summary.run_id, 0);

    let events = context.get_audit_events();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, WalCompactionAuditEvent::CandidateIdentified { .. }))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, WalCompactionAuditEvent::Summary { .. }))
    );
}

#[test]
fn scheduler_respects_max_segments_per_run() {
    let context = MockCompactionContext::new()
        .with_fragmented_segment(FragmentationMetrics::new(1, 1000, 350).unwrap())
        .with_fragmented_segment(FragmentationMetrics::new(2, 1000, 350).unwrap())
        .with_fragmented_segment(FragmentationMetrics::new(3, 1000, 350).unwrap())
        .with_segment_records(1, vec![test_record(1)])
        .with_segment_records(2, vec![test_record(2)])
        .with_segment_records(3, vec![test_record(3)]);

    let config = WalCompactionSchedulerConfig::new(Duration::from_secs(60), 0.30)
        .unwrap()
        .with_max_segments(2);

    let scheduler = WalCompactionScheduler::new(config);
    let summary = scheduler.run(&context).unwrap();

    assert_eq!(summary.candidates_identified, 3);
    assert!(summary.candidates_compacted <= 2);
}

#[test]
fn scheduler_increments_run_id() {
    let context = MockCompactionContext::new();

    let config = WalCompactionSchedulerConfig::new(Duration::from_secs(60), 0.30).unwrap();
    let scheduler = WalCompactionScheduler::new(config);

    let summary1 = scheduler.run(&context).unwrap();
    let summary2 = scheduler.run(&context).unwrap();
    let summary3 = scheduler.run(&context).unwrap();

    assert_eq!(summary1.run_id, 0);
    assert_eq!(summary2.run_id, 1);
    assert_eq!(summary3.run_id, 2);
}

#[test]
fn scheduler_provides_interval_and_threshold_accessors() {
    let duration = Duration::from_secs(120);
    let threshold = 0.35;

    let config = WalCompactionSchedulerConfig::new(duration, threshold).unwrap();
    let scheduler = WalCompactionScheduler::new(config);

    assert_eq!(scheduler.interval(), duration);
    assert_eq!(scheduler.fragmentation_threshold(), threshold);
}
