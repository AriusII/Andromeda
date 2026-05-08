use andromeda_contract::{
    AccessMode, CatalogObjectRef, CompatibilityPolicy, IsolationPolicy, MultiResultPolicy,
    ObjectKind, ProcedureContract, ProcedureContractCandidate, ProcedureErrorPolicy,
    ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy, ResultStreamCardinality,
    ResultStreamContract, StatsVersion, TransactionPolicy,
};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId, ScalarType,
    TypeDescriptor,
};

const EXECUTE_PERMISSION: &str = "Inventory.ReserveStock.Execute";
const INSUFFICIENT_STOCK: &str = "InsufficientStock";

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal,
    }
}

fn object(version: u64) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(1),
        name: QualifiedName::parse("Inventory.ReserveStock").expect("valid qualified name"),
        kind: ObjectKind::Procedure,
        catalog_version: CatalogVersion::new(version),
    }
}

fn result_stream(
    stream_id: u64,
    name: &str,
    columns: Vec<ColumnDescriptor>,
    cardinality: ResultStreamCardinality,
) -> ResultStreamContract {
    ResultStreamContract {
        stream_id,
        name: name.to_string(),
        columns,
        cardinality,
        row_count_exact_required: cardinality.legacy_row_count_exact_required(),
    }
}

fn rows(columns: Vec<ColumnDescriptor>) -> ResultStreamContract {
    result_stream(1, "Rows", columns, ResultStreamCardinality::Many)
}

fn candidate(
    version: u64,
    compatibility_policy: CompatibilityPolicy,
) -> ProcedureContractCandidate {
    ProcedureContractCandidate {
        object: object(version),
        procedure_id: ProcedureId::new(1),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xA1),
            frame_envelope_hash: ContractHash::test_vector(0xA2),
        },
        inputs: vec![column("ProductId", 0)],
        structured_inputs: Vec::new(),
        result_streams: vec![rows(vec![column("ProductId", 0)])],
        required_permissions: vec![EXECUTE_PERMISSION.to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        compatibility_policy,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: vec![INSUFFICIENT_STOCK.to_string()],
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    }
}

fn materialize(candidate: ProcedureContractCandidate) -> ProcedureContract {
    candidate
        .materialize()
        .expect("contract fixture must materialize")
}

fn assert_has_message(
    diagnostic: &andromeda_contract::ContractCompatibilityDiagnostic,
    text: &str,
) {
    assert!(
        diagnostic
            .messages
            .iter()
            .any(|message| message.contains(text)),
        "expected diagnostic to contain {text:?}; got {:?}",
        diagnostic.messages
    );
}

#[test]
fn exact_hash_compatibility_rejects_rehashed_shape_change() {
    let previous = materialize(candidate(1, CompatibilityPolicy::ExactHash));
    let mut next_candidate = candidate(2, CompatibilityPolicy::ExactHash);
    next_candidate.result_streams = vec![rows(vec![column("ProductId", 0), column("Quantity", 1)])];
    let next = materialize(next_candidate);

    assert_ne!(previous.contract_hash, next.contract_hash);

    let diagnostic = next.compatibility_with(&previous);

    assert!(!diagnostic.compatible);
    assert_has_message(
        &diagnostic,
        "exact-hash compatibility requires unchanged contract hash",
    );
}

#[test]
fn additive_compatibility_accepts_appended_result_stream_columns() {
    let previous = materialize(candidate(1, CompatibilityPolicy::AdditiveOnly));
    let mut next_candidate = candidate(2, CompatibilityPolicy::AdditiveOnly);
    next_candidate.result_streams = vec![rows(vec![column("ProductId", 0), column("Quantity", 1)])];
    let next = materialize(next_candidate);

    assert_ne!(previous.contract_hash, next.contract_hash);

    let diagnostic = next.compatibility_with(&previous);

    assert!(diagnostic.compatible, "{:?}", diagnostic.messages);
    assert!(diagnostic.messages.is_empty());
}

