use super::*;

#[test]
fn decision_evidence_attaches_and_is_queryable_by_procedure_and_invocation() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();

    let record = decision(101, 1, 42);
    store.attach_invocation_decision(record.clone()).unwrap();

    let by_procedure = store.invocation_decisions_for(ProcedureId::new(1));
    assert_eq!(by_procedure.len(), 1);
    assert_eq!(by_procedure[0], record);

    let by_invocation: Vec<_> = store
        .invocation_decisions_for_invocation(InvocationId::new(101))
        .cloned()
        .collect();
    assert_eq!(by_invocation, vec![record]);
    assert_eq!(store.total_recorded_decisions(), 1);
}

#[test]
fn decision_evidence_rejects_unknown_procedure() {
    let mut store = ProcedureStore::new();
    let err = store
        .attach_invocation_decision(decision(101, 1, 42))
        .unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().contains("unknown procedure id"));
}

#[test]
fn decision_evidence_rejects_contract_hash_mismatch_with_binding() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();
    let err = store
        .attach_invocation_decision(decision(101, 1, 99))
        .unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().contains("contract hash"));
}

#[test]
fn decision_evidence_rejects_catalog_version_drift() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();
    let mut record = decision(101, 1, 42);
    record.binding.catalog_version = CatalogVersion::new(8);
    let err = store.attach_invocation_decision(record).unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().contains("catalog version"));
}

#[test]
fn decision_evidence_rejects_stats_or_policy_version_drift() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();

    let mut stats_drift = decision(101, 1, 42);
    stats_drift.binding.stats_version = StatsVersion::new(8);
    let err = store.attach_invocation_decision(stats_drift).unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().contains("stats version"));

    let mut policy_drift = decision(102, 1, 42);
    policy_drift.binding.policy_version = PolicyVersion::new([8; PolicyVersion::LEN]);
    let err = store.attach_invocation_decision(policy_drift).unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().contains("policy version"));
}

#[test]
fn decision_evidence_requires_non_empty_reason() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let bad = InvocationDecisionRecord::new(
        InvocationId::new(101),
        binding,
        DecisionTrace {
            trace_id: TraceId::new(1),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "   ".to_string(),
        },
    );
    let err = bad.unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().contains("non-empty reason"));
}
