use crate::support::*;
use andromeda_restore::plan_replay_segments;
use andromeda_wal::Lsn;

#[test]
fn test_plan_replay_single_segment_containing_pitr() {
    let manifest = make_test_manifest();
    let segments = vec![make_wal_segment(1001, 2000, None)];
    let pitr_lsn = Lsn::new(1500);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments);
    assert!(plan.is_ok());

    let segments_to_replay = plan.unwrap();
    assert_eq!(segments_to_replay.len(), 1);
    assert!(segments_to_replay[0].contains_pitr_target);
    assert_eq!(segments_to_replay[0].sequence_index, 0);
    assert_eq!(segments_to_replay[0].replay_stop_lsn, pitr_lsn);
}

#[test]
fn test_plan_replay_multiple_segments() {
    let manifest = make_test_manifest();
    let segments = vec![
        make_wal_segment(1001, 1500, None),
        make_wal_segment(1501, 2000, Some(1500)),
    ];
    let pitr_lsn = Lsn::new(1750);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments);
    assert!(plan.is_ok());

    let segments_to_replay = plan.unwrap();
    assert_eq!(segments_to_replay.len(), 2);
    assert!(!segments_to_replay[0].contains_pitr_target);
    assert!(segments_to_replay[1].contains_pitr_target);
    assert_eq!(segments_to_replay[0].replay_stop_lsn, Lsn::new(1500));
    assert_eq!(segments_to_replay[1].replay_stop_lsn, pitr_lsn);
}

#[test]
fn test_plan_replay_stops_after_pitr_segment() {
    let manifest = make_test_manifest();
    let segments = vec![
        make_wal_segment(1001, 1500, None),
        make_wal_segment(1501, 2000, Some(1500)),
    ];
    let pitr_lsn = Lsn::new(1250);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments);
    assert!(plan.is_ok());

    let segments_to_replay = plan.unwrap();
    assert_eq!(segments_to_replay.len(), 1);
    assert!(segments_to_replay[0].contains_pitr_target);
    assert_eq!(segments_to_replay[0].replay_stop_lsn, pitr_lsn);
}

#[test]
fn test_plan_replay_empty_segments_rejected() {
    let manifest = make_test_manifest();
    let pitr_lsn = Lsn::new(1500);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &[]);
    assert!(plan.is_err());
}

#[test]
fn test_plan_replay_detects_lsn_gap() {
    let manifest = make_test_manifest();
    let segments = vec![
        make_wal_segment(1001, 1400, None),
        make_wal_segment(1402, 2000, Some(1401)),
    ];
    let pitr_lsn = Lsn::new(1500);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments);
    assert!(plan.is_err());
}

#[test]
fn test_plan_replay_requires_first_segment_at_archive_start() {
    let manifest = make_test_manifest();
    let segments = vec![make_wal_segment(1200, 2000, None)];
    let pitr_lsn = Lsn::new(1500);

    let err = plan_replay_segments(&manifest, pitr_lsn, &segments)
        .expect_err("restore must not skip WAL before the first segment");

    assert!(
        err.message().contains("archive start"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_plan_replay_rejects_segment_bounds_outside_manifest_archive() {
    let manifest = make_test_manifest();
    let segments = vec![make_wal_segment(1001, 2001, None)];

    let err = plan_replay_segments(&manifest, Lsn::new(1500), &segments)
        .expect_err("restore replay plan must reject WAL beyond backup archive bounds");

    assert!(
        err.message().contains("archive range"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_plan_replay_rejects_missing_segment_containing_target() {
    let manifest = make_test_manifest();
    let segments = vec![make_wal_segment(1001, 1500, None)];

    let err = plan_replay_segments(&manifest, Lsn::new(1750), &segments)
        .expect_err("restore replay plan must prove the PITR target segment exists");

    assert!(
        err.message().contains("PITR target"),
        "unexpected error: {err}"
    );
}

#[test]
fn test_plan_replay_validates_first_segment_no_previous_lsn() {
    let manifest = make_test_manifest();
    let segments = vec![make_wal_segment(1001, 1500, Some(1000))];
    let pitr_lsn = Lsn::new(1250);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments);
    assert!(plan.is_err());
}

#[test]
fn test_pitr_checkpoint_after_single_segment_replay() {
    let manifest = make_test_manifest();
    let segments = vec![make_wal_segment(1001, 2000, None)];
    let pitr_lsn = Lsn::new(1500);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments).expect("plan failed");

    assert_eq!(plan.len(), 1);
    assert_eq!(plan[0].segment_descriptor.first_lsn, Lsn::new(1001));
    assert_eq!(plan[0].segment_descriptor.last_lsn, Lsn::new(2000));
    assert!(plan[0].contains_pitr_target);
    assert_eq!(plan[0].replay_stop_lsn, pitr_lsn);
}

#[test]
fn test_pitr_checkpoint_preserves_segment_chain() {
    let manifest = make_test_manifest();
    let segments = vec![
        make_wal_segment(1001, 1500, None),
        make_wal_segment(1501, 2000, Some(1500)),
    ];
    let pitr_lsn = Lsn::new(1750);

    let plan = plan_replay_segments(&manifest, pitr_lsn, &segments).expect("plan failed");

    assert_eq!(plan.len(), 2);
    let seg1_end = plan[0].segment_descriptor.last_lsn;
    let seg2_start = plan[1].segment_descriptor.first_lsn;
    assert_eq!(seg1_end.get() + 1, seg2_start.get());
}
