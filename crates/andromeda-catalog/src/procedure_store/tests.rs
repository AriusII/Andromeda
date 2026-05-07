use super::*;
use crate::{
    AccessMode, CatalogObjectRef, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy,
    ObjectKind, PolicyVersion, ProcedureContractBinding, ProcedureErrorPolicy, ProcedureFeedback,
    ProtocolLayoutRef, QualifiedName, RecordOutcome, ResultMetadataPolicy, StatsVersion,
    TransactionPolicy,
};
use andromeda_core::{
    AndromedaErrorKind, CatalogObjectId, CatalogVersion, ContractHash, EngineTimestamp,
    InvocationId, ProcedureId,
};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};

fn entry(id: u64, name: &str, hash_seed: u8) -> ProcedureStoreEntry {
    ProcedureStoreEntry {
        procedure_id: ProcedureId::new(id),
        name: QualifiedName::parse(name).unwrap(),
        binding: ProcedureContractBinding {
            procedure_id: ProcedureId::new(id),
            catalog_version: CatalogVersion::new(7),
            contract_hash: ContractHash::test_vector(hash_seed),
            stats_version: StatsVersion::new(1),
            policy_version: PolicyVersion::new([1; PolicyVersion::LEN]),
        },
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(101),
            frame_envelope_hash: ContractHash::test_vector(202),
        },
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Snapshot,
            retryable: false,
        },
        required_permissions: vec!["execute_procedure".to_string()],
    }
}

fn decision(invocation: u64, procedure_id: u64, hash_seed: u8) -> InvocationDecisionRecord {
    let binding = ProcedureContractBinding {
        procedure_id: ProcedureId::new(procedure_id),
        catalog_version: CatalogVersion::new(7),
        contract_hash: ContractHash::test_vector(hash_seed),
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new([1; PolicyVersion::LEN]),
    };
    InvocationDecisionRecord::new(
        InvocationId::new(invocation),
        binding,
        DecisionTrace {
            trace_id: TraceId::new(invocation as u128),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "contract validated against registered binding".to_string(),
        },
    )
    .unwrap()
}

fn plan_key_for(binding: &ProcedureContractBinding) -> crate::PlanCacheKey {
    crate::PlanCacheKey::build(
        binding,
        crate::PlanClass::Singleton,
        crate::PlanShapeFingerprint::empty(),
    )
    .expect("singleton plan key")
}

fn plan_id_for(binding: &ProcedureContractBinding) -> ProcedureRuntimePlanId {
    ProcedureRuntimePlanId::from_plan_cache_key(&plan_key_for(binding))
}

fn runtime_record(
    invocation: u64,
    binding: ProcedureContractBinding,
    rows_read: u64,
    rows_written: u64,
    wal_bytes: u64,
) -> InvocationRuntimeRecord {
    InvocationRuntimeRecord::new(
        InvocationId::new(invocation),
        binding,
        EngineTimestamp::from_unix_millis(1_000),
        EngineTimestamp::from_unix_millis(1_042),
        Some(plan_key_for(&binding)),
        Some(plan_id_for(&binding)),
        ProcedureRuntimeCounters::new(rows_read, rows_written, wal_bytes, 8),
        ProcedureRuntimeStatus::Committed,
        None,
    )
    .unwrap()
}

#[test]
fn registration_inserts_new_procedure_and_is_idempotent_for_same_entry() {
    let mut store = ProcedureStore::new();
    let e = entry(1, "Inventory.ReserveStock", 42);

    assert_eq!(
        store.register(e.clone()).unwrap(),
        ProcedureRegistration::Inserted
    );
    assert_eq!(store.len(), 1);
    assert_eq!(
        store.register(e.clone()).unwrap(),
        ProcedureRegistration::AlreadyRegistered
    );
    assert!(store.get(ProcedureId::new(1)).is_some());
    assert!(
        store
            .lookup_by_name(&QualifiedName::parse("Inventory.ReserveStock").unwrap())
            .is_some()
    );
}

#[test]
fn registration_rejects_contract_hash_divergence_for_same_id() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();
    let err = store
        .register(entry(1, "Inventory.ReserveStock", 43))
        .unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().contains("contract hash divergence"));
}

