//! Integration tests for the Procedure Store contract boundary.
//!
//! Validates that:
//! - A validated `ProcedureContract` can be projected into a store entry.
//! - Decision evidence (built from `andromeda-observe`'s `DecisionTrace`)
//!   can be attached to invocations and queried back.
//! - The store rejects evidence that does not match the registered binding.

use andromeda_catalog::ProcedureStore;
use andromeda_catalog_store::{CatalogObjectRef, ObjectKind, QualifiedName};
use andromeda_error::AndromedaErrorKind;
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_plan_cache::{PlanCacheKey, PlanClass, PlanShapeFingerprint};
use andromeda_procedure_contract::{
    AccessMode, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy, PolicyVersion,
    ProcedureContract, ProcedureErrorPolicy, ProtocolLayoutRef, ResultMetadataPolicy, StatsVersion,
    TransactionPolicy,
};
use andromeda_procedure_store::{
    CompletionEvidence, CompletionStatus, FeedbackId, InvocationDecisionRecord,
    InvocationRuntimeRecord, InvocationRuntimeRecordOutcome, ProcedureFeedback,
    ProcedureRegistration, ProcedureRuntimeCounters, ProcedureRuntimePlanId,
    ProcedureRuntimeStatus, ProcedureStoreEntry, ProcedureStoreEvidenceRole, RecordOutcome,
};
use andromeda_scenario_evidence::{
    EvidenceConfidence, EvidenceScore, ScenarioEvidence, ScenarioEvidenceOptimizerBoundary,
    ScenarioId, ScenarioKind, ScenarioTarget, ValidityWindow,
};
use andromeda_time::EngineTimestamp;
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash, InvocationId, ProcedureId};

fn sample_contract() -> ProcedureContract {
    let mut contract = ProcedureContract {
        object: CatalogObjectRef {
            object_id: CatalogObjectId::new(11),
            name: QualifiedName::parse("Inventory.ReserveStock").unwrap(),
            kind: ObjectKind::Procedure,
            catalog_version: CatalogVersion::new(7),
        },
        procedure_id: ProcedureId::new(1),
        contract_hash: ContractHash::test_vector(42),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(101),
            frame_envelope_hash: ContractHash::test_vector(202),
        },
        inputs: Vec::new(),
        structured_inputs: Vec::new(),
        result_streams: Vec::new(),
        required_permissions: vec!["execute_procedure".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Snapshot,
            retryable: false,
        },
        compatibility_policy: CompatibilityPolicy::ExactHash,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: Vec::new(),
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    };
    contract.contract_hash = contract.canonical_hash();
    contract
}

#[test]
fn procedure_store_registers_contract_and_records_invocation_decision() {
    let contract = sample_contract();
    let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();

    let mut store = ProcedureStore::new();
    assert_eq!(
        store.register(entry).unwrap(),
        ProcedureRegistration::Inserted
    );

    let lookup = store
        .lookup_by_name(&QualifiedName::parse("Inventory.ReserveStock").unwrap())
        .expect("registered procedure is reachable by qualified name");
    assert_eq!(lookup.procedure_id, ProcedureId::new(1));

    let evidence = InvocationDecisionRecord::new(
        InvocationId::new(909),
        lookup.binding,
        DecisionTrace {
            trace_id: TraceId::new(909),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "v0 inventory reserve-stock contract validated".to_string(),
        },
    )
    .unwrap();
    assert_eq!(evidence.evidence_role().as_tag(), 0x01);
    assert!(evidence.is_authoritative_decision());
    assert!(!evidence.is_observed_feedback());
    assert!(!evidence.can_select_plan_alone());

    store
        .attach_invocation_decision(evidence.clone())
        .expect("evidence matching the registered binding is accepted");

    let by_invocation: Vec<_> = store
        .invocation_decisions_for_invocation(InvocationId::new(909))
        .cloned()
        .collect();
    assert_eq!(by_invocation, vec![evidence]);
}

