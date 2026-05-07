use super::*;

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