#[test]
fn registration_rejects_binding_divergence_for_same_id() {
    let mut store = ProcedureStore::new();
    let registered = entry(1, "Inventory.ReserveStock", 42);
    store.register(registered.clone()).unwrap();

    let mut drifted_stats = registered.clone();
    drifted_stats.binding.stats_version = StatsVersion::new(2);
    let err = store.register(drifted_stats).unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().contains("binding divergence"));

    let mut drifted_policy = registered;
    drifted_policy.binding.policy_version = PolicyVersion::new([2; PolicyVersion::LEN]);
    let err = store.register(drifted_policy).unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().contains("binding divergence"));
}

#[test]
fn registration_rejects_qualified_name_collision_across_ids() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();
    let err = store
        .register(entry(2, "Inventory.ReserveStock", 99))
        .unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Catalog);
    assert!(err.message().contains("qualified name collision"));
}

#[test]
fn registration_rejects_zero_procedure_id() {
    let mut store = ProcedureStore::new();
    let bad = entry(0, "Inventory.ReserveStock", 42);
    let err = store.register(bad).unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Catalog);
}

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

#[test]
fn runtime_record_attaches_full_binding_and_metrics() {
    let mut store = ProcedureStore::new();
    let registered = entry(1, "Inventory.ReserveStock", 42);
    let binding = registered.binding;
    store.register(registered).unwrap();

    let record = runtime_record(501, binding, 9, 2, 128);
    assert_eq!(
        store.attach_invocation_runtime(record.clone()).unwrap(),
        InvocationRuntimeRecordOutcome::Stored
    );
    assert_eq!(
        store.attach_invocation_runtime(record.clone()).unwrap(),
        InvocationRuntimeRecordOutcome::Duplicate
    );

    let by_procedure = store.invocation_runtime_for(ProcedureId::new(1));
    assert_eq!(by_procedure, std::slice::from_ref(&record));
    let by_invocation = store
        .invocation_runtime_for_invocation(InvocationId::new(501))
        .expect("runtime record is indexed by invocation");
    assert_eq!(by_invocation.duration_millis, 42);
    assert_eq!(by_invocation.counters.rows_read, 9);
    assert_eq!(by_invocation.counters.rows_returned, 0);
    assert_eq!(by_invocation.counters.rows_written, 2);
    assert_eq!(by_invocation.counters.rows_affected, 2);
    assert_eq!(by_invocation.counters.wal_bytes, 128);
    assert_eq!(
        by_invocation.plan_key.expect("plan key recorded"),
        plan_key_for(&binding)
    );
    assert_eq!(
        by_invocation.plan_class(),
        Some(crate::PlanClass::Singleton)
    );
    assert_eq!(
        by_invocation.expected_contract_hash(),
        binding.contract_hash
    );
    assert_eq!(
        by_invocation.expected_catalog_version(),
        binding.catalog_version
    );
    assert_eq!(
        by_invocation.expected_stats_version(),
        binding.stats_version
    );
    assert_eq!(by_invocation.binding.policy_version, binding.policy_version);
    assert_eq!(
        by_invocation.expected_policy_version(),
        binding.policy_version
    );
    assert!(!by_invocation.record_id.is_zero());
    assert!(by_invocation.is_terminal());
    assert!(by_invocation.is_observed_feedback());
    assert!(!by_invocation.is_authoritative_decision());
    assert_eq!(store.total_recorded_runtime_invocations(), 1);
}

#[test]
fn runtime_record_captures_extended_rows_io_temp_and_spill_counters() {
    let mut store = ProcedureStore::new();
    let registered = entry(1, "Inventory.ReserveStock", 42);
    let binding = registered.binding;
    store.register(registered).unwrap();

    let counters = ProcedureRuntimeCounters::new(15, 3, 512, 64)
        .with_rows_returned(8)
        .with_rows_affected(11)
        .with_read_counters(21, 2, 19)
        .with_spill_bytes(32);
    let record = InvocationRuntimeRecord::new_with_decision_trace(
        InvocationId::new(601),
        binding,
        EngineTimestamp::from_unix_millis(2_000),
        EngineTimestamp::from_unix_millis(2_033),
        TraceId::new(9_001),
        Some(plan_key_for(&binding)),
        Some(plan_id_for(&binding)),
        counters,
        ProcedureRuntimeStatus::Committed,
        None,
    )
    .expect("complete runtime evidence with trace id is valid");

    assert_eq!(
        store.attach_invocation_runtime(record.clone()).unwrap(),
        InvocationRuntimeRecordOutcome::Stored
    );

    let stored = store
        .invocation_runtime_for_invocation(InvocationId::new(601))
        .expect("runtime record is indexed by invocation");
    assert_eq!(stored.decision_trace_id, Some(TraceId::new(9_001)));
    assert_eq!(stored.counters.rows_read, 15);
    assert_eq!(stored.counters.rows_returned, 8);
    assert_eq!(stored.counters.rows_written, 3);
    assert_eq!(stored.counters.rows_affected, 11);
    assert_eq!(stored.counters.logical_reads, 21);
    assert_eq!(stored.counters.physical_reads, 2);
    assert_eq!(stored.counters.cache_hits, 19);
    assert_eq!(stored.counters.wal_bytes, 512);
    assert_eq!(stored.counters.temp_bytes, 64);
    assert_eq!(stored.counters.spill_bytes, 32);
}

