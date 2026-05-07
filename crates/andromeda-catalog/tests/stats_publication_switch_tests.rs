use andromeda_catalog::{
    EvidenceConfidence, EvidenceScore, HistogramBucket, HistogramPlaceholder, PlanClass,
    STATS_PUBLICATION_SWITCH_HISTORY_LIMIT, STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES,
    ScenarioEvidence, ScenarioEvidenceAdvisoryUse, ScenarioId, ScenarioKind, ScenarioTarget,
    SkewMarker, StatsColumnTarget, StatsPublicationAdvisoryEvidenceReference,
    StatsPublicationBuilder, StatsPublicationDecisionEvidence,
    StatsPublicationDecisionEvidenceKind, StatsPublicationDecisionStage, StatsPublicationSwitch,
    StatsPublicationSwitchDecision, StatsPublicationSwitchError, StatsVersion, ValidityWindow,
};
use andromeda_core::{CatalogObjectId, CatalogVersion, ContractHash, EngineTimestamp, ProcedureId};
use andromeda_observe::TraceId;

fn target(object: u64, column: u16) -> StatsColumnTarget {
    StatsColumnTarget::new(CatalogObjectId::new(object), column)
}

fn bucket(lo: u64, hi: u64, rows: u64, distinct: u64) -> HistogramBucket {
    HistogramBucket {
        lower_inclusive: lo,
        upper_inclusive: hi,
        row_estimate: rows,
        distinct_estimate: distinct,
    }
}

fn histogram() -> HistogramPlaceholder {
    HistogramPlaceholder::new(
        vec![bucket(0, 9, 50, 10), bucket(10, 19, 40, 10)],
        SkewMarker::LowSkew,
    )
    .expect("test histogram is valid")
}

fn publication(stats_version: u64, object: u64) -> andromeda_catalog::StatsPublication {
    publication_for_catalog(2, stats_version, object)
}

fn publication_for_catalog(
    catalog_version: u64,
    stats_version: u64,
    object: u64,
) -> andromeda_catalog::StatsPublication {
    StatsPublicationBuilder::new(
        CatalogVersion::new(catalog_version),
        StatsVersion::new(stats_version),
    )
    .unwrap()
    .push(target(object, 1), histogram())
    .unwrap()
    .finish()
}

fn canonical_evidence(trace_id: u128) -> StatsPublicationDecisionEvidence {
    StatsPublicationDecisionEvidence::canonical_validation(TraceId::new(trace_id)).unwrap()
}

fn recovery_evidence(trace_id: u128) -> StatsPublicationDecisionEvidence {
    StatsPublicationDecisionEvidence::recovery_review(TraceId::new(trace_id)).unwrap()
}

fn advisory_use_for_stats(stats_version: u64, scenario_id: u64) -> ScenarioEvidenceAdvisoryUse {
    let evidence = ScenarioEvidence::new(
        ScenarioId::new(scenario_id).unwrap(),
        ScenarioKind::RegressionProbe,
        ScenarioTarget {
            procedure_id: ProcedureId::new(7),
            catalog_version: CatalogVersion::new(2),
            stats_version: StatsVersion::new(stats_version),
            plan_class: Some(PlanClass::Cardinality),
            contract_hash: Some(ContractHash::new([0xAA; ContractHash::LEN])),
        },
        EvidenceScore::from_permille(700).unwrap(),
        EvidenceConfidence::from_permille(850).unwrap(),
        ValidityWindow::new(
            EngineTimestamp::from_unix_millis(100),
            EngineTimestamp::from_unix_millis(200),
        )
        .unwrap(),
    )
    .unwrap();
    let advisory = evidence
        .advisory_use_at(EngineTimestamp::from_unix_millis(150))
        .unwrap();
    assert!(!advisory.can_drive_active_stats_version_transition());
    advisory
}

fn predictive_evidence(trace_id: u128, stats_version: u64) -> StatsPublicationDecisionEvidence {
    StatsPublicationDecisionEvidence::predictive_scenario_evidence(
        TraceId::new(trace_id),
        advisory_use_for_stats(stats_version, trace_id as u64),
    )
    .unwrap()
}

fn canonical_advisory_evidence(
    trace_id: u128,
    stats_version: u64,
    scenario_id: u64,
) -> StatsPublicationDecisionEvidence {
    StatsPublicationDecisionEvidence::new_with_advisory_reference(
        StatsPublicationDecisionEvidenceKind::CanonicalValidation,
        TraceId::new(trace_id),
        advisory_use_for_stats(stats_version, scenario_id),
    )
    .unwrap()
}

fn recovery_advisory_evidence(
    trace_id: u128,
    stats_version: u64,
    scenario_id: u64,
) -> StatsPublicationDecisionEvidence {
    StatsPublicationDecisionEvidence::recovery_review_with_advisory_reference(
        TraceId::new(trace_id),
        advisory_use_for_stats(stats_version, scenario_id),
    )
    .unwrap()
}

fn publish(
    switch: &mut StatsPublicationSwitch,
    publication: andromeda_catalog::StatsPublication,
    trace_base: u128,
) {
    switch
        .stage_candidate(
            publication,
            TraceId::new(trace_base),
            "candidate prepared by statistics builder",
        )
        .unwrap();
    let validation = switch
        .validate_candidate(
            TraceId::new(trace_base + 1),
            "candidate validated before publication switch",
        )
        .unwrap();
    assert!(validation.accepted);
    switch
        .publish_validated_candidate(
            TraceId::new(trace_base + 2),
            canonical_evidence(trace_base + 100),
            "validated candidate published as active StatsVersion",
        )
        .unwrap();
}

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
        Some(andromeda_catalog::StatsPublicationCandidateState::Staged)
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
        Some(andromeda_catalog::StatsPublicationCandidateState::Validated)
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
        Some(andromeda_catalog::StatsPublicationCandidateState::Validated)
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

