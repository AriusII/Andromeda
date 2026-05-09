//! Integration tests for the Procedure Store registry and evidence boundary.

use andromeda_error::{AndromedaErrorKind, AndromedaResult};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_plan_cache::{PlanCacheKey, PlanClass, PlanShapeFingerprint};
use andromeda_procedure_contract::{
    AccessMode, CatalogObjectRef, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy,
    ObjectKind, ProcedureContract, ProcedureErrorPolicy, ProtocolLayoutRef, QualifiedName,
    ResultMetadataPolicy, StatsVersion, TransactionPolicy,
};
use andromeda_procedure_store::{
    CompletionEvidence, CompletionStatus, FeedbackId, InvocationDecisionRecord,
    InvocationRuntimeRecord, InvocationRuntimeRecordOutcome, ProcedureFeedback,
    ProcedureRegistration, ProcedureRuntimeCounters, ProcedureRuntimePlanId,
    ProcedureRuntimeStatus, ProcedureStore, ProcedureStoreEntry, RecordOutcome,
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

fn registered_store() -> AndromedaResult<(ProcedureStore, ProcedureStoreEntry)> {
    let contract = sample_contract();
    let entry = ProcedureStoreEntry::from_contract(&contract)?;
    let mut store = ProcedureStore::new();
    assert_eq!(
        store.register(entry.clone())?,
        ProcedureRegistration::Inserted
    );
    Ok((store, entry))
}

#[test]
fn procedure_store_registers_contract_and_records_authoritative_decision() {
    let (mut store, entry) = registered_store().unwrap();

    assert_eq!(
        store.register(entry.clone()).unwrap(),
        ProcedureRegistration::AlreadyRegistered
    );
    assert_eq!(store.get(entry.procedure_id).unwrap(), &entry);
    assert_eq!(
        store
            .lookup_by_name(&QualifiedName::parse("Inventory.ReserveStock").unwrap())
            .unwrap()
            .procedure_id,
        entry.procedure_id
    );

    let decision = InvocationDecisionRecord::new(
        InvocationId::new(909),
        entry.binding,
        DecisionTrace {
            trace_id: TraceId::new(909),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "inventory reserve stock contract validated".to_string(),
        },
    )
    .unwrap();

    store
        .attach_invocation_decision(decision.clone())
        .expect("matching decision evidence is accepted");

    let by_invocation = store
        .invocation_decisions_for_invocation(InvocationId::new(909))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(by_invocation, vec![decision]);
    assert_eq!(store.total_recorded_decisions(), 1);
}

#[test]
fn procedure_store_rejects_evidence_with_mismatched_binding() {
    let (mut store, entry) = registered_store().unwrap();
    let mut binding = entry.binding;
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
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("contract hash"));
}

#[test]
fn procedure_store_records_runtime_invocation_once_per_invocation() {
    let (mut store, entry) = registered_store().unwrap();
    let binding = entry.binding;
    let plan_key = PlanCacheKey::build(
        &binding,
        PlanClass::Singleton,
        PlanShapeFingerprint::empty(),
    )
    .expect("singleton plan key is valid");
    let plan_id = ProcedureRuntimePlanId::from_plan_cache_key(&plan_key);
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

    assert_eq!(
        store.attach_invocation_runtime(runtime.clone()).unwrap(),
        InvocationRuntimeRecordOutcome::Stored
    );
    assert_eq!(
        store.attach_invocation_runtime(runtime.clone()).unwrap(),
        InvocationRuntimeRecordOutcome::Duplicate
    );

    let stored = store
        .invocation_runtime_for_invocation(InvocationId::new(910))
        .expect("runtime evidence is indexed by invocation id");
    assert_eq!(stored.duration_millis, 87);
    assert_eq!(stored.counters.rows_read, 13);
    assert_eq!(stored.counters.rows_returned, 9);
    assert_eq!(stored.counters.rows_written, 4);
    assert_eq!(stored.counters.rows_affected, 4);
    assert_eq!(stored.plan_key, Some(plan_key));
    assert_eq!(stored.plan_id, Some(plan_id));
    assert!(stored.is_observed_feedback());
    assert!(!stored.is_authoritative_decision());
    assert!(!stored.can_select_plan_alone());
    assert_eq!(store.total_recorded_runtime_invocations(), 1);
}

#[test]
fn procedure_store_keeps_advisory_feedback_out_of_decision_indexes() {
    let (mut store, entry) = registered_store().unwrap();
    let binding = entry.binding;
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
        andromeda_scenario_evidence::ValidityWindow::new(
            EngineTimestamp::from_unix_millis(2_000),
            EngineTimestamp::from_unix_millis(3_000),
        )
        .unwrap(),
    )
    .expect("valid feedback remains advisory");

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