#[test]
fn runtime_record_id_is_deterministic_and_covers_terminal_evidence() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let first = runtime_record(701, binding, 9, 2, 128);
    let identical = runtime_record(701, binding, 9, 2, 128);
    let different_wal = runtime_record(701, binding, 9, 2, 256);

    assert_eq!(first.record_id, identical.record_id);
    assert_ne!(first.record_id, different_wal.record_id);

    let mut tampered = first.clone();
    tampered.counters.rows_returned = 99;
    let err = tampered.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("record id"));
}

#[test]
fn runtime_record_rejects_internal_authoritative_role_tampering() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let mut record = runtime_record(702, binding, 9, 2, 128);
    record.evidence_role = ProcedureStoreEvidenceRole::authoritative_decision();

    let err = record.validate().unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("observed feedback"));
}

#[test]
fn runtime_record_positive_covers_every_terminal_status() {
    assert_eq!(
        ProcedureRuntimeStatus::ALL.len(),
        ProcedureRuntimeStatus::VARIANT_COUNT
    );

    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    for status in ProcedureRuntimeStatus::ALL {
        let counters = match status {
            ProcedureRuntimeStatus::Committed => ProcedureRuntimeCounters::new(1, 1, 64, 0),
            ProcedureRuntimeStatus::RolledBack | ProcedureRuntimeStatus::Failed => {
                ProcedureRuntimeCounters::new(1, 0, 0, 0)
            }
            ProcedureRuntimeStatus::Aborted => ProcedureRuntimeCounters::new(0, 0, 0, 0),
        };
        let error_kind = match status {
            ProcedureRuntimeStatus::Committed => None,
            ProcedureRuntimeStatus::RolledBack => Some(AndromedaErrorKind::Transaction),
            ProcedureRuntimeStatus::Failed => Some(AndromedaErrorKind::Execution),
            ProcedureRuntimeStatus::Aborted => Some(AndromedaErrorKind::Security),
        };

        let record = InvocationRuntimeRecord::new(
            InvocationId::new(800 + status.as_tag() as u64),
            binding,
            EngineTimestamp::from_unix_millis(1_000),
            EngineTimestamp::from_unix_millis(1_005),
            Some(plan_key_for(&binding)),
            Some(plan_id_for(&binding)),
            counters,
            status,
            error_kind,
        )
        .expect("each terminal status has one valid evidence shape");

        assert!(record.is_terminal());
        assert_eq!(record.status, status);
    }
}

#[test]
fn runtime_record_rejects_policy_version_drift() {
    let mut store = ProcedureStore::new();
    let registered = entry(1, "Inventory.ReserveStock", 42);
    let mut drifted = registered.binding;
    store.register(registered).unwrap();
    drifted.policy_version = PolicyVersion::new([2; PolicyVersion::LEN]);

    let err = store
        .attach_invocation_runtime(runtime_record(501, drifted, 9, 2, 128))
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("policy version"));
    assert_eq!(store.total_recorded_runtime_invocations(), 0);
}

#[test]
fn runtime_record_rejects_stats_version_drift() {
    let mut store = ProcedureStore::new();
    let registered = entry(1, "Inventory.ReserveStock", 42);
    let mut drifted = registered.binding;
    store.register(registered).unwrap();
    drifted.stats_version = StatsVersion::new(2);

    let err = store
        .attach_invocation_runtime(runtime_record(501, drifted, 9, 2, 128))
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("stats version"));
}

