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

mod contracts;
mod decision_evidence;
mod feedback;
mod guardrails;
mod registration;
mod runtime_evidence;
