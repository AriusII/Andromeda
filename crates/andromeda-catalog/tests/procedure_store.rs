//! Integration tests for the Procedure Store scaffold.
//!
//! Validates that:
//! - A validated `ProcedureContract` can be projected into a store entry.
//! - Decision evidence (built from `andromeda-observe`'s `DecisionTrace`)
//!   can be attached to invocations and queried back.
//! - The store rejects evidence that does not match the registered binding.

use andromeda_catalog::{
    AccessMode, CatalogObjectRef, CompatibilityPolicy, InvocationDecisionRecord, IsolationPolicy,
    MultiResultPolicy, ObjectKind, ProcedureContract, ProcedureErrorPolicy, ProcedureRegistration,
    ProcedureStore, ProcedureStoreEntry, ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy,
    StatsVersion, TransactionPolicy,
};
use andromeda_core::{CatalogObjectId, CatalogVersion, ContractHash, InvocationId, ProcedureId};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};

fn sample_contract() -> ProcedureContract {
    ProcedureContract {
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
    }
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
        ProcedureId::new(1),
        ContractHash::test_vector(42),
        CatalogVersion::new(7),
        DecisionTrace {
            trace_id: TraceId::new(909),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "v0 inventory reserve-stock contract validated".to_string(),
        },
    )
    .unwrap();

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
    let mut store = ProcedureStore::new();
    store.register(entry).unwrap();

    let mismatched = InvocationDecisionRecord::new(
        InvocationId::new(1),
        ProcedureId::new(1),
        ContractHash::test_vector(99), // wrong contract hash
        CatalogVersion::new(7),
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