#[test]
fn runtime_record_rejects_contract_or_catalog_version_drift() {
    let mut store = ProcedureStore::new();
    let registered = entry(1, "Inventory.ReserveStock", 42);
    let binding = registered.binding;
    store.register(registered).unwrap();

    let mut contract_drift = binding;
    contract_drift.contract_hash = ContractHash::test_vector(99);
    let err = store
        .attach_invocation_runtime(runtime_record(501, contract_drift, 9, 2, 128))
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("contract hash"));

    let mut catalog_drift = binding;
    catalog_drift.catalog_version = CatalogVersion::new(binding.catalog_version.get() + 1);
    let err = store
        .attach_invocation_runtime(runtime_record(502, catalog_drift, 9, 2, 128))
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("catalog version"));
    assert_eq!(store.total_recorded_runtime_invocations(), 0);
}

#[test]
fn runtime_record_rejects_committed_write_without_wal_bytes() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let err = InvocationRuntimeRecord::new(
        InvocationId::new(501),
        binding,
        EngineTimestamp::from_unix_millis(1_000),
        EngineTimestamp::from_unix_millis(1_001),
        Some(plan_key_for(&binding)),
        Some(plan_id_for(&binding)),
        ProcedureRuntimeCounters::new(1, 1, 0, 0),
        ProcedureRuntimeStatus::Committed,
        None,
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("WAL bytes"));
}

#[test]
fn runtime_record_rejects_committed_with_error_kind() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let err = InvocationRuntimeRecord::new(
        InvocationId::new(501),
        binding,
        EngineTimestamp::from_unix_millis(1_000),
        EngineTimestamp::from_unix_millis(1_001),
        Some(plan_key_for(&binding)),
        Some(plan_id_for(&binding)),
        ProcedureRuntimeCounters::new(1, 0, 0, 0),
        ProcedureRuntimeStatus::Committed,
        Some(AndromedaErrorKind::Execution),
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("must not carry an error kind"));
}

#[test]
fn runtime_record_rejects_failed_without_error_kind() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let err = InvocationRuntimeRecord::new(
        InvocationId::new(501),
        binding,
        EngineTimestamp::from_unix_millis(1_000),
        EngineTimestamp::from_unix_millis(1_001),
        Some(plan_key_for(&binding)),
        Some(plan_id_for(&binding)),
        ProcedureRuntimeCounters::new(1, 0, 0, 0),
        ProcedureRuntimeStatus::Failed,
        None,
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("error kind"));
}

#[test]
fn runtime_record_rejects_aborted_with_any_runtime_counters() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let err = InvocationRuntimeRecord::new(
        InvocationId::new(501),
        binding,
        EngineTimestamp::from_unix_millis(1_000),
        EngineTimestamp::from_unix_millis(1_001),
        None,
        None,
        ProcedureRuntimeCounters::new(0, 0, 0, 1),
        ProcedureRuntimeStatus::Aborted,
        Some(AndromedaErrorKind::Security),
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("aborted pre-transaction"));
}

#[test]
fn runtime_record_rejects_zero_decision_trace_id_when_present() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let err = InvocationRuntimeRecord::new_with_decision_trace(
        InvocationId::new(501),
        binding,
        EngineTimestamp::from_unix_millis(1_000),
        EngineTimestamp::from_unix_millis(1_001),
        TraceId::new(0),
        Some(plan_key_for(&binding)),
        Some(plan_id_for(&binding)),
        ProcedureRuntimeCounters::new(1, 0, 0, 0),
        ProcedureRuntimeStatus::Committed,
        None,
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("decision trace id"));
}

#[test]
fn runtime_record_rejects_plan_id_without_observable_plan_cache_key() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let err = InvocationRuntimeRecord::new(
        InvocationId::new(501),
        binding,
        EngineTimestamp::from_unix_millis(1_000),
        EngineTimestamp::from_unix_millis(1_001),
        None,
        Some(plan_id_for(&binding)),
        ProcedureRuntimeCounters::new(1, 0, 0, 0),
        ProcedureRuntimeStatus::Committed,
        None,
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("plan cache key"));
}