#[test]
fn procedure_store_rejects_evidence_with_mismatched_binding() {
    let contract = sample_contract();
    let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();
    let mut binding = entry.binding;
    let mut store = ProcedureStore::new();
    store.register(entry).unwrap();
    binding.contract_hash = ContractHash::test_vector(99);

    let mismatched = InvocationDecisionRecord::new(
        InvocationId::new(1),
        binding,
        DecisionTrace {
            trace_id: TraceId::new(1),
            decision: CriticalDecisionKind::ContractRejected,
            reason: "wrong contract hash".to_string(),
        },
    )
    .unwrap();

    let err = store.attach_invocation_decision(mismatched).unwrap_err();
    assert!(err.message().contains("contract hash"));
}

#[test]
fn procedure_store_rejects_invocation_decision_with_stale_stats_version() {
    let contract = sample_contract();
    let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();
    let mut binding = entry.binding;
    let mut store = ProcedureStore::new();
    store.register(entry).unwrap();
    binding.stats_version = StatsVersion::new(binding.stats_version.get() + 1);

    let stale = InvocationDecisionRecord::new(
        InvocationId::new(2),
        binding,
        DecisionTrace {
            trace_id: TraceId::new(2),
            decision: CriticalDecisionKind::ContractRejected,
            reason: "stale stats version".to_string(),
        },
    )
    .unwrap();

    let err = store.attach_invocation_decision(stale).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("stats version"));
}

