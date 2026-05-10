pub(crate) use andromeda_observability::TraceId;
pub(crate) use andromeda_procedure_contract::StatsVersion;
pub(crate) use andromeda_scenario_evidence::{EvidenceConfidence, EvidenceScore};
pub(crate) use andromeda_scenario_evidence::{
    ScenarioEvidence, ScenarioEvidenceAdvisoryUse, ScenarioId, ScenarioKind, ScenarioTarget,
    ValidityWindow,
};
pub(crate) use andromeda_statistics::{
    HistogramBucket, HistogramPlaceholder, STATS_PUBLICATION_SWITCH_HISTORY_LIMIT,
    STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES, SkewMarker, StatsColumnTarget, StatsPublication,
    StatsPublicationAdvisoryEvidenceReference, StatsPublicationBuilder,
    StatsPublicationCandidateState, StatsPublicationDecisionEvidence,
    StatsPublicationDecisionEvidenceKind, StatsPublicationDecisionStage, StatsPublicationSwitch,
    StatsPublicationSwitchDecision, StatsPublicationSwitchError,
};
pub(crate) use andromeda_time::EngineTimestamp;
pub(crate) use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash, ProcedureId};

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

pub(crate) fn publication(stats_version: u64, object: u64) -> StatsPublication {
    publication_for_catalog(2, stats_version, object)
}

fn publication_for_catalog(
    catalog_version: u64,
    stats_version: u64,
    object: u64,
) -> StatsPublication {
    StatsPublicationBuilder::new(
        CatalogVersion::new(catalog_version),
        StatsVersion::new(stats_version),
    )
    .unwrap()
    .push(target(object, 1), histogram())
    .unwrap()
    .finish()
}

pub(crate) fn canonical_evidence(trace_id: u128) -> StatsPublicationDecisionEvidence {
    StatsPublicationDecisionEvidence::canonical_validation(TraceId::new(trace_id)).unwrap()
}

pub(crate) fn recovery_evidence(trace_id: u128) -> StatsPublicationDecisionEvidence {
    StatsPublicationDecisionEvidence::recovery_review(TraceId::new(trace_id)).unwrap()
}

pub(crate) fn advisory_use_for_stats(
    stats_version: u64,
    scenario_id: u64,
) -> ScenarioEvidenceAdvisoryUse {
    let evidence = ScenarioEvidence::new(
        ScenarioId::new(scenario_id).unwrap(),
        ScenarioKind::RegressionProbe,
        ScenarioTarget {
            procedure_id: ProcedureId::new(7),
            catalog_version: CatalogVersion::new(2),
            stats_version: StatsVersion::new(stats_version),
            plan_class: None,
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

pub(crate) fn predictive_evidence(
    trace_id: u128,
    stats_version: u64,
) -> StatsPublicationDecisionEvidence {
    StatsPublicationDecisionEvidence::predictive_scenario_evidence(
        TraceId::new(trace_id),
        advisory_use_for_stats(stats_version, trace_id as u64),
    )
    .unwrap()
}

pub(crate) fn canonical_advisory_evidence(
    trace_id: u128,
    stats_version: u64,
    scenario_id: u64,
) -> StatsPublicationDecisionEvidence {
    StatsPublicationDecisionEvidence::canonical_validation_with_advisory_reference(
        TraceId::new(trace_id),
        advisory_use_for_stats(stats_version, scenario_id),
    )
    .unwrap()
}

pub(crate) fn recovery_advisory_evidence(
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

pub(crate) fn publish(
    switch: &mut StatsPublicationSwitch,
    publication: StatsPublication,
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
fn publication_digest_and_summary_are_bound_to_stats_version() {
    let v21 = publication(21, 90);
    let v21_again = publication(21, 90);
    let v22_same_histogram = publication(22, 90);

    assert_eq!(v21.digest(), v21_again.digest());
    assert_ne!(
        v21.digest(),
        v22_same_histogram.digest(),
        "StatsVersion must participate in the published statistics digest"
    );

    let summary = v21.summary();
    assert_eq!(summary.catalog_version, CatalogVersion::new(2));
    assert_eq!(summary.version, StatsVersion::new(21));
    assert_eq!(summary.digest, v21.digest());
    assert_eq!(summary.entry_count, 1);
}

#[test]
fn switch_publishes_successor_stats_version_with_traceable_digest() {
    let mut switch = StatsPublicationSwitch::new();
    let first = publication(31, 91);
    let first_summary = first.summary();
    publish(&mut switch, first, 4_000);

    let successor = publication(32, 91);
    let successor_summary = successor.summary();
    assert_ne!(
        first_summary.digest, successor_summary.digest,
        "successor StatsVersion should carry a distinct publication digest"
    );

    let staged = switch
        .stage_candidate(
            successor,
            TraceId::new(4_100),
            "successor statistics candidate staged after collection",
        )
        .unwrap();
    assert_eq!(staged.active_before, Some(first_summary));
    assert_eq!(staged.candidate, Some(successor_summary));
    assert_eq!(staged.active_after, Some(first_summary));
    assert!(!staged.active_changed());
    assert_eq!(switch.active().unwrap().summary(), first_summary);

    let validated = switch
        .validate_candidate(
            TraceId::new(4_101),
            "successor statistics candidate passed validation",
        )
        .unwrap();
    assert_eq!(validated.active_after, Some(first_summary));
    assert_eq!(switch.active().unwrap().summary(), first_summary);

    let published = switch
        .publish_validated_candidate(
            TraceId::new(4_102),
            canonical_evidence(4_202),
            "successor StatsVersion published after canonical validation",
        )
        .unwrap();

    assert_eq!(published.active_before, Some(first_summary));
    assert_eq!(published.candidate, Some(successor_summary));
    assert_eq!(published.active_after, Some(successor_summary));
    assert!(published.active_changed());
    assert_eq!(switch.active().unwrap().summary(), successor_summary);
    assert!(
        published
            .reason
            .contains("active_before=catalog_version:2,version:31")
    );
    assert!(
        published
            .reason
            .contains("candidate=catalog_version:2,version:32")
    );
    assert!(
        published
            .reason
            .contains("active_after=catalog_version:2,version:32")
    );
    assert!(
        published
            .reason
            .contains("decision_evidence=kind:CanonicalValidation")
    );
}
