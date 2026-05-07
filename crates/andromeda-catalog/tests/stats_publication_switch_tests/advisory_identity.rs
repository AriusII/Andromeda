use super::common::*;

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
        Some(StatsPublicationCandidateState::Validated)
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
        StatsPublicationDecisionEvidence::canonical_validation_with_advisory_identity(
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
        StatsPublicationDecisionEvidence::canonical_validation_with_advisory_identity(
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