#[test]
fn advisory_identity_is_validated_against_publication_target() {
    let mut switch = StatsPublicationSwitch::new();
    switch
        .stage_candidate(
            publication(1, 10),
            TraceId::new(700),
            "candidate staged before advisory mismatch rejection",
        )
        .unwrap();
    switch
        .validate_candidate(TraceId::new(701), "candidate passed canonical validation")
        .unwrap();
    let history_before = switch.history().len();

    let err = switch
        .publish_validated_candidate(
            TraceId::new(702),
            canonical_advisory_evidence(902, 99, 44),
            "advisory identity targets the wrong StatsVersion",
        )
        .unwrap_err();
    assert_eq!(
        err,
        StatsPublicationSwitchError::AdvisoryEvidenceStatsVersionMismatch {
            expected: StatsVersion::new(1),
            actual: StatsVersion::new(99)
        }
    );
    assert!(switch.active().is_none());
    assert_eq!(
        switch.pending_state(),
        Some(andromeda_catalog::StatsPublicationCandidateState::Validated)
    );
    assert_eq!(switch.history().len(), history_before);

    let published = switch
        .publish_validated_candidate(
            TraceId::new(703),
            canonical_advisory_evidence(903, 1, 45),
            "canonical validation published with advisory identity attached",
        )
        .unwrap();
    assert!(published.reason.contains("advisory:scenario:45"));
    assert!(
        published
            .reason
            .contains("target:procedure:7,catalog:2,stats:1")
    );
    assert!(published.reason.contains("digest:"));
    assert_eq!(
        published
            .decision_evidence
            .advisory_evidence_reference()
            .unwrap()
            .target()
            .stats_version,
        StatsVersion::new(1)
    );
}

#[test]
fn advisory_identity_rejects_digest_and_target_mismatch() {
    let advisory_use = advisory_use_for_stats(1, 60);
    assert!(!advisory_use.can_drive_active_stats_version_transition());

    let zero_digest = StatsPublicationAdvisoryEvidenceReference::new(
        advisory_use.scenario_id(),
        [0u8; 32],
        advisory_use.target(),
    )
    .unwrap_err();
    assert_eq!(
        zero_digest,
        StatsPublicationSwitchError::AdvisoryEvidenceDigestZero
    );

    let mut mismatched_digest = advisory_use.digest();
    mismatched_digest[0] ^= 0xFF;
    let digest_mismatch = StatsPublicationAdvisoryEvidenceReference::new(
        advisory_use.scenario_id(),
        mismatched_digest,
        advisory_use.target(),
    )
    .unwrap();
    assert_eq!(
        StatsPublicationDecisionEvidence::new_with_advisory_identity(
            StatsPublicationDecisionEvidenceKind::CanonicalValidation,
            TraceId::new(930),
            advisory_use,
            digest_mismatch,
        )
        .unwrap_err(),
        StatsPublicationSwitchError::AdvisoryEvidenceIdentityMismatch
    );

    let mut mismatched_target = advisory_use.target();
    mismatched_target.stats_version = StatsVersion::new(2);
    let target_mismatch = StatsPublicationAdvisoryEvidenceReference::new(
        advisory_use.scenario_id(),
        advisory_use.digest(),
        mismatched_target,
    )
    .unwrap();
    assert_eq!(
        StatsPublicationDecisionEvidence::new_with_advisory_identity(
            StatsPublicationDecisionEvidenceKind::CanonicalValidation,
            TraceId::new(931),
            advisory_use,
            target_mismatch,
        )
        .unwrap_err(),
        StatsPublicationSwitchError::AdvisoryEvidenceIdentityMismatch
    );
}

#[test]
fn rollback_advisory_identity_targets_current_active_publication() {
    let mut switch = StatsPublicationSwitch::new();
    publish(&mut switch, publication(1, 10), 800);
    publish(&mut switch, publication(2, 20), 810);
    let history_before = switch.history().len();

    let err = switch
        .rollback_last_publish(
            TraceId::new(820),
            recovery_advisory_evidence(920, 1, 55),
            "rollback advisory identity targeted the previous StatsVersion",
        )
        .unwrap_err();
    assert_eq!(
        err,
        StatsPublicationSwitchError::AdvisoryEvidenceStatsVersionMismatch {
            expected: StatsVersion::new(2),
            actual: StatsVersion::new(1)
        }
    );
    assert_eq!(switch.active().unwrap().version(), StatsVersion::new(2));
    assert_eq!(switch.history().len(), history_before);

    let rollback = switch
        .rollback_last_publish(
            TraceId::new(821),
            recovery_advisory_evidence(921, 2, 56),
            "recovery review restored previous active StatsVersion with advisory identity",
        )
        .unwrap();
    assert_eq!(rollback.active_after.unwrap().version, StatsVersion::new(1));
    assert!(rollback.reason.contains("advisory:scenario:56"));
    assert!(
        rollback
            .reason
            .contains("target:procedure:7,catalog:2,stats:2")
    );
}

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
        Some(andromeda_catalog::StatsPublicationCandidateState::Validated)
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
