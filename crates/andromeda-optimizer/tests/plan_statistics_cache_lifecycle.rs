#![forbid(unsafe_code)]

use andromeda_contract::{PolicyVersion, ProcedureContractBinding, StatsVersion};
use andromeda_decision_trace::DecisionTraceId;
use andromeda_optimizer::{OptimizerPlanReason, OptimizerPolicy, evaluate_optimizer_plan_inputs};
use andromeda_plan_cache::{
    PlanCacheKey, PlanCachePolicy, PlanCacheReuseReason, PlanClass, PlanShapeFingerprint,
    evaluate_plan_cache_reuse,
};
use andromeda_statistics::{
    StatisticsUsePolicy, StatisticsUseReason, StatsObjectDescriptor, StatsPublicationState,
    StatsSetDigest, evaluate_statistics_for_optimizer,
};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

fn policy_version(byte: u8) -> PolicyVersion {
    PolicyVersion::new([byte; PolicyVersion::LEN])
}

fn binding(
    procedure_id: u64,
    catalog_version: u64,
    contract_hash_byte: u8,
    stats_version: u64,
    policy_version: PolicyVersion,
) -> ProcedureContractBinding {
    ProcedureContractBinding {
        procedure_id: ProcedureId::new(procedure_id),
        catalog_version: CatalogVersion::new(catalog_version),
        contract_hash: ContractHash::new([contract_hash_byte; ContractHash::LEN]),
        stats_version: StatsVersion::new(stats_version),
        policy_version,
    }
}

fn key(binding: ProcedureContractBinding, plan_class: PlanClass) -> PlanCacheKey {
    PlanCacheKey::build(binding, plan_class, PlanShapeFingerprint::empty())
        .expect("singleton plan cache key should be valid")
}

fn stats(
    catalog_version: u64,
    stats_version: u64,
    state: StatsPublicationState,
) -> StatsObjectDescriptor {
    StatsObjectDescriptor::new(
        CatalogVersion::new(catalog_version),
        StatsVersion::new(stats_version),
        StatsSetDigest::new([0x5A; StatsSetDigest::LEN]).expect("digest must be non-zero"),
        state,
    )
    .expect("statistics descriptor should be valid")
}

#[test]
fn optimizer_statistics_and_plan_cache_share_version_bound_identity() {
    let policy = policy_version(0xB7);
    let binding = binding(77, 9, 0xA1, 4, policy);
    let key = key(binding, PlanClass::Singleton);
    let descriptor = stats(9, 4, StatsPublicationState::Published);

    let cache_decision = evaluate_plan_cache_reuse(
        key,
        PlanCachePolicy::enabled(8, policy).expect("bounded cache policy"),
        DecisionTraceId::new(101).expect("trace id"),
    )
    .expect("plan-cache reuse decision should evaluate");
    assert!(cache_decision.may_reuse());
    assert_eq!(cache_decision.reason(), PlanCacheReuseReason::ReuseAllowed);
    assert!(cache_decision.trace().is_versioned());

    let statistics_decision = evaluate_statistics_for_optimizer(
        Some(descriptor),
        StatisticsUsePolicy::enabled(policy),
        DecisionTraceId::new(102).expect("trace id"),
    )
    .expect("statistics use decision should evaluate");
    assert!(statistics_decision.accepted());
    assert_eq!(
        statistics_decision.reason(),
        StatisticsUseReason::AcceptedPublished
    );
    assert!(statistics_decision.trace().is_versioned());

    let optimizer_decision = evaluate_optimizer_plan_inputs(
        key,
        statistics_decision.descriptor(),
        OptimizerPolicy::adaptive_enabled(policy),
        DecisionTraceId::new(103).expect("trace id"),
    )
    .expect("optimizer decision should evaluate");
    assert!(optimizer_decision.uses_statistics());
    assert!(!optimizer_decision.bounded_fallback());
    assert_eq!(
        optimizer_decision.reason(),
        OptimizerPlanReason::StatisticsAccepted
    );
    assert!(optimizer_decision.trace().is_observable());
    assert!(optimizer_decision.trace().is_versioned());
    assert_eq!(optimizer_decision.trace().evidence().len(), 2);
}

