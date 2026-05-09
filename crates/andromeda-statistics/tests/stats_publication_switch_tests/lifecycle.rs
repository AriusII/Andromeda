use super::common::*;

#[test]
fn active_publication_requires_validated_candidate_switch() {
    let publication = publication(1, 10);
    let mut switch = StatsPublicationSwitch::new();

    let staged = switch
        .stage_candidate(
            publication,
            TraceId::new(1),
            "candidate prepared from completed stats collection",
        )
        .unwrap();

    assert_eq!(staged.stage, StatsPublicationDecisionStage::Candidate);
    assert_eq!(
        staged.selected_decision,
        StatsPublicationSwitchDecision::KeepActive
    );
    assert!(
        staged
            .reason
            .starts_with("candidate prepared from completed stats collection")
    );
    assert!(staged.reason.contains("stage=Candidate"));
    assert!(staged.reason.contains("selected_decision=KeepActive"));
    assert!(staged.reason.contains("accepted=true"));
    assert!(staged.reason.contains("active_before=none"));
    assert!(
        staged
            .reason
            .contains("candidate=catalog_version:2,version:1")
    );
    assert!(!staged.active_changed());
    assert!(switch.active().is_none());
    assert_eq!(
        switch.pending_state(),
        Some(StatsPublicationCandidateState::Staged)
    );

    let publish_error = switch
        .publish_validated_candidate(
            TraceId::new(2),
            canonical_evidence(200),
            "attempted active switch before validation",
        )
        .unwrap_err();
    assert_eq!(
        publish_error,
        StatsPublicationSwitchError::PendingCandidateNotValidated
    );
    assert!(switch.active().is_none());

    let validation = switch
        .validate_candidate(TraceId::new(3), "candidate passed canonical validation")
        .unwrap();
    assert!(validation.accepted);
    assert_eq!(validation.stage, StatsPublicationDecisionStage::Validate);
    assert_eq!(
        switch.pending_state(),
        Some(StatsPublicationCandidateState::Validated)
    );
    assert!(switch.active().is_none());

    let published = switch
        .publish_validated_candidate(
            TraceId::new(4),
            canonical_evidence(204),
            "validated candidate selected for active publication",
        )
        .unwrap();
    assert_eq!(published.stage, StatsPublicationDecisionStage::Publish);
    assert_eq!(
        published.selected_decision,
        StatsPublicationSwitchDecision::PublishCandidate
    );
    assert!(published.reason.contains("stage=Publish"));
    assert!(
        published
            .reason
            .contains("selected_decision=PublishCandidate")
    );
    assert_eq!(
        published.decision_evidence.kind(),
        StatsPublicationDecisionEvidenceKind::CanonicalValidation
    );
    assert_eq!(published.decision_evidence.trace_id(), TraceId::new(204));
    assert!(
        published
            .reason
            .contains("decision_evidence=kind:CanonicalValidation,trace:204,can_drive_active:true")
    );
    assert!(published.reason.contains("advisory_can_drive_active:false"));
    assert!(published.reason.contains("accepted=true"));
    assert!(
        published
            .reason
            .contains("candidate=catalog_version:2,version:1")
    );
    assert!(
        published
            .reason
            .contains("active_after=catalog_version:2,version:1")
    );
    assert!(published.active_changed());
    assert_eq!(
        switch.active().unwrap().catalog_version(),
        CatalogVersion::new(2)
    );
    assert_eq!(switch.active().unwrap().version(), StatsVersion::new(1));
    assert_eq!(switch.history().len(), 3);
}

#[test]
fn stale_candidate_is_rejected_without_changing_active_publication() {
    let mut switch = StatsPublicationSwitch::new();
    publish(&mut switch, publication(2, 20), 10);
    let active_before = switch.active().unwrap().summary();

    switch
        .stage_candidate(
            publication(1, 10),
            TraceId::new(20),
            "older candidate prepared after active stats advanced",
        )
        .unwrap();
    let validation = switch
        .validate_candidate(
            TraceId::new(21),
            "candidate rejected because active StatsVersion already advanced",
        )
        .unwrap();

    assert_eq!(validation.stage, StatsPublicationDecisionStage::Validate);
    assert!(!validation.accepted);
    assert_eq!(
        validation.selected_decision,
        StatsPublicationSwitchDecision::KeepActive
    );
    assert!(
        validation
            .reason
            .contains("must advance active StatsVersion")
    );
    assert!(switch.pending_candidate().is_none());
    assert_eq!(switch.active().unwrap().summary(), active_before);
}

#[test]
fn explicit_reject_path_discards_candidate_and_preserves_active_publication() {
    let mut switch = StatsPublicationSwitch::new();
    publish(&mut switch, publication(1, 10), 30);
    let active_before = switch.active().unwrap().summary();

    switch
        .stage_candidate(
            publication(2, 20),
            TraceId::new(40),
            "candidate prepared for active publication review",
        )
        .unwrap();
    let rejected = switch
        .reject_candidate(
            TraceId::new(41),
            "candidate rejected by policy because sample coverage was insufficient",
        )
        .unwrap();

    assert_eq!(rejected.stage, StatsPublicationDecisionStage::Reject);
    assert!(!rejected.accepted);
    assert_eq!(
        rejected.selected_decision,
        StatsPublicationSwitchDecision::KeepActive
    );
    assert!(rejected.reason.contains("stage=Reject"));
    assert!(rejected.reason.contains("accepted=false"));
    assert!(
        rejected
            .reason
            .contains("active_before=catalog_version:2,version:1")
    );
    assert!(
        rejected
            .reason
            .contains("active_after=catalog_version:2,version:1")
    );
    assert!(!rejected.active_changed());
    assert!(switch.pending_candidate().is_none());
    assert_eq!(switch.active().unwrap().summary(), active_before);
}

#[test]
fn rollback_restores_previous_publication_with_trace() {
    let mut switch = StatsPublicationSwitch::new();
    publish(&mut switch, publication(1, 10), 50);
    publish(&mut switch, publication(2, 20), 60);

    assert_eq!(switch.active().unwrap().version(), StatsVersion::new(2));
    let rollback = switch
        .rollback_last_publish(
            TraceId::new(70),
            recovery_evidence(270),
            "operator restored previous active StatsVersion after publication review",
        )
        .unwrap();

    assert_eq!(rollback.stage, StatsPublicationDecisionStage::Rollback);
    assert_eq!(
        rollback.selected_decision,
        StatsPublicationSwitchDecision::RestorePreviousPublication
    );
    assert_eq!(
        rollback.decision_evidence.kind(),
        StatsPublicationDecisionEvidenceKind::RecoveryReview
    );
    assert_eq!(rollback.decision_evidence.trace_id(), TraceId::new(270));
    assert!(rollback.active_changed());
    assert_eq!(
        rollback.active_before.unwrap().version,
        StatsVersion::new(2)
    );
    assert_eq!(rollback.active_after.unwrap().version, StatsVersion::new(1));
    assert_eq!(switch.active().unwrap().version(), StatsVersion::new(1));
    assert!(switch.history().iter().all(|trace| trace.has_reason()));
}
