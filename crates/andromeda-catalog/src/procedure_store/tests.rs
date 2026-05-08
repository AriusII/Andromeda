use super::*;
use crate::{
    AccessMode, CatalogObjectRef, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy,
    ObjectKind, PolicyVersion, ProcedureContractBinding, ProcedureErrorPolicy, ProcedureFeedback,
    ProtocolLayoutRef, QualifiedName, RecordOutcome, ResultMetadataPolicy, StatsVersion,
    TransactionPolicy,
};
use andromeda_error::AndromedaErrorKind;
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_time::EngineTimestamp;
use andromeda_types::{CatalogObjectId, CatalogVersion, ContractHash, InvocationId, ProcedureId};

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

fn make_window(issued: u64, expires: u64) -> crate::ValidityWindow {
    crate::ValidityWindow::new(
        andromeda_time::EngineTimestamp::from_unix_millis(issued),
        andromeda_time::EngineTimestamp::from_unix_millis(expires),
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

/// Documents the passive invocation history sink shape for non-committed
/// terminal outcomes. Failed invocations may retain plan and counter evidence;
/// aborted invocations represent pre-transaction failures and stay counter-free.
#[test]
fn invocation_history_sink_records_failed_and_pre_transaction_aborted_outcomes() {
    let mut store = ProcedureStore::new();
    let registered = entry(1, "Inventory.ReserveStock", 42);
    let binding = registered.binding;
    store.register(registered).unwrap();

    let failed = InvocationRuntimeRecord::new_with_decision_trace(
        InvocationId::new(901),
        binding,
        EngineTimestamp::from_unix_millis(1_000),
        EngineTimestamp::from_unix_millis(1_017),
        TraceId::new(9_901),
        Some(plan_key_for(&binding)),
        Some(plan_id_for(&binding)),
        ProcedureRuntimeCounters::new(3, 0, 0, 0).with_rows_returned(1),
        ProcedureRuntimeStatus::Failed,
        Some(AndromedaErrorKind::Execution),
    )
    .expect("failed terminal evidence is a valid invocation history record");
    let aborted = InvocationRuntimeRecord::new(
        InvocationId::new(902),
        binding,
        EngineTimestamp::from_unix_millis(1_020),
        EngineTimestamp::from_unix_millis(1_021),
        None,
        None,
        ProcedureRuntimeCounters::new(0, 0, 0, 0),
        ProcedureRuntimeStatus::Aborted,
        Some(AndromedaErrorKind::Security),
    )
    .expect("aborted pre-transaction evidence is a valid empty-counter record");

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
        .invocation_runtime_for_invocation(InvocationId::new(901))
        .expect("failed outcome is indexed by invocation id");
    assert_eq!(failed.status, ProcedureRuntimeStatus::Failed);
    assert_eq!(failed.error_kind, Some(AndromedaErrorKind::Execution));
    assert_eq!(failed.decision_trace_id, Some(TraceId::new(9_901)));
    assert!(failed.plan_key.is_some());
    assert!(failed.plan_id.is_some());
    assert!(!failed.counters.is_empty());
    assert!(failed.is_terminal());
    assert!(failed.is_observed_feedback());
    assert!(!failed.is_authoritative_decision());
    assert!(!failed.can_select_plan_alone());

    let aborted = store
        .invocation_runtime_for_invocation(InvocationId::new(902))
        .expect("aborted pre-transaction outcome is indexed by invocation id");
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

mod contracts;
mod decision_evidence;
mod feedback;
mod guardrails;
mod registration;
mod runtime_evidence;