#[test]
fn runtime_record_rejects_plan_key_without_plan_id() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let err = InvocationRuntimeRecord::new(
        InvocationId::new(501),
        binding,
        EngineTimestamp::from_unix_millis(1_000),
        EngineTimestamp::from_unix_millis(1_001),
        Some(plan_key_for(&binding)),
        None,
        ProcedureRuntimeCounters::new(1, 0, 0, 0),
        ProcedureRuntimeStatus::Committed,
        None,
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("requires a recorded plan id"));
}

#[test]
fn runtime_record_rejects_zero_plan_id_when_plan_is_present() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let err = InvocationRuntimeRecord::new(
        InvocationId::new(501),
        binding,
        EngineTimestamp::from_unix_millis(1_000),
        EngineTimestamp::from_unix_millis(1_001),
        Some(plan_key_for(&binding)),
        Some(ProcedureRuntimePlanId::new(
            [0; ProcedureRuntimePlanId::LEN],
        )),
        ProcedureRuntimeCounters::new(1, 0, 0, 0),
        ProcedureRuntimeStatus::Committed,
        None,
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("plan id must not be zero"));
}

#[test]
fn runtime_record_rejects_temp_or_spill_without_observable_plan_cache_key() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    for counters in [
        ProcedureRuntimeCounters::new(1, 0, 0, 1),
        ProcedureRuntimeCounters::new(1, 0, 0, 0).with_spill_bytes(1),
    ] {
        let err = InvocationRuntimeRecord::new(
            InvocationId::new(501),
            binding,
            EngineTimestamp::from_unix_millis(1_000),
            EngineTimestamp::from_unix_millis(1_001),
            None,
            None,
            counters,
            ProcedureRuntimeStatus::Committed,
            None,
        )
        .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("temp or spill bytes"));
    }
}

#[test]
fn runtime_record_rejects_plan_key_version_drift_from_binding() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let mut drifted = binding;
    drifted.stats_version = StatsVersion::new(binding.stats_version.get() + 1);
    let plan_key = plan_key_for(&binding);
    let err = InvocationRuntimeRecord::new(
        InvocationId::new(502),
        drifted,
        EngineTimestamp::from_unix_millis(1_000),
        EngineTimestamp::from_unix_millis(1_001),
        Some(plan_key),
        Some(ProcedureRuntimePlanId::from_plan_cache_key(&plan_key)),
        ProcedureRuntimeCounters::new(1, 0, 0, 0),
        ProcedureRuntimeStatus::Committed,
        None,
    )
    .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("plan cache key stats version"));
}

#[test]
fn runtime_record_rejects_plan_key_identity_drift_from_binding() {
    let binding = entry(1, "Inventory.ReserveStock", 42).binding;
    let plan_key = plan_key_for(&binding);
    let plan_id = ProcedureRuntimePlanId::from_plan_cache_key(&plan_key);

    let mut cases = Vec::new();

    let mut procedure_drift = binding;
    procedure_drift.procedure_id = ProcedureId::new(2);
    cases.push((procedure_drift, "procedure id"));

    let mut contract_drift = binding;
    contract_drift.contract_hash = ContractHash::test_vector(99);
    cases.push((contract_drift, "contract hash"));

    let mut catalog_drift = binding;
    catalog_drift.catalog_version = CatalogVersion::new(8);
    cases.push((catalog_drift, "catalog version"));

    let mut policy_drift = binding;
    policy_drift.policy_version = PolicyVersion::new([9; PolicyVersion::LEN]);
    cases.push((policy_drift, "policy version"));

    for (drifted, expected_message) in cases {
        let err = InvocationRuntimeRecord::new(
            InvocationId::new(502),
            drifted,
            EngineTimestamp::from_unix_millis(1_000),
            EngineTimestamp::from_unix_millis(1_001),
            Some(plan_key),
            Some(plan_id),
            ProcedureRuntimeCounters::new(1, 0, 0, 0),
            ProcedureRuntimeStatus::Committed,
            None,
        )
        .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains(expected_message));
    }
}

