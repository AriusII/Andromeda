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
const AUDIT_PERMISSION: &str = "Inventory.ReserveStock.Audit";
const INSUFFICIENT_STOCK: &str = "InsufficientStock";
const TIMEOUT: &str = "Timeout";

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

fn candidate_with_policy(
    version: u64,
    required_permissions: &[&str],
    allowed_error_codes: &[&str],
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
        result_streams: Vec::new(),
        required_permissions: required_permissions
            .iter()
            .map(|permission| (*permission).to_string())
            .collect(),
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        compatibility_policy: CompatibilityPolicy::ExactHash,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: allowed_error_codes
                .iter()
                .map(|code| (*code).to_string())
                .collect(),
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    }
}

fn materialize(candidate: ProcedureContractCandidate) -> ProcedureContract {
    candidate
        .materialize()
        .expect("contract fixture must materialize")
}

fn contract_with_policy(
    version: u64,
    required_permissions: &[&str],
    allowed_error_codes: &[&str],
) -> ProcedureContract {
    materialize(candidate_with_policy(
        version,
        required_permissions,
        allowed_error_codes,
    ))
}

fn contract(version: u64) -> ProcedureContract {
    contract_with_policy(version, &[EXECUTE_PERMISSION], &[INSUFFICIENT_STOCK])
}

fn contract_with_shape(
    version: u64,
    inputs: Vec<ColumnDescriptor>,
    result_streams: Vec<ResultStreamContract>,
) -> ProcedureContract {
    let mut candidate =
        candidate_with_policy(version, &[EXECUTE_PERMISSION], &[INSUFFICIENT_STOCK]);
    candidate.inputs = inputs;
    candidate.result_streams = result_streams;
    materialize(candidate)
}

fn contract_with_stats_version(stats_version: StatsVersion) -> ProcedureContract {
    let mut candidate = candidate_with_policy(1, &[EXECUTE_PERMISSION], &[INSUFFICIENT_STOCK]);
    candidate.stats_version = stats_version;
    materialize(candidate)
}

