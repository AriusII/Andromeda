use super::common::*;

#[test]
fn active_stats_version_transition_requires_authoritative_decision_evidence() {
    assert_eq!(
        StatsPublicationDecisionEvidence::canonical_validation(TraceId::new(0)).unwrap_err(),
        StatsPublicationSwitchError::DecisionEvidenceTraceIdZero
    );
    assert_eq!(
        StatsPublicationDecisionEvidence::new(
            StatsPublicationDecisionEvidenceKind::PredictiveScenarioEvidence,
            TraceId::new(284),
        )
        .unwrap_err(),
        StatsPublicationSwitchError::PredictiveEvidenceRequiresAdvisoryIdentity
    );

    let mut switch = StatsPublicationSwitch::new();
    switch
        .stage_candidate(
            publication(1, 10),
            TraceId::new(90),
            "candidate staged before predictive-only rejection",
        )
        .unwrap();
    switch
        .validate_candidate(TraceId::new(91), "candidate passed canonical validation")
        .unwrap();
    let history_before = switch.history().len();

    let predictive_publish = predictive_evidence(292, 1);
    assert!(predictive_publish.is_predictive_only());
    assert!(!predictive_publish.can_drive_active_stats_version_transition());
    assert!(!predictive_publish.advisory_evidence_can_drive_active_stats_version_transition());
    assert!(
        predictive_publish.advisory_evidence_reference().is_some(),
        "predictive evidence must carry explicit advisory identity when referenced"
    );
    let err = switch
        .publish_validated_candidate(
            TraceId::new(92),
            predictive_publish,
            "predictive evidence alone attempted to publish active StatsVersion",
        )
        .unwrap_err();
    assert_eq!(
        err,
        StatsPublicationSwitchError::PredictiveEvidenceCannotDriveActiveStatsVersion
    );
    assert!(switch.active().is_none());
    assert_eq!(
        switch.pending_state(),
        Some(StatsPublicationCandidateState::Validated)
    );
    assert_eq!(switch.history().len(), history_before);

    let published = switch
        .publish_validated_candidate(
            TraceId::new(93),
            canonical_evidence(293),
            "canonical validation evidence published active StatsVersion",
        )
        .unwrap();
    assert_eq!(
        published.active_after.unwrap().version,
        StatsVersion::new(1)
    );

    publish(&mut switch, publication(2, 20), 300);
    let history_before = switch.history().len();
    let predictive_rollback = predictive_evidence(294, 2);
    assert!(predictive_rollback.is_predictive_only());
    assert!(!predictive_rollback.can_drive_active_stats_version_transition());
    assert!(!predictive_rollback.advisory_evidence_can_drive_active_stats_version_transition());
    assert!(
        predictive_rollback.advisory_evidence_reference().is_some(),
        "predictive evidence must carry explicit advisory identity when referenced"
    );
    let err = switch
        .rollback_last_publish(
            TraceId::new(94),
            predictive_rollback,
            "predictive evidence alone attempted rollback",
        )
        .unwrap_err();
    assert_eq!(
        err,
        StatsPublicationSwitchError::PredictiveEvidenceCannotDriveActiveStatsVersion
    );
    assert_eq!(switch.active().unwrap().version(), StatsVersion::new(2));
    assert_eq!(switch.history().len(), history_before);

    let rollback = switch
        .rollback_last_publish(
            TraceId::new(95),
            recovery_evidence(295),
            "recovery review evidence restored previous active StatsVersion",
        )
        .unwrap();
    assert_eq!(rollback.active_after.unwrap().version, StatsVersion::new(1));
}