#[test]
fn lifecycle_drift_changes_keys_and_forces_observable_optimizer_fallbacks() {
    let policy = policy_version(0xB8);
    let base = binding(88, 10, 0xA2, 5, policy);
    let base_key = key(base, PlanClass::Singleton);

    let stale_catalog_key = key(binding(88, 11, 0xA2, 5, policy), PlanClass::Singleton);
    let stale_contract_key = key(binding(88, 10, 0xA3, 5, policy), PlanClass::Singleton);
    let stale_stats_key = key(binding(88, 10, 0xA2, 6, policy), PlanClass::Singleton);
    let stale_policy_key = key(
        binding(88, 10, 0xA2, 5, policy_version(0xB9)),
        PlanClass::Singleton,
    );

    assert_ne!(base_key.digest(), stale_catalog_key.digest());
    assert_ne!(base_key.digest(), stale_contract_key.digest());
    assert_ne!(base_key.digest(), stale_stats_key.digest());
    assert_ne!(base_key.digest(), stale_policy_key.digest());

    let matching_stats = stats(10, 5, StatsPublicationState::Published);
    let catalog_fallback = evaluate_optimizer_plan_inputs(
        stale_catalog_key,
        Some(matching_stats),
        OptimizerPolicy::adaptive_enabled(policy),
        DecisionTraceId::new(201).expect("trace id"),
    )
    .expect("catalog drift should produce bounded fallback");
    assert_eq!(
        catalog_fallback.reason(),
        OptimizerPlanReason::StatisticsVersionMismatch
    );
    assert!(catalog_fallback.bounded_fallback());
    assert!(catalog_fallback.trace().is_versioned());

    let stats_fallback = evaluate_optimizer_plan_inputs(
        stale_stats_key,
        Some(matching_stats),
        OptimizerPolicy::adaptive_enabled(policy),
        DecisionTraceId::new(202).expect("trace id"),
    )
    .expect("stats drift should produce bounded fallback");
    assert_eq!(
        stats_fallback.reason(),
        OptimizerPlanReason::StatisticsVersionMismatch
    );
    assert!(stats_fallback.bounded_fallback());

    let policy_fallback = evaluate_optimizer_plan_inputs(
        stale_policy_key,
        Some(matching_stats),
        OptimizerPolicy::adaptive_enabled(policy),
        DecisionTraceId::new(203).expect("trace id"),
    )
    .expect("policy drift should produce bounded fallback");
    assert_eq!(
        policy_fallback.reason(),
        OptimizerPlanReason::PolicyVersionMismatch
    );
    assert!(policy_fallback.bounded_fallback());

    let rejected_stats = evaluate_statistics_for_optimizer(
        Some(stats(10, 5, StatsPublicationState::Candidate)),
        StatisticsUsePolicy::enabled(policy),
        DecisionTraceId::new(204).expect("trace id"),
    )
    .expect("candidate statistics should be classified");
    assert!(!rejected_stats.accepted());
    assert_eq!(
        rejected_stats.reason(),
        StatisticsUseReason::CandidateNotPublished
    );
    assert!(rejected_stats.trace().is_observable());
    assert_eq!(rejected_stats.trace().evidence().len(), 1);
}

#[test]
fn plan_class_participates_in_owner_key_identity() {
    let policy = policy_version(0xBA);
    let singleton = key(binding(99, 12, 0xA4, 7, policy), PlanClass::Singleton);
    let parameter_shape = PlanCacheKey::build(
        binding(99, 12, 0xA4, 7, policy),
        PlanClass::ParameterShape,
        andromeda_plan_cache::PlanShapeFingerprintBuilder::new()
            .push_parameter(0x01, false, 0)
            .finish(),
    )
    .expect("parameter-shape key should be valid with a shape fingerprint");

    assert_ne!(singleton, parameter_shape);
    assert_ne!(singleton.digest(), parameter_shape.digest());
}