#[test]
fn runtime_record_rejects_each_missing_binding_identity() {
    let valid = entry(1, "Inventory.ReserveStock", 42).binding;
    let mut cases = Vec::new();

    let mut missing_procedure_id = valid;
    missing_procedure_id.procedure_id = ProcedureId::new(0);
    cases.push(missing_procedure_id);

    let mut missing_catalog_version = valid;
    missing_catalog_version.catalog_version = CatalogVersion::new(0);
    cases.push(missing_catalog_version);

    let mut missing_contract_hash = valid;
    missing_contract_hash.contract_hash = ContractHash::zero();
    cases.push(missing_contract_hash);

    let mut missing_stats_version = valid;
    missing_stats_version.stats_version = StatsVersion::new(0);
    cases.push(missing_stats_version);

    let mut missing_policy_version = valid;
    missing_policy_version.policy_version = PolicyVersion::zero();
    cases.push(missing_policy_version);

    for binding in cases {
        let err = InvocationRuntimeRecord::new(
            InvocationId::new(501),
            binding,
            EngineTimestamp::from_unix_millis(1_000),
            EngineTimestamp::from_unix_millis(1_001),
            None,
            None,
            ProcedureRuntimeCounters::new(0, 0, 0, 0),
            ProcedureRuntimeStatus::Committed,
            None,
        )
        .unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    }
}

#[test]
fn runtime_record_rejects_conflicting_terminal_evidence_for_invocation() {
    let mut store = ProcedureStore::new();
    let registered = entry(1, "Inventory.ReserveStock", 42);
    let binding = registered.binding;
    store.register(registered).unwrap();
    store
        .attach_invocation_runtime(runtime_record(501, binding, 9, 2, 128))
        .unwrap();

    let err = store
        .attach_invocation_runtime(runtime_record(501, binding, 10, 2, 128))
        .unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message()
            .contains("conflicting terminal runtime evidence")
    );
    assert_eq!(store.total_recorded_runtime_invocations(), 1);
}

#[test]
fn store_round_trips_a_validated_procedure_contract() {
    let mut contract = crate::ProcedureContract {
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
    let expected_hash = contract.contract_hash;

    let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();
    let mut store = ProcedureStore::new();
    assert_eq!(
        store.register(entry).unwrap(),
        ProcedureRegistration::Inserted
    );
    assert_eq!(
        store.get(ProcedureId::new(1)).unwrap().contract_hash(),
        expected_hash
    );
}

#[test]
fn store_entry_from_contract_rejects_stale_canonical_contract_hash() {
    let mut contract = crate::ProcedureContract {
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
    contract.stats_version = StatsVersion::new(2);

    let err = ProcedureStoreEntry::from_contract(&contract).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("canonical contract shape"));
}

fn make_window(issued: u64, expires: u64) -> crate::ValidityWindow {
    crate::ValidityWindow::new(
        andromeda_core::EngineTimestamp::from_unix_millis(issued),
        andromeda_core::EngineTimestamp::from_unix_millis(expires),
    )
    .expect("valid window")
}

fn feedback_for(feedback_id: u64, procedure_id: u64, stats_version: u64) -> ProcedureFeedback {
    ProcedureFeedback::new(
        crate::FeedbackId::new(feedback_id).expect("non-zero feedback id"),
        ProcedureId::new(procedure_id),
        Some([0xCD; 32]),
        StatsVersion::new(stats_version),
        crate::CompletionEvidence {
            status: crate::CompletionStatus::Committed,
            completion_code: Some(0),
            row_count: Some(3),
            durable_lsn: Some(42),
        },
        make_window(10, 1_000),
    )
    .expect("valid feedback")
}

#[test]
fn procedure_feedback_attaches_to_registered_procedure() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();

    let outcome = store
        .attach_procedure_feedback(feedback_for(1001, 1, 1))
        .expect("registered procedure accepts matching feedback");
    assert_eq!(outcome, RecordOutcome::Stored);

    // Idempotent re-attach is a deterministic Duplicate.
    let outcome = store
        .attach_procedure_feedback(feedback_for(1001, 1, 1))
        .expect("byte-identical feedback is idempotent");
    assert_eq!(outcome, RecordOutcome::Duplicate);
    assert_eq!(store.total_procedure_feedback(), 1);

    let visible = store.procedure_feedback_for_at(
        ProcedureId::new(1),
        andromeda_core::EngineTimestamp::from_unix_millis(50),
    );
    assert_eq!(visible.len(), 1);
    assert!(!visible[0].is_authoritative());
}

#[test]
fn procedure_feedback_rejects_unknown_procedure() {
    let mut store = ProcedureStore::new();
    let err = store
        .attach_procedure_feedback(feedback_for(1001, 99, 1))
        .unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().contains("unknown procedure id"));
    assert_eq!(store.total_procedure_feedback(), 0);
}

