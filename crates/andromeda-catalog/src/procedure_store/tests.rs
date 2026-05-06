use super::*;
use crate::{
    AccessMode, CatalogObjectRef, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy,
    ObjectKind, PolicyVersion, ProcedureContractBinding, ProcedureErrorPolicy, ProcedureFeedback,
    ProtocolLayoutRef, QualifiedName, RecordOutcome, ResultMetadataPolicy, StatsVersion,
    TransactionPolicy,
};
use andromeda_core::{CatalogObjectId, CatalogVersion, ContractHash, InvocationId, ProcedureId};
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
    InvocationDecisionRecord::new(
        InvocationId::new(invocation),
        ProcedureId::new(procedure_id),
        ContractHash::test_vector(hash_seed),
        CatalogVersion::new(7),
        DecisionTrace {
            trace_id: TraceId::new(invocation as u128),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "contract validated against registered binding".to_string(),
        },
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
    record.catalog_version = CatalogVersion::new(8);
    let err = store.attach_invocation_decision(record).unwrap_err();
    assert_eq!(err.kind(), andromeda_core::AndromedaErrorKind::Contract);
    assert!(err.message().contains("catalog version"));
}

#[test]
fn decision_evidence_requires_non_empty_reason() {
    let bad = InvocationDecisionRecord::new(
        InvocationId::new(101),
        ProcedureId::new(1),
        ContractHash::test_vector(42),
        CatalogVersion::new(7),
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
fn store_round_trips_a_validated_procedure_contract() {
    let contract = crate::ProcedureContract {
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

    let entry = ProcedureStoreEntry::from_contract(&contract).unwrap();
    let mut store = ProcedureStore::new();
    assert_eq!(
        store.register(entry).unwrap(),
        ProcedureRegistration::Inserted
    );
    assert_eq!(
        store.get(ProcedureId::new(1)).unwrap().contract_hash(),
        ContractHash::test_vector(42)
    );
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
