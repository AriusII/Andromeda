use super::common::*;

#[test]
fn zero_switch_trace_rejects_without_state_change() {
    let mut switch = StatsPublicationSwitch::new();
    switch
        .stage_candidate(
            publication(1, 10),
            TraceId::new(100),
            "candidate staged before zero trace publish rejection",
        )
        .unwrap();
    switch
        .validate_candidate(TraceId::new(101), "candidate passed canonical validation")
        .unwrap();
    let history_before = switch.history().len();

    let err = switch
        .publish_validated_candidate(
            TraceId::new(0),
            canonical_evidence(400),
            "zero switch trace must not publish active StatsVersion",
        )
        .unwrap_err();
    assert_eq!(err, StatsPublicationSwitchError::TraceIdZero);
    assert!(switch.active().is_none());
    assert_eq!(
        switch.pending_state(),
        Some(StatsPublicationCandidateState::Validated)
    );
    assert_eq!(switch.history().len(), history_before);

    switch
        .publish_validated_candidate(
            TraceId::new(102),
            canonical_evidence(402),
            "nonzero trace publishes active StatsVersion after rejection",
        )
        .unwrap();
    publish(&mut switch, publication(2, 20), 500);
    let active_before = switch.active().unwrap().summary();
    let history_before = switch.history().len();

    let err = switch
        .rollback_last_publish(
            TraceId::new(0),
            recovery_evidence(501),
            "zero switch trace must not rollback active StatsVersion",
        )
        .unwrap_err();
    assert_eq!(err, StatsPublicationSwitchError::TraceIdZero);
    assert_eq!(switch.active().unwrap().summary(), active_before);
    assert_eq!(switch.history().len(), history_before);
}

#[test]
fn publication_switch_reason_is_trimmed_and_bounded() {
    let mut switch = StatsPublicationSwitch::new();
    let staged = switch
        .stage_candidate(
            publication(1, 10),
            TraceId::new(80),
            "  candidate staged with surrounding whitespace  ",
        )
        .unwrap();

    assert!(
        staged
            .reason
            .starts_with("candidate staged with surrounding whitespace")
    );
    assert!(staged.reason.contains("stage=Candidate"));
    assert!(
        staged
            .reason
            .contains("candidate=catalog_version:2,version:1")
    );

    let too_long = "x".repeat(STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES + 1);
    let err = switch
        .reject_candidate(TraceId::new(81), too_long)
        .unwrap_err();
    assert_eq!(err, StatsPublicationSwitchError::ReasonTooLong);
    assert!(
        switch.pending_candidate().is_some(),
        "invalid reject reason must not drop the pending candidate"
    );
    switch
        .reject_candidate(
            TraceId::new(82),
            "candidate rejected after bounded reason validation",
        )
        .unwrap();
}

#[test]
fn publication_switch_history_is_bounded_and_retains_recent_traces() {
    let mut switch = StatsPublicationSwitch::new();

    for index in 0..(STATS_PUBLICATION_SWITCH_HISTORY_LIMIT + 8) {
        let trace_id = 1_000 + (index as u128 * 2);
        switch
            .stage_candidate(
                publication((index + 1) as u64, 10 + index as u64),
                TraceId::new(trace_id),
                "candidate staged for bounded history retention",
            )
            .unwrap();
        switch
            .reject_candidate(
                TraceId::new(trace_id + 1),
                "candidate explicitly rejected after review",
            )
            .unwrap();
    }

    assert_eq!(
        switch.history().len(),
        STATS_PUBLICATION_SWITCH_HISTORY_LIMIT
    );
    let retained_first_iteration =
        (STATS_PUBLICATION_SWITCH_HISTORY_LIMIT + 8) - (STATS_PUBLICATION_SWITCH_HISTORY_LIMIT / 2);
    assert_eq!(
        switch.history().first().unwrap().trace_id,
        TraceId::new(1_000 + (retained_first_iteration as u128 * 2))
    );
    assert_eq!(
        switch.history().last().unwrap().trace_id,
        TraceId::new(1_000 + ((STATS_PUBLICATION_SWITCH_HISTORY_LIMIT + 7) as u128 * 2) + 1)
    );
}