#[test]
fn procedure_feedback_rejects_stats_version_mismatch_with_binding() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();
    // Registered binding uses StatsVersion::new(1); supply 7 instead.
    let err = store
        .attach_procedure_feedback(feedback_for(1001, 1, 7))
        .unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().contains("stats version"));
    assert_eq!(store.total_procedure_feedback(), 0);
}

#[test]
fn procedure_feedback_rejects_conflicting_record_with_same_id_but_different_digest() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();
    store
        .attach_procedure_feedback(feedback_for(1001, 1, 1))
        .unwrap();

    let conflicting = ProcedureFeedback::new(
        crate::FeedbackId::new(1001).unwrap(),
        ProcedureId::new(1),
        Some([0xCD; 32]),
        StatsVersion::new(1),
        crate::CompletionEvidence {
            status: crate::CompletionStatus::Failed,
            completion_code: Some(7),
            row_count: None,
            durable_lsn: None,
        },
        make_window(10, 1_000),
    )
    .unwrap();

    let err = store.attach_procedure_feedback(conflicting).unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().to_lowercase().contains("conflict"));
    assert_eq!(store.total_procedure_feedback(), 1);
}

#[test]
fn procedure_feedback_prune_expired_drops_only_expired_records() {
    let mut store = ProcedureStore::new();
    store
        .register(entry(1, "Inventory.ReserveStock", 42))
        .unwrap();
    // Window 10..50 expires at t=50; window 10..1000 stays valid.
    let short = ProcedureFeedback::new(
        crate::FeedbackId::new(1).unwrap(),
        ProcedureId::new(1),
        None,
        StatsVersion::new(1),
        crate::CompletionEvidence {
            status: crate::CompletionStatus::Committed,
            completion_code: Some(0),
            row_count: Some(1),
            durable_lsn: Some(7),
        },
        make_window(10, 50),
    )
    .unwrap();
    store.attach_procedure_feedback(short).unwrap();
    store
        .attach_procedure_feedback(feedback_for(2, 1, 1))
        .unwrap();

    let removed = store
        .prune_expired_procedure_feedback(andromeda_core::EngineTimestamp::from_unix_millis(100));
    assert_eq!(removed, 1);
    assert_eq!(store.total_procedure_feedback(), 1);
}

#[test]
fn procedure_store_production_sources_do_not_panic_or_unwrap() {
    let sources = [
        include_str!("../procedure_store.rs"),
        include_str!("decision.rs"),
        include_str!("entry.rs"),
        include_str!("evidence_role.rs"),
        include_str!("registration.rs"),
        include_str!("runtime.rs"),
        include_str!("store.rs"),
    ];
    let forbidden = [
        "unwrap(",
        "unwrap_or(",
        "unwrap_or_else(",
        "expect(",
        "panic!",
        "todo!",
        "unimplemented!",
        "unreachable!",
    ];

    for source in sources {
        for token in forbidden {
            assert!(
                !source.contains(token),
                "procedure store production source must not contain panic/unwrap token: {token}"
            );
        }
    }
}

/// Doctrine guard: the Procedure Store must not expose an ad hoc SQL or
/// raw-text query surface. Callers must address procedures by typed id
/// or qualified name. This test scans this module's source for forbidden
/// tokens that would indicate such a surface was introduced. The needles
/// are built at runtime from halves so this test body is not itself a
/// false positive.
#[test]
fn procedure_store_exposes_no_ad_hoc_sql_surface() {
    let sources = [
        include_str!("../procedure_store.rs"),
        include_str!("decision.rs"),
        include_str!("entry.rs"),
        include_str!("registration.rs"),
        include_str!("store.rs"),
    ];
    let halves: &[(&str, &str)] = &[
        ("fn query_", "sql"),
        ("fn execute_", "sql"),
        ("fn raw_", "query"),
        ("raw_", "sql"),
        ("SE", "LECT "),
        ("INSERT ", "INTO"),
        ("EXECUTE ", "IMMEDIATE"),
        ("prepare_", "sql"),
    ];
    for source in sources {
        for (a, b) in halves {
            let needle = format!("{a}{b}");
            assert!(
                !source.contains(needle.as_str()),
                "procedure store source must not expose ad-hoc SQL surface token: {needle}"
            );
        }
    }
}
