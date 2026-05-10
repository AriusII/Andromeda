use andromeda_observability::{CriticalDecisionKind, TraceId};
use andromeda_plan_cache::{
    AdvisoryEvidenceIdentity, AdvisoryEvidenceStatus, AdvisoryEvidenceSummaryBuilder,
    BoundedPlanCache, CardinalityBucket, PlanCacheKey, PlanCacheMissReason, PlanCandidate,
    PlanCandidateId, PlanCandidateRank, PlanClass, PlanDecisionOutcome, PlanShapeFingerprint,
    PlanShapeFingerprintBuilder, classify_advisory_identity_for_key,
    select_minimal_plan_with_advisory_evidence,
};
use andromeda_procedure_contract::{PolicyVersion, ProcedureContractBinding, StatsVersion};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

fn binding(
    procedure_id: u64,
    catalog_version: u64,
    contract_byte: u8,
    stats_version: u64,
    policy_byte: u8,
) -> ProcedureContractBinding {
    ProcedureContractBinding {
        procedure_id: ProcedureId::new(procedure_id),
        catalog_version: CatalogVersion::new(catalog_version),
        contract_hash: ContractHash::new([contract_byte; ContractHash::LEN]),
        stats_version: StatsVersion::new(stats_version),
        policy_version: PolicyVersion::new([policy_byte; PolicyVersion::LEN]),
    }
}

fn shaped_fingerprint() -> PlanShapeFingerprint {
    PlanShapeFingerprintBuilder::new()
        .push_parameter(0x01, false, 0)
        .push_cardinality(0, CardinalityBucket::classify(4_097))
        .finish()
}

fn candidate(id: u64, plan_class: PlanClass, rank: u16, digest_byte: u8) -> PlanCandidate {
    PlanCandidate::new(
        PlanCandidateId::new(id).expect("test plan candidate id must be non-zero"),
        plan_class,
        PlanCandidateRank::from_permille(rank).expect("test plan rank must be bounded"),
        [digest_byte; 32],
    )
    .expect("test plan digest must be non-zero")
}

fn advisory_identity_for_key(key: PlanCacheKey) -> AdvisoryEvidenceIdentity {
    AdvisoryEvidenceIdentity {
        procedure_id: key.procedure_id,
        catalog_version: key.catalog_version,
        stats_version: key.stats_version,
        plan_class: Some(key.plan_class),
        contract_hash: Some(key.contract_hash),
    }
}

fn advisory_summary(
    supplied_count: usize,
    accepted: &[(u16, u16, [u8; 32])],
    rejected: &[AdvisoryEvidenceStatus],
) -> andromeda_plan_cache::AdvisoryEvidenceSummary {
    let mut builder =
        AdvisoryEvidenceSummaryBuilder::new(supplied_count).expect("summary is bounded");
    for (score, confidence, digest) in accepted {
        builder.record_accepted_advisory(*score, *confidence, *digest);
    }
    for status in rejected {
        builder.record_rejected(*status);
    }
    builder.finish()
}