#[test]
fn additive_compatibility_rejects_input_shape_changes() {
    let previous = materialize(candidate(1, CompatibilityPolicy::AdditiveOnly));
    let mut next_candidate = candidate(2, CompatibilityPolicy::AdditiveOnly);
    next_candidate.inputs = vec![column("ProductId", 0), column("Quantity", 1)];
    let next = materialize(next_candidate);

    let diagnostic = next.compatibility_with(&previous);

    assert!(!diagnostic.compatible);
    assert_has_message(
        &diagnostic,
        "additive compatibility does not permit input changes",
    );
}

#[test]
fn additive_compatibility_rejects_structured_input_shape_changes() {
    let previous = materialize(candidate(1, CompatibilityPolicy::AdditiveOnly));
    let mut next_candidate = candidate(2, CompatibilityPolicy::AdditiveOnly);
    next_candidate.structured_inputs =
        vec![QualifiedName::parse("Inventory.Reservation").expect("valid qualified name")];
    let next = materialize(next_candidate);

    let diagnostic = next.compatibility_with(&previous);

    assert!(!diagnostic.compatible);
    assert_has_message(
        &diagnostic,
        "additive compatibility does not permit structured input changes",
    );
}

#[test]
fn additive_compatibility_rejects_existing_result_column_reorder() {
    let mut previous_candidate = candidate(1, CompatibilityPolicy::AdditiveOnly);
    previous_candidate.result_streams =
        vec![rows(vec![column("ProductId", 0), column("Quantity", 1)])];
    let previous = materialize(previous_candidate);

    let mut next_candidate = candidate(2, CompatibilityPolicy::AdditiveOnly);
    next_candidate.result_streams = vec![rows(vec![column("Quantity", 0), column("ProductId", 1)])];
    let next = materialize(next_candidate);

    let diagnostic = next.compatibility_with(&previous);

    assert!(!diagnostic.compatible);
    assert_has_message(
        &diagnostic,
        "additive compatibility requires existing columns to remain an unchanged prefix",
    );
}

#[test]
fn additive_compatibility_rejects_result_stream_removal() {
    let previous = materialize(candidate(1, CompatibilityPolicy::AdditiveOnly));
    let mut next_candidate = candidate(2, CompatibilityPolicy::AdditiveOnly);
    next_candidate.result_streams = Vec::new();
    let next = materialize(next_candidate);

    let diagnostic = next.compatibility_with(&previous);

    assert!(!diagnostic.compatible);
    assert_has_message(
        &diagnostic,
        "additive compatibility does not permit removing result stream Rows",
    );
}

#[test]
fn additive_compatibility_rejects_result_stream_id_change() {
    let previous = materialize(candidate(1, CompatibilityPolicy::AdditiveOnly));
    let mut next_candidate = candidate(2, CompatibilityPolicy::AdditiveOnly);
    next_candidate.result_streams = vec![result_stream(
        2,
        "Rows",
        vec![column("ProductId", 0)],
        ResultStreamCardinality::Many,
    )];
    let next = materialize(next_candidate);

    let diagnostic = next.compatibility_with(&previous);

    assert!(!diagnostic.compatible);
    assert_has_message(
        &diagnostic,
        "additive compatibility does not permit changing result stream id for result stream Rows",
    );
}

#[test]
fn additive_compatibility_rejects_result_stream_cardinality_change() {
    let previous = materialize(candidate(1, CompatibilityPolicy::AdditiveOnly));
    let mut next_candidate = candidate(2, CompatibilityPolicy::AdditiveOnly);
    next_candidate.result_streams = vec![result_stream(
        1,
        "Rows",
        vec![column("ProductId", 0)],
        ResultStreamCardinality::NonEmptyMany,
    )];
    let next = materialize(next_candidate);

    let diagnostic = next.compatibility_with(&previous);

    assert!(!diagnostic.compatible);
    assert_has_message(
        &diagnostic,
        "additive compatibility does not permit changing cardinality contract for result stream Rows",
    );
}
