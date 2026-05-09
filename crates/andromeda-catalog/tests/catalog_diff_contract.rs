#![forbid(unsafe_code)]

use andromeda_catalog_diff::{
    CatalogObjectDiffImpact, CatalogObjectDiffKind, CatalogObjectDiffSeverity,
    diff_catalog_object_definitions,
};
use andromeda_catalog_store::{CatalogDefinition, CatalogObjectRef, ObjectKind, QualifiedName};
use andromeda_procedure_contract::{
    AccessMode, CompatibilityPolicy, ContractCompatibilityDiagnostic, IsolationPolicy,
    MultiResultPolicy, ProcedureContract, ProcedureContractCandidate, ProcedureErrorPolicy,
    ProtocolLayoutRef, ResultMetadataPolicy, ResultStreamCardinality, ResultStreamContract,
    StatsVersion, TransactionPolicy,
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

fn assert_has_message(diagnostic: &ContractCompatibilityDiagnostic, expected: &str) {
    assert!(
        diagnostic
            .messages
            .iter()
            .any(|message| message.contains(expected)),
        "expected diagnostic to contain {expected:?}; got {:?}",
        diagnostic.messages
    );
}

#[test]
fn additive_result_stream_column_append_is_current_catalog_diff_evidence() {
    let previous = materialize(candidate(1, CompatibilityPolicy::AdditiveOnly));
    let mut next_candidate = candidate(2, CompatibilityPolicy::AdditiveOnly);
    next_candidate.result_streams = vec![rows(vec![
        column("ProductId", 0),
        column("QuantityAvailable", 1),
    ])];
    let next = materialize(next_candidate);

    assert_eq!(previous.procedure_id, next.procedure_id);
    assert_eq!(previous.object.object_id, next.object.object_id);
    assert_eq!(previous.object.name, next.object.name);
    assert!(next.object.catalog_version > previous.object.catalog_version);
    assert_ne!(
        previous.contract_hash, next.contract_hash,
        "catalog diff evidence must notice an additive output-shape change"
    );
    let previous_definition = CatalogDefinition::Procedure(previous.clone());
    let next_definition = CatalogDefinition::Procedure(next.clone());
    let diff = diff_catalog_object_definitions(Some(&previous_definition), Some(&next_definition))
        .expect("procedure shape change must produce catalog diff evidence");

    assert_eq!(diff.kind, CatalogObjectDiffKind::Replaced);
    assert_eq!(diff.severity, CatalogObjectDiffSeverity::WalRequired);
    assert!(diff.requires_durable_wal());
    assert!(
        diff.impacts
            .contains(&CatalogObjectDiffImpact::CatalogVersionChanged)
    );
    assert!(
        diff.impacts
            .contains(&CatalogObjectDiffImpact::ContractHashChanged)
    );

    let diagnostic = next.compatibility_with(&previous);
    assert!(
        diagnostic.compatible,
        "appended result columns should remain additive under AdditiveOnly: {:?}",
        diagnostic.messages
    );
    assert!(diagnostic.messages.is_empty());
    assert!(previous.validated_binding().is_ok());
    assert!(next.validated_binding().is_ok());
}

#[test]
fn breaking_input_shape_change_reports_compatibility_diagnostic() {
    let previous = materialize(candidate(1, CompatibilityPolicy::AdditiveOnly));
    let mut next_candidate = candidate(2, CompatibilityPolicy::AdditiveOnly);
    next_candidate.inputs = vec![column("ProductId", 0), column("Quantity", 1)];
    let next = materialize(next_candidate);

    assert_ne!(
        previous.contract_hash, next.contract_hash,
        "input-shape changes must produce different contract evidence"
    );
    let previous_definition = CatalogDefinition::Procedure(previous.clone());
    let next_definition = CatalogDefinition::Procedure(next.clone());
    let diff = diff_catalog_object_definitions(Some(&previous_definition), Some(&next_definition))
        .expect("breaking procedure shape change must produce catalog diff evidence");

    assert_eq!(diff.kind, CatalogObjectDiffKind::Replaced);
    assert_eq!(diff.severity, CatalogObjectDiffSeverity::WalRequired);
    assert!(
        diff.impacts
            .contains(&CatalogObjectDiffImpact::CatalogVersionChanged)
    );
    assert!(
        diff.impacts
            .contains(&CatalogObjectDiffImpact::ContractHashChanged)
    );

    let diagnostic = next.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert_has_message(&diagnostic, "input changes");
}