fn result_stream(name: &str, columns: Vec<ColumnDescriptor>) -> ResultStreamContract {
    let cardinality = ResultStreamCardinality::Many;
    ResultStreamContract {
        stream_id: 1,
        name: name.to_string(),
        columns,
        cardinality,
        row_count_exact_required: cardinality.legacy_row_count_exact_required(),
    }
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
fn exact_hash_compatibility_accepts_canonical_advancing_contracts() {
    let previous = contract(1);
    let next = contract(2);

    assert_eq!(previous.contract_hash, next.contract_hash);
    assert_eq!(next.contract_hash, next.canonical_hash());

    let diagnostic = next.compatibility_with(&previous);

    assert!(diagnostic.compatible, "{:?}", diagnostic.messages);
    assert!(diagnostic.messages.is_empty());
}

#[test]
fn catalog_version_advancement_changes_binding_but_not_canonical_contract_hash() {
    let previous = contract(1);
    let next = contract(2);

    assert_eq!(previous.contract_hash, next.contract_hash);
    assert_eq!(previous.canonical_hash(), next.canonical_hash());
    assert_eq!(previous.policy_version(), next.policy_version());
    assert_ne!(
        previous.binding().catalog_version,
        next.binding().catalog_version
    );
}

#[test]
fn stats_version_participates_in_contract_hash_and_policy_version() {
    let baseline = contract_with_stats_version(StatsVersion::new(1));
    let changed = contract_with_stats_version(StatsVersion::new(2));

    assert_ne!(baseline.contract_hash, changed.contract_hash);
    assert_ne!(baseline.policy_version(), changed.policy_version());
}

#[test]
fn compatibility_rejects_next_contract_with_stale_hash() {
    let previous = contract(1);
    let mut next = contract(2);
    next.required_permissions.push(AUDIT_PERMISSION.to_string());

    assert_eq!(
        previous.contract_hash, next.contract_hash,
        "test fixture must keep the stale stored hash that ExactHash would otherwise accept"
    );
    assert_ne!(next.contract_hash, next.canonical_hash());

    let diagnostic = next.compatibility_with(&previous);

    assert!(!diagnostic.compatible);
    assert_has_message(&diagnostic, "next contract is invalid or non-canonical");
    assert_has_message(
        &diagnostic,
        "procedure contract hash must match canonical contract shape",
    );
}

#[test]
fn compatibility_rejects_previous_contract_with_stale_hash() {
    let mut previous = contract(1);
    previous
        .error_policy
        .allowed_error_codes
        .push(TIMEOUT.to_string());
    let next = contract(2);

    assert_eq!(
        previous.contract_hash, next.contract_hash,
        "test fixture must keep the stale stored hash that ExactHash would otherwise accept"
    );
    assert_ne!(previous.contract_hash, previous.canonical_hash());

    let diagnostic = next.compatibility_with(&previous);

    assert!(!diagnostic.compatible);
    assert_has_message(&diagnostic, "previous contract is invalid or non-canonical");
    assert_has_message(
        &diagnostic,
        "procedure contract hash must match canonical contract shape",
    );
}

#[test]
fn permission_order_participates_in_contract_hash_and_policy_version() {
    let baseline = contract_with_policy(
        1,
        &[EXECUTE_PERMISSION, AUDIT_PERMISSION],
        &[INSUFFICIENT_STOCK],
    );
    let repeated = contract_with_policy(
        1,
        &[EXECUTE_PERMISSION, AUDIT_PERMISSION],
        &[INSUFFICIENT_STOCK],
    );
    let reordered = contract_with_policy(
        1,
        &[AUDIT_PERMISSION, EXECUTE_PERMISSION],
        &[INSUFFICIENT_STOCK],
    );

    assert_eq!(baseline.contract_hash, repeated.contract_hash);
    assert_eq!(baseline.policy_version(), repeated.policy_version());
    assert_ne!(baseline.contract_hash, reordered.contract_hash);
    assert_ne!(baseline.policy_version(), reordered.policy_version());
}

#[test]
fn error_code_order_participates_in_contract_hash_and_policy_version() {
    let baseline = contract_with_policy(1, &[EXECUTE_PERMISSION], &[INSUFFICIENT_STOCK, TIMEOUT]);
    let repeated = contract_with_policy(1, &[EXECUTE_PERMISSION], &[INSUFFICIENT_STOCK, TIMEOUT]);
    let reordered = contract_with_policy(1, &[EXECUTE_PERMISSION], &[TIMEOUT, INSUFFICIENT_STOCK]);

    assert_eq!(baseline.contract_hash, repeated.contract_hash);
    assert_eq!(baseline.policy_version(), repeated.policy_version());
    assert_ne!(baseline.contract_hash, reordered.contract_hash);
    assert_ne!(baseline.policy_version(), reordered.policy_version());
}

#[test]
fn input_field_order_participates_in_contract_hash_but_not_policy_version() {
    let baseline = contract_with_shape(
        1,
        vec![column("ProductId", 0), column("Quantity", 1)],
        Vec::new(),
    );
    let repeated = contract_with_shape(
        1,
        vec![column("ProductId", 0), column("Quantity", 1)],
        Vec::new(),
    );
    let reordered = contract_with_shape(
        1,
        vec![column("Quantity", 0), column("ProductId", 1)],
        Vec::new(),
    );

    assert_eq!(baseline.contract_hash, repeated.contract_hash);
    assert_eq!(baseline.policy_version(), repeated.policy_version());
    assert_ne!(baseline.contract_hash, reordered.contract_hash);
    assert_eq!(baseline.policy_version(), reordered.policy_version());
}

#[test]
fn result_stream_field_order_participates_in_contract_hash_but_not_policy_version() {
    let baseline = contract_with_shape(
        1,
        vec![column("ProductId", 0)],
        vec![result_stream(
            "Rows",
            vec![column("ProductId", 0), column("Quantity", 1)],
        )],
    );
    let repeated = contract_with_shape(
        1,
        vec![column("ProductId", 0)],
        vec![result_stream(
            "Rows",
            vec![column("ProductId", 0), column("Quantity", 1)],
        )],
    );
    let reordered = contract_with_shape(
        1,
        vec![column("ProductId", 0)],
        vec![result_stream(
            "Rows",
            vec![column("Quantity", 0), column("ProductId", 1)],
        )],
    );

    assert_eq!(baseline.contract_hash, repeated.contract_hash);
    assert_eq!(baseline.policy_version(), repeated.policy_version());
    assert_ne!(baseline.contract_hash, reordered.contract_hash);
    assert_eq!(baseline.policy_version(), reordered.policy_version());
}
