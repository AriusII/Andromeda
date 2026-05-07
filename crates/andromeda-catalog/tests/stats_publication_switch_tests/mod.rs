pub(crate) use andromeda_catalog::{
    EvidenceConfidence, EvidenceScore, HistogramBucket, HistogramPlaceholder, PlanClass,
    STATS_PUBLICATION_SWITCH_HISTORY_LIMIT, STATS_PUBLICATION_SWITCH_REASON_MAX_BYTES,
    ScenarioEvidence, ScenarioEvidenceAdvisoryUse, ScenarioId, ScenarioKind, ScenarioTarget,
    SkewMarker, StatsColumnTarget, StatsPublication, StatsPublicationAdvisoryEvidenceReference,
    StatsPublicationBuilder, StatsPublicationCandidateState, StatsPublicationDecisionEvidence,
    StatsPublicationDecisionEvidenceKind, StatsPublicationDecisionStage, StatsPublicationSwitch,
    StatsPublicationSwitchDecision, StatsPublicationSwitchError, StatsVersion, ValidityWindow,
};
pub(crate) use andromeda_core::{
    CatalogObjectId, CatalogVersion, ContractHash, EngineTimestamp, ProcedureId,
};
pub(crate) use andromeda_observe::TraceId;

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