#[test]
fn procedure_store_records_runtime_invocation_metrics_against_full_binding() {
    let contract = sample_contract();
    let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();
    let binding = entry.binding;
    let plan_key = PlanCacheKey::build(
        &binding,
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .expect("singleton plan key is valid");
    let plan_id = ProcedureRuntimePlanId::from_plan_cache_key(&plan_key);
    let mut store = ProcedureStore::new();
    store.register(entry).unwrap();

    let runtime = InvocationRuntimeRecord::new(
        InvocationId::new(910),
        binding,
        EngineTimestamp::from_unix_millis(2_000),
        EngineTimestamp::from_unix_millis(2_087),
        Some(plan_key),
        Some(plan_id),
        ProcedureRuntimeCounters::new(13, 4, 256, 32)
            .with_rows_returned(9)
            .with_rows_affected(4)
            .with_spill_bytes(16),
        ProcedureRuntimeStatus::Committed,
        None,
    )
    .expect("runtime evidence binds to a valid registered contract");

    store.attach_invocation_runtime(runtime.clone()).unwrap();

    let stored = store
        .invocation_runtime_for_invocation(InvocationId::new(910))
        .expect("runtime evidence is indexed by invocation id");
    assert_eq!(stored.duration_millis, 87);
    assert_eq!(stored.counters.rows_read, 13);
    assert_eq!(stored.counters.rows_returned, 9);
    assert_eq!(stored.counters.rows_written, 4);
    assert_eq!(stored.counters.rows_affected, 4);
    assert_eq!(stored.counters.wal_bytes, 256);
    assert_eq!(stored.counters.spill_bytes, 16);
    assert_eq!(stored.plan_key, Some(plan_key));
    assert_eq!(stored.plan_class(), Some(PlanClass::Singleton));
    assert_eq!(stored.plan_id, Some(plan_id));
    assert!(!stored.record_id.is_zero());
    assert_eq!(stored.evidence_role().as_tag(), 0x02);
    assert!(stored.is_observed_feedback());
    assert!(!stored.is_authoritative_decision());
    assert!(!stored.can_select_plan_alone());
    assert_eq!(stored.evidence_role().as_tag(), 0x02);
    assert!(stored.is_observed_feedback());
    assert!(!stored.is_authoritative_decision());
    assert!(!stored.can_select_plan_alone());
    assert_eq!(stored.expected_contract_hash(), contract.contract_hash);
    assert_eq!(stored.expected_catalog_version(), CatalogVersion::new(7));
    assert_eq!(stored.expected_stats_version(), StatsVersion::new(1));
    assert_eq!(stored.expected_policy_version(), binding.policy_version);
    assert_eq!(stored.binding.contract_hash, contract.contract_hash);
    assert_eq!(stored.binding.catalog_version, CatalogVersion::new(7));
    assert_eq!(stored.binding.stats_version, StatsVersion::new(1));
    assert_eq!(stored.binding.policy_version, binding.policy_version);
}

/// Public contract coverage for the passive invocation history sink: terminal
/// non-committed outcomes are retained as observed feedback and do not populate
/// authoritative decision or advisory feedback indexes.
#[test]
fn procedure_store_invocation_history_sink_records_failed_and_pre_transaction_aborted_outcomes() {
    let contract = sample_contract();
    let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();
    let binding = entry.binding;
    let plan_key = PlanCacheKey::build(
        &binding,
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .expect("singleton plan key is valid");
    let plan_id = ProcedureRuntimePlanId::from_plan_cache_key(&plan_key);
    let mut store = ProcedureStore::new();
    store.register(entry).unwrap();

    let failed = InvocationRuntimeRecord::new_with_decision_trace(
        InvocationId::new(911),
        binding,
        EngineTimestamp::from_unix_millis(3_000),
        EngineTimestamp::from_unix_millis(3_021),
        TraceId::new(9_111),
        Some(plan_key),
        Some(plan_id),
        ProcedureRuntimeCounters::new(5, 0, 0, 0).with_rows_returned(2),
        ProcedureRuntimeStatus::Failed,
        Some(AndromedaErrorKind::Execution),
    )
    .expect("failed terminal evidence is accepted by the public constructor");
    let aborted = InvocationRuntimeRecord::new(
        InvocationId::new(912),
        binding,
        EngineTimestamp::from_unix_millis(3_050),
        EngineTimestamp::from_unix_millis(3_051),
        None,
        None,
        ProcedureRuntimeCounters::new(0, 0, 0, 0),
        ProcedureRuntimeStatus::Aborted,
        Some(AndromedaErrorKind::Security),
    )
    .expect("aborted pre-transaction evidence is accepted with empty counters");

    assert_eq!(
        store.attach_invocation_runtime(failed.clone()).unwrap(),
        InvocationRuntimeRecordOutcome::Stored
    );
    assert_eq!(
        store.attach_invocation_runtime(aborted.clone()).unwrap(),
        InvocationRuntimeRecordOutcome::Stored
    );

    let history = store.invocation_runtime_for(binding.procedure_id);
    assert_eq!(history.len(), 2);
    assert_eq!(history[0], failed);
    assert_eq!(history[1], aborted);
    assert_eq!(store.total_recorded_runtime_invocations(), 2);
    assert_eq!(store.total_recorded_decisions(), 0);
    assert_eq!(store.total_procedure_feedback(), 0);

    let failed = store
        .invocation_runtime_for_invocation(InvocationId::new(911))
        .expect("failed runtime evidence is indexed by invocation id");
    assert_eq!(failed.status, ProcedureRuntimeStatus::Failed);
    assert_eq!(failed.error_kind, Some(AndromedaErrorKind::Execution));
    assert_eq!(failed.decision_trace_id, Some(TraceId::new(9_111)));
    assert_eq!(failed.plan_key, Some(plan_key));
    assert_eq!(failed.plan_id, Some(plan_id));
    assert_eq!(failed.counters.rows_read, 5);
    assert_eq!(failed.counters.rows_returned, 2);
    assert!(!failed.counters.is_empty());
    assert!(failed.is_terminal());
    assert!(failed.is_observed_feedback());
    assert!(!failed.is_authoritative_decision());
    assert!(!failed.can_select_plan_alone());

    let aborted = store
        .invocation_runtime_for_invocation(InvocationId::new(912))
        .expect("aborted pre-transaction evidence is indexed by invocation id");
    assert_eq!(aborted.status, ProcedureRuntimeStatus::Aborted);
    assert_eq!(aborted.error_kind, Some(AndromedaErrorKind::Security));
    assert_eq!(aborted.plan_key, None);
    assert_eq!(aborted.plan_id, None);
    assert!(aborted.counters.is_empty());
    assert!(aborted.is_terminal());
    assert!(aborted.is_observed_feedback());
    assert!(!aborted.is_authoritative_decision());
    assert!(!aborted.can_select_plan_alone());
}

#[test]
fn scenario_evidence_advisory_use_never_selects_plan_alone() {
    let contract = sample_contract();
    let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();
    let binding = entry.binding;
    let evidence = ScenarioEvidence::new(
        ScenarioId::new(31).expect("non-zero scenario id"),
        ScenarioKind::Microbenchmark,
        ScenarioTarget {
            procedure_id: binding.procedure_id,
            catalog_version: binding.catalog_version,
            stats_version: binding.stats_version,
            plan_class: Some(PlanClass::Singleton),
            contract_hash: Some(binding.contract_hash),
        },
        EvidenceScore::from_permille(1_000).unwrap(),
        EvidenceConfidence::from_permille(1_000).unwrap(),
        ValidityWindow::new(
            EngineTimestamp::from_unix_millis(100),
            EngineTimestamp::from_unix_millis(200),
        )
        .unwrap(),
    )
    .expect("valid advisory scenario evidence");

    assert_eq!(
        evidence.optimizer_boundary(),
        ScenarioEvidenceOptimizerBoundary::AdvisoryOnly
    );
    assert!(!evidence.is_authoritative());
    assert!(!evidence.can_select_plan_alone());

    let advisory = evidence
        .advisory_use_at(EngineTimestamp::from_unix_millis(150))
        .expect("valid evidence window yields an advisory token");
    assert_eq!(
        advisory.boundary(),
        ScenarioEvidenceOptimizerBoundary::AdvisoryOnly
    );
    assert_eq!(advisory.digest(), evidence.digest());
    let advisory_target = advisory.target();
    assert_eq!(advisory_target.procedure_id, binding.procedure_id);
    assert_eq!(advisory_target.catalog_version, binding.catalog_version);
    assert_eq!(advisory_target.stats_version, binding.stats_version);
    assert_eq!(advisory_target.contract_hash, Some(binding.contract_hash));
    assert!(!advisory.is_authoritative());
    assert!(!advisory.can_select_plan_alone());
}

#[test]
fn evidence_boundary_taxonomies_are_bounded() {
    assert_eq!(ProcedureStoreEvidenceRole::VARIANT_COUNT, 2);

    let contract = sample_contract();
    let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();
    let decision = InvocationDecisionRecord::new(
        InvocationId::new(51),
        entry.binding,
        DecisionTrace {
            trace_id: TraceId::new(51),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "role taxonomy decision record".to_string(),
        },
    )
    .unwrap();
    let runtime = InvocationRuntimeRecord::new(
        InvocationId::new(52),
        entry.binding,
        EngineTimestamp::from_unix_millis(2_000),
        EngineTimestamp::from_unix_millis(2_001),
        None,
        None,
        ProcedureRuntimeCounters::new(1, 0, 0, 0),
        ProcedureRuntimeStatus::RolledBack,
        Some(AndromedaErrorKind::Transaction),
    )
    .unwrap();

    let decision_role = decision.evidence_role();
    let runtime_role = runtime.evidence_role();
    assert_eq!(decision_role.as_tag(), 0x01);
    assert_eq!(runtime_role.as_tag(), 0x02);
    assert!(decision_role.is_authoritative_decision());
    assert!(!decision_role.is_observed_feedback());
    assert!(runtime_role.is_observed_feedback());
    assert!(!runtime_role.is_authoritative_decision());
    assert!(!decision_role.can_select_plan_alone());
    assert!(!runtime_role.can_select_plan_alone());

    assert_eq!(ScenarioEvidenceOptimizerBoundary::VARIANT_COUNT, 1);
    assert!(!ScenarioEvidenceOptimizerBoundary::AdvisoryOnly.is_authoritative());
    assert!(!ScenarioEvidenceOptimizerBoundary::AdvisoryOnly.can_select_plan_alone());
}

#[test]
fn public_role_and_advisory_token_constructors_remain_closed() {
    let role_source = include_str!("../../andromeda-procedure-store/src/evidence_role.rs");
    assert!(role_source.contains("pub struct ProcedureStoreEvidenceRole"));
    assert!(role_source.contains("kind: ProcedureStoreEvidenceRoleKind"));
    assert!(role_source.contains("enum ProcedureStoreEvidenceRoleKind"));
    assert!(!role_source.contains("pub enum ProcedureStoreEvidenceRole"));
    assert!(!role_source.contains("pub const ALL"));
    assert!(!role_source.contains("pub const fn authoritative_decision"));
    assert!(!role_source.contains("pub const fn observed_feedback"));

    let advisory_source = include_str!("../src/scenario_evidence.rs");
    assert!(advisory_source.contains("pub struct ScenarioEvidenceAdvisoryUse"));
    assert!(!advisory_source.contains("pub scenario_id:"));
    assert!(!advisory_source.contains("pub digest: [u8; 32]"));
    assert!(!advisory_source.contains("pub boundary:"));
    assert!(!advisory_source.contains("pub target: ScenarioTarget"));
    assert!(!advisory_source.contains("pub score: EvidenceScore"));
    assert!(!advisory_source.contains("pub confidence: EvidenceConfidence"));
}

#[test]
fn procedure_store_rejects_runtime_record_with_mismatched_expected_binding() {
    let contract = sample_contract();
    let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();
    let binding = entry.binding;
    let mut store = ProcedureStore::new();
    store.register(entry).unwrap();

    let mut contract_drift = binding;
    contract_drift.contract_hash = ContractHash::test_vector(99);
    let mut catalog_drift = binding;
    catalog_drift.catalog_version = CatalogVersion::new(binding.catalog_version.get() + 1);
    let mut stats_drift = binding;
    stats_drift.stats_version = StatsVersion::new(binding.stats_version.get() + 1);
    let mut policy_drift = binding;
    policy_drift.policy_version = PolicyVersion::new([9; PolicyVersion::LEN]);

    for (offset, (drifted, expected_message)) in [
        (contract_drift, "contract hash"),
        (catalog_drift, "catalog version"),
        (stats_drift, "stats version"),
        (policy_drift, "policy version"),
    ]
    .into_iter()
    .enumerate()
    {
        let runtime = InvocationRuntimeRecord::new(
            InvocationId::new(910 + offset as u64),
            drifted,
            EngineTimestamp::from_unix_millis(2_000),
            EngineTimestamp::from_unix_millis(2_010),
            None,
            None,
            ProcedureRuntimeCounters::new(1, 0, 0, 0),
            ProcedureRuntimeStatus::RolledBack,
            Some(AndromedaErrorKind::Transaction),
        )
        .expect("runtime record is structurally valid before store binding check");

        let err = store.attach_invocation_runtime(runtime).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains(expected_message));
    }
    assert_eq!(store.total_recorded_runtime_invocations(), 0);
}

#[test]
fn public_boundary_keeps_feedback_advisory_and_out_of_decision_indexes() {
    let contract = sample_contract();
    let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();
    let binding = entry.binding;
    let mut store = ProcedureStore::new();
    store.register(entry).unwrap();

    let feedback = ProcedureFeedback::new(
        FeedbackId::new(41).expect("non-zero feedback id"),
        binding.procedure_id,
        Some([0xCA; 32]),
        binding.stats_version,
        CompletionEvidence {
            status: CompletionStatus::Committed,
            completion_code: Some(0),
            row_count: Some(4),
            durable_lsn: Some(77),
        },
        ValidityWindow::new(
            EngineTimestamp::from_unix_millis(2_000),
            EngineTimestamp::from_unix_millis(3_000),
        )
        .unwrap(),
    )
    .expect("valid feedback remains advisory");
    assert!(!feedback.is_authoritative());

    assert_eq!(
        store.attach_procedure_feedback(feedback).unwrap(),
        RecordOutcome::Stored
    );
    assert_eq!(store.total_recorded_decisions(), 0);
    assert_eq!(store.total_recorded_runtime_invocations(), 0);
    assert_eq!(store.total_procedure_feedback(), 1);

    let visible = store.procedure_feedback_for_at(
        binding.procedure_id,
        EngineTimestamp::from_unix_millis(2_500),
    );
    assert_eq!(visible, vec![feedback]);
    assert!(visible.iter().all(|item| !item.is_authoritative()));
}