#[test]
fn versioned_plan_key_decision_trace_and_staleness_contract() {
    let fingerprint = shaped_fingerprint();
    let base_key = PlanCacheKey::build(
        binding(4_001, 12, 0xA1, 7, 0xB1),
        PlanClass::StatsAdaptive,
        fingerprint,
    )
    .expect("base PlanCacheKey must bind non-zero contract, catalog, stats, and policy versions");
    let rebuilt_base_key = PlanCacheKey::build(
        binding(4_001, 12, 0xA1, 7, 0xB1),
        PlanClass::StatsAdaptive,
        fingerprint,
    )
    .expect("same versioned binding must rebuild the same PlanCacheKey");
    assert_eq!(base_key, rebuilt_base_key);
    assert_eq!(base_key.digest(), rebuilt_base_key.digest());

    let bumped_stats_key = PlanCacheKey::build(
        binding(4_001, 12, 0xA1, 8, 0xB1),
        PlanClass::StatsAdaptive,
        fingerprint,
    )
    .expect("bumped StatsVersion must still build a valid PlanCacheKey");
    assert_ne!(base_key, bumped_stats_key);
    assert_ne!(
        base_key.digest(),
        bumped_stats_key.digest(),
        "StatsVersion participates in the v0 key digest"
    );

    let candidates = [
        candidate(301, PlanClass::StatsAdaptive, 100, 0x31),
        candidate(302, PlanClass::StatsAdaptive, 900, 0x32),
    ];
    let base_selection = select_minimal_plan_with_advisory_evidence(
        base_key,
        &candidates,
        advisory_summary(1, &[(900, 850, [0x51; 32])], &[]),
        TraceId::new(44_001),
    )
    .expect("fresh advisory evidence must not block deterministic selection");
    assert_eq!(
        base_selection.selected().plan_id(),
        PlanCandidateId::new(301).expect("test plan candidate id must be non-zero"),
        "static bounded rank remains the selector authority"
    );
    assert_eq!(
        base_selection.trace().advisory_evidence().accepted_count(),
        1
    );

    let selection_trace = base_selection.trace().as_decision_trace();
    assert_eq!(
        selection_trace.decision,
        CriticalDecisionKind::PlanSelection
    );
    assert!(selection_trace.has_explanation());
    assert!(selection_trace.reason.contains("outcome=selected"));
    assert!(
        selection_trace
            .reason
            .contains("version_binding=ContractHash+CatalogVersion+StatsVersion")
    );
    assert!(selection_trace.reason.contains("procedure_id=4001"));
    assert!(selection_trace.reason.contains("catalog_version=12"));
    assert!(selection_trace.reason.contains("stats_version=7"));
    assert!(selection_trace.reason.contains("plan_class=StatsAdaptive"));
    assert!(
        selection_trace
            .reason
            .contains("scenario_evidence_supplied=1")
    );
    assert!(selection_trace.reason.contains("accepted-advisory:1"));
    assert!(selection_trace.reason.contains("advisory_only=true"));

    let mut cache = BoundedPlanCache::new(2).expect("test cache capacity must be bounded");
    let insert = cache
        .insert_selection(&base_selection, TraceId::new(44_002))
        .expect("selected plan can be admitted into the bounded cache");
    assert_eq!(
        insert.insert_trace().outcome(),
        PlanDecisionOutcome::CacheInsert
    );
    assert_eq!(insert.inserted().key(), base_key);
    assert_eq!(cache.len(), 1);

    let hit = cache
        .lookup(base_key, TraceId::new(44_003))
        .expect("same complete versioned key should produce observable hit evidence");
    assert!(hit.is_hit());
    assert_eq!(hit.trace().outcome(), PlanDecisionOutcome::CacheHit);
    assert_eq!(hit.trace().cache_miss_reason(), None);
    let hit_trace = hit.trace().as_decision_trace();
    assert!(hit_trace.reason.contains("outcome=cache-hit"));
    assert!(hit_trace.reason.contains("cache_miss_reason=none"));
    assert!(hit_trace.reason.contains("stats_version=7"));
    assert!(hit_trace.reason.contains("policy_version="));

    let miss = cache
        .lookup(bumped_stats_key, TraceId::new(44_004))
        .expect("changed StatsVersion should produce observable miss evidence");
    assert!(!miss.is_hit());
    assert_eq!(miss.trace().outcome(), PlanDecisionOutcome::CacheMiss);
    assert_eq!(
        miss.trace().cache_miss_reason(),
        Some(PlanCacheMissReason::StatsVersionMismatch)
    );
    let miss_trace = miss.trace().as_decision_trace();
    assert_eq!(miss_trace.decision, CriticalDecisionKind::PlanSelection);
    assert!(miss_trace.reason.contains("outcome=cache-miss"));
    assert!(miss_trace.reason.contains("stats_version=8"));
    assert!(
        miss_trace
            .reason
            .contains("cache_miss_reason=stats-version-mismatch")
    );

    let stale_identity = advisory_identity_for_key(base_key);
    assert_eq!(
        classify_advisory_identity_for_key(&bumped_stats_key, stale_identity, false, false),
        AdvisoryEvidenceStatus::StatsVersionMismatch
    );
    let refreshed_selection = select_minimal_plan_with_advisory_evidence(
        bumped_stats_key,
        &candidates,
        advisory_summary(1, &[], &[AdvisoryEvidenceStatus::StatsVersionMismatch]),
        TraceId::new(44_005),
    )
    .expect("stale advisory evidence is rejected from evidence use, not made authoritative");
    assert_eq!(
        refreshed_selection.selected().plan_id(),
        PlanCandidateId::new(301).expect("test plan candidate id must be non-zero")
    );
    assert_eq!(
        refreshed_selection
            .trace()
            .advisory_evidence()
            .accepted_count(),
        0
    );
    assert_eq!(
        refreshed_selection
            .trace()
            .advisory_evidence()
            .rejected_count(),
        1
    );
    assert_eq!(
        refreshed_selection
            .trace()
            .advisory_evidence()
            .status_count(AdvisoryEvidenceStatus::StatsVersionMismatch),
        1
    );
    let refreshed_trace = refreshed_selection.trace().as_decision_trace();
    assert!(
        refreshed_trace
            .reason
            .contains("scenario_evidence_rejected=1")
    );
    assert!(refreshed_trace.reason.contains("stats-version-mismatch:1"));
    assert!(refreshed_trace.reason.contains("advisory_only=true"));

    cache
        .insert_selection(&refreshed_selection, TraceId::new(44_006))
        .expect("fresh selection for the new version may occupy a separate cache slot");
    assert_eq!(
        cache.len(),
        2,
        "the in-memory cache separates versioned keys instead of treating old slots as durable truth"
    );
    assert!(
        cache
            .lookup(base_key, TraceId::new(44_007))
            .expect("old versioned key remains independently addressable")
            .is_hit()
    );
    assert!(
        cache
            .lookup(bumped_stats_key, TraceId::new(44_008))
            .expect("new versioned key becomes addressable only after fresh selection")
            .is_hit()
    );
}
