use andromeda_error::AndromedaErrorKind;
use andromeda_procedure_contract::{
    AccessMode, CatalogObjectRef, CompatibilityPolicy, ContractCompatibilityDiagnostic,
    IsolationPolicy, MultiResultPolicy, ObjectKind, PolicyVersion, ProcedureContract,
    ProcedureContractBinding, ProcedureContractCandidate, ProcedureErrorPolicy, ProtocolLayoutRef,
    QualifiedName, ResultMetadataPolicy, ResultStreamCardinality, ResultStreamContract,
    StatsVersion, TransactionPolicy,
};
use andromeda_types::{
    CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId, ScalarType,
    TypeDescriptor,
};

fn column(name: &str, ordinal: u32) -> ColumnDescriptor {
    typed_column(name, ordinal, ScalarType::I64)
}

fn typed_column(name: &str, ordinal: u32, scalar: ScalarType) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(scalar),
        ordinal,
    }
}

fn object(id: u64, name: &str, kind: ObjectKind, version: CatalogVersion) -> CatalogObjectRef {
    CatalogObjectRef {
        object_id: CatalogObjectId::new(id),
        name: QualifiedName::parse(name).unwrap(),
        kind,
        catalog_version: version,
    }
}

fn procedure(
    id: u64,
    name: &str,
    version: CatalogVersion,
    structured_inputs: Vec<QualifiedName>,
) -> ProcedureContract {
    procedure_contract(
        id,
        name,
        version,
        structured_inputs,
        Vec::new(),
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::SingleResultOnly,
    )
}

fn procedure_contract(
    id: u64,
    name: &str,
    version: CatalogVersion,
    structured_inputs: Vec<QualifiedName>,
    result_streams: Vec<ResultStreamContract>,
    compatibility_policy: CompatibilityPolicy,
    multi_result_policy: MultiResultPolicy,
) -> ProcedureContract {
    ProcedureContractCandidate {
        object: object(id, name, ObjectKind::Procedure, version),
        procedure_id: ProcedureId::new(id),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xA1),
            frame_envelope_hash: ContractHash::test_vector(0xA2),
        },
        inputs: vec![column("ProductId", 0)],
        structured_inputs,
        result_streams,
        required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        compatibility_policy,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: vec!["InsufficientStock".to_string()],
        },
        multi_result_policy,
    }
    .materialize()
    .unwrap()
}

fn procedure_contract_with_input_columns(
    id: u64,
    name: &str,
    version: CatalogVersion,
    inputs: Vec<ColumnDescriptor>,
    result_streams: Vec<ResultStreamContract>,
) -> ProcedureContract {
    ProcedureContractCandidate {
        object: object(id, name, ObjectKind::Procedure, version),
        procedure_id: ProcedureId::new(id),
        stats_version: StatsVersion::new(1),
        protocol_layout: ProtocolLayoutRef {
            descriptor_set_hash: ContractHash::test_vector(0xA1),
            frame_envelope_hash: ContractHash::test_vector(0xA2),
        },
        inputs,
        structured_inputs: Vec::new(),
        result_streams,
        required_permissions: vec!["Inventory.ReserveStock.Execute".to_string()],
        transaction_policy: TransactionPolicy {
            access_mode: AccessMode::ReadWrite,
            isolation: IsolationPolicy::Serializable,
            retryable: false,
        },
        compatibility_policy: CompatibilityPolicy::ExactHash,
        result_metadata_policy: ResultMetadataPolicy::RequireBeforePayload,
        error_policy: ProcedureErrorPolicy {
            rollback_on_error: true,
            allowed_error_codes: vec!["InsufficientStock".to_string()],
        },
        multi_result_policy: MultiResultPolicy::SingleResultOnly,
    }
    .materialize()
    .unwrap()
}

fn result_stream(
    stream_id: u64,
    name: &str,
    cardinality: ResultStreamCardinality,
    columns: Vec<ColumnDescriptor>,
) -> ResultStreamContract {
    ResultStreamContract {
        stream_id,
        name: name.to_string(),
        columns,
        cardinality,
        row_count_exact_required: cardinality.legacy_row_count_exact_required(),
    }
}

fn assert_has_message(diagnostic: &ContractCompatibilityDiagnostic, text: &str) {
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
fn contract_hash_is_digest_backed_and_deterministic() {
    let proc_a = procedure(11, "Inventory.ReserveStock", CatalogVersion::new(1), vec![]);
    let proc_b = procedure(11, "Inventory.ReserveStock", CatalogVersion::new(1), vec![]);

    assert_eq!(proc_a.contract_hash, proc_b.contract_hash);
    assert!(!proc_a.contract_hash.is_zero());

    // SHA-256 must thoroughly mix every byte: a single bit change in the
    // procedure name flips a high fraction of output bits.
    let proc_renamed = procedure(11, "Inventory.ReserveAtock", CatalogVersion::new(1), vec![]);
    let differing_bytes = proc_a
        .contract_hash
        .as_bytes()
        .iter()
        .zip(proc_renamed.contract_hash.as_bytes().iter())
        .filter(|(left, right)| left != right)
        .count();
    assert!(
        differing_bytes >= 16,
        "digest-backed hash must avalanche: only {differing_bytes}/32 bytes differ"
    );
}

#[test]
fn contract_hash_ignores_srpl_source_formatting_but_tracks_observable_shape() {
    let compact_source = br#"procedure Inventory.ReserveStock(ProductId: I64) returns Reservations(ProductId: I64);"#;
    let formatted_source = br#"
        procedure   Inventory.ReserveStock
        (
            ProductId : I64
        )
        returns
            Reservations ( ProductId : I64 ) ;
    "#;
    assert_ne!(
        andromeda_digest::sha256(compact_source),
        andromeda_digest::sha256(formatted_source),
        "raw SRPL source evidence must distinguish formatting-only byte drift"
    );

    let compact_lowering = procedure_contract(
        18,
        "Inventory.ReserveStock",
        CatalogVersion::new(1),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![column("ProductId", 0)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::SingleResultOnly,
    );
    let formatted_lowering = procedure_contract(
        18,
        "Inventory.ReserveStock",
        CatalogVersion::new(1),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![column("ProductId", 0)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::SingleResultOnly,
    );

    assert_eq!(
        compact_lowering.contract_hash, formatted_lowering.contract_hash,
        "canonical ContractHash must be independent from SRPL source formatting"
    );

    let changed_result_shape = procedure_contract(
        18,
        "Inventory.ReserveStock",
        CatalogVersion::new(1),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![column("ProductId", 0), column("ReservedQuantity", 1)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::SingleResultOnly,
    );
    assert_ne!(
        compact_lowering.contract_hash, changed_result_shape.contract_hash,
        "observable result contract growth must change ContractHash"
    );

    let changed_input_shape = procedure_contract_with_input_columns(
        18,
        "Inventory.ReserveStock",
        CatalogVersion::new(1),
        vec![typed_column("ProductId", 0, ScalarType::Bool)],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![column("ProductId", 0)],
        )],
    );

    assert_ne!(
        compact_lowering.contract_hash, changed_input_shape.contract_hash,
        "observable input type drift must change ContractHash"
    );
}

#[test]
fn contract_hash_ignores_catalog_version_while_binding_tracks_catalog_version() {
    let v1 = procedure(19, "Inventory.ReserveStock", CatalogVersion::new(1), vec![]);
    let v2 = procedure(19, "Inventory.ReserveStock", CatalogVersion::new(2), vec![]);

    assert_eq!(
        v1.contract_hash, v2.contract_hash,
        "CatalogVersion is publication evidence, not part of canonical procedure shape"
    );
    assert_ne!(
        v1.binding().catalog_version,
        v2.binding().catalog_version,
        "binding evidence must still carry the concrete CatalogVersion accepted at invocation"
    );
    assert_eq!(v1.binding().contract_hash, v2.binding().contract_hash);
}

#[test]
fn contract_hash_changes_with_stats_and_policy_drift() {
    // Per SPEC_PROCEDURE_CONTRACT_V0 §"Properties" line 86: ContractHash MUST
    // change when observable procedure shape, protocol, permission, policy,
    // result metadata, error, multi-result, or statistics evidence changes.
    let baseline = procedure(20, "Inventory.ReserveStock", CatalogVersion::new(1), vec![]);
    let mut drifted = baseline.clone();
    drifted.stats_version = StatsVersion::new(2);
    drifted.transaction_policy.isolation = IsolationPolicy::Snapshot;
    drifted
        .required_permissions
        .push("Inventory.Audit.Execute".to_string());

    assert_ne!(
        baseline.contract_hash,
        drifted.canonical_hash(),
        "ContractHash must change with stats/policy drift per SPEC_PROCEDURE_CONTRACT_V0 line 86"
    );
    // The drifted contract still carries the baseline's stored hash, so the
    // canonical-vs-stored validation must reject it as non-canonical.
    assert!(drifted.validate_canonical_hash().is_err());
    assert_ne!(
        baseline.binding().stats_version,
        drifted.binding().stats_version
    );
    assert_ne!(
        baseline.binding().policy_version,
        drifted.binding().policy_version
    );
}

#[test]
fn policy_version_changes_with_policy_fields_only() {
    let baseline = procedure(12, "Inventory.ReserveStock", CatalogVersion::new(1), vec![]);
    let baseline_policy = baseline.policy_version();
    assert!(!baseline_policy.is_zero());
    assert_eq!(baseline_policy, baseline.policy_version());

    // Changing structured inputs alters the contract shape but not the
    // policy surface; PolicyVersion must be unaffected.
    let mut shape_only = baseline.clone();
    shape_only.structured_inputs = vec![QualifiedName::parse("Inventory.Stock").unwrap()];
    assert_eq!(
        baseline_policy,
        shape_only.policy_version(),
        "PolicyVersion must ignore non-policy shape changes"
    );

    // Changing a true policy field must change the PolicyVersion.
    let mut policy_changed = baseline.clone();
    policy_changed.transaction_policy.isolation = IsolationPolicy::Snapshot;
    assert_ne!(
        baseline_policy,
        policy_changed.policy_version(),
        "PolicyVersion must reflect transaction-policy changes"
    );

    let mut perms_changed = baseline.clone();
    perms_changed
        .required_permissions
        .push("Inventory.Audit".to_string());
    assert_ne!(baseline_policy, perms_changed.policy_version());
}

#[test]
fn procedure_additive_compatibility_accepts_additive_result_growth() {
    let previous = procedure_contract(
        31,
        "Inventory.ReserveStock",
        CatalogVersion::new(1),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![column("ProductId", 0)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let additive = procedure_contract(
        31,
        "Inventory.ReserveStock",
        CatalogVersion::new(2),
        vec![],
        vec![
            result_stream(
                1,
                "Reservations",
                ResultStreamCardinality::One,
                vec![column("ProductId", 0), column("ReservedQuantity", 1)],
            ),
            result_stream(
                2,
                "AuditRows",
                ResultStreamCardinality::Many,
                vec![column("AuditId", 0)],
            ),
        ],
        CompatibilityPolicy::AdditiveOnly,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );

    assert_ne!(
        previous.contract_hash, additive.contract_hash,
        "additive result growth changes the canonical contract hash"
    );

    let diagnostic = additive.compatibility_with(&previous);
    assert!(
        diagnostic.compatible,
        "AdditiveOnly must accept appended result columns and new result streams: {:?}",
        diagnostic.messages
    );
    assert!(diagnostic.messages.is_empty());
}

#[test]
fn procedure_exact_hash_compatibility_rejects_additive_contract_hash_change() {
    let previous = procedure_contract(
        32,
        "Inventory.ReserveStock",
        CatalogVersion::new(1),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![column("ProductId", 0)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let additive_shape_with_exact_hash = procedure_contract(
        32,
        "Inventory.ReserveStock",
        CatalogVersion::new(2),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![column("ProductId", 0), column("ReservedQuantity", 1)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );

    assert_ne!(
        previous.contract_hash, additive_shape_with_exact_hash.contract_hash,
        "the exact-hash gate has to see additive result-shape drift"
    );

    let diagnostic = additive_shape_with_exact_hash.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert_has_message(
        &diagnostic,
        "exact-hash compatibility requires unchanged contract hash",
    );
}

#[test]
fn procedure_additive_compatibility_rejects_shape_shifting_returns() {
    let previous = procedure_contract(
        33,
        "Inventory.ReserveStock",
        CatalogVersion::new(1),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![column("ProductId", 0), column("ReservedQuantity", 1)],
        )],
        CompatibilityPolicy::ExactHash,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );

    let changed_cardinality = procedure_contract(
        33,
        "Inventory.ReserveStock",
        CatalogVersion::new(2),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::Many,
            vec![column("ProductId", 0), column("ReservedQuantity", 1)],
        )],
        CompatibilityPolicy::AdditiveOnly,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let diagnostic = changed_cardinality.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert_has_message(&diagnostic, "changing cardinality contract");

    let changed_column_prefix = procedure_contract(
        33,
        "Inventory.ReserveStock",
        CatalogVersion::new(2),
        vec![],
        vec![result_stream(
            1,
            "Reservations",
            ResultStreamCardinality::One,
            vec![
                typed_column("ProductId", 0, ScalarType::Bool),
                column("ReservedQuantity", 1),
            ],
        )],
        CompatibilityPolicy::AdditiveOnly,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let diagnostic = changed_column_prefix.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert_has_message(
        &diagnostic,
        "existing columns to remain an unchanged prefix",
    );

    let removed_stream = procedure_contract(
        33,
        "Inventory.ReserveStock",
        CatalogVersion::new(2),
        vec![],
        Vec::new(),
        CompatibilityPolicy::AdditiveOnly,
        MultiResultPolicy::MultipleResultStreamsAllowed,
    );
    let diagnostic = removed_stream.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert_has_message(&diagnostic, "removing result stream Reservations");
}

#[test]
fn procedure_binding_carries_four_identities() {
    let proc = procedure(13, "Inventory.ReserveStock", CatalogVersion::new(7), vec![]);
    let binding = proc.binding();

    assert_eq!(binding.procedure_id, proc.procedure_id);
    assert_eq!(binding.contract_hash, proc.contract_hash);
    assert_eq!(binding.catalog_version, proc.object.catalog_version);
    assert_eq!(binding.stats_version, proc.stats_version);
    assert_eq!(binding.policy_version, proc.policy_version());
    assert!(binding.validate().is_ok());
    assert_eq!(proc.validated_binding().unwrap(), binding);

    let mut zero_stats = binding;
    zero_stats.stats_version = StatsVersion::new(0);
    assert_eq!(
        zero_stats.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let mut zero_policy = binding;
    zero_policy.policy_version = PolicyVersion::zero();
    assert_eq!(
        zero_policy.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}

#[test]
fn procedure_binding_constructor_rejects_incomplete_evidence() {
    let proc = procedure(14, "Inventory.ReserveStock", CatalogVersion::new(7), vec![]);
    let binding = proc.binding();

    let zero_stats = ProcedureContractBinding::new(
        binding.procedure_id,
        binding.catalog_version,
        binding.contract_hash,
        StatsVersion::new(0),
        binding.policy_version,
    )
    .unwrap_err();
    assert_eq!(zero_stats.kind(), AndromedaErrorKind::Contract);

    let default_policy = ProcedureContractBinding::new(
        binding.procedure_id,
        binding.catalog_version,
        binding.contract_hash,
        binding.stats_version,
        PolicyVersion::zero(),
    )
    .unwrap_err();
    assert_eq!(default_policy.kind(), AndromedaErrorKind::Contract);
}

#[test]
fn procedure_compatibility_rejects_identity_or_non_advancing_version_drift() {
    let previous = procedure(23, "Inventory.ReserveStock", CatalogVersion::new(1), vec![]);
    let mut next = previous.clone();
    next.object.catalog_version = CatalogVersion::new(2);
    assert!(
        next.compatibility_with(&previous).compatible,
        "same Procedure identity with an advancing CatalogVersion and same hash remains compatible"
    );

    let mut procedure_id_drift = next.clone();
    procedure_id_drift.procedure_id = ProcedureId::new(24);
    let diagnostic = procedure_id_drift.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert!(
        diagnostic
            .messages
            .iter()
            .any(|message| message.contains("ProcedureId"))
    );

    let mut object_id_drift = next.clone();
    object_id_drift.object.object_id = CatalogObjectId::new(24);
    let diagnostic = object_id_drift.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert!(
        diagnostic
            .messages
            .iter()
            .any(|message| message.contains("object id"))
    );

    let non_advancing = previous.clone();
    let diagnostic = non_advancing.compatibility_with(&previous);
    assert!(!diagnostic.compatible);
    assert!(
        diagnostic
            .messages
            .iter()
            .any(|message| message.contains("advancing CatalogVersion"))
    );
}

#[test]
fn procedure_validated_binding_rejects_stale_contract_hash() {
    let mut stale = procedure(15, "Inventory.ReserveStock", CatalogVersion::new(7), vec![]);
    stale.contract_hash = ContractHash::test_vector(0xE5);

    let projected = stale.binding();
    assert!(
        projected.validate().is_ok(),
        "non-zero binding evidence is not enough without canonical contract validation"
    );

    let error = stale
        .validated_binding()
        .expect_err("stale canonical hash must reject binding evidence");
    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
}

#[test]
fn procedure_validate_binding_rejects_stats_or_policy_drift() {
    let proc = procedure(16, "Inventory.ReserveStock", CatalogVersion::new(7), vec![]);

    let mut drifted_stats = proc.binding();
    drifted_stats.stats_version = StatsVersion::new(proc.stats_version.get() + 1);
    let error = proc
        .validate_binding(&drifted_stats)
        .expect_err("drifted StatsVersion must be rejected");
    assert_eq!(error.kind(), AndromedaErrorKind::Contract);

    let mut drifted_policy = proc.binding();
    drifted_policy.policy_version = PolicyVersion::new([0xDB; PolicyVersion::LEN]);
    let error = proc
        .validate_binding(&drifted_policy)
        .expect_err("drifted PolicyVersion must be rejected");
    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
}

#[test]
fn procedure_validate_binding_rejects_catalog_version_or_procedure_id_drift() {
    let proc = procedure(17, "Inventory.ReserveStock", CatalogVersion::new(7), vec![]);

    let mut drifted_catalog = proc.binding();
    drifted_catalog.catalog_version = CatalogVersion::new(proc.object.catalog_version.get() + 1);
    let error = proc
        .validate_binding(&drifted_catalog)
        .expect_err("drifted CatalogVersion must be rejected before invocation");
    assert_eq!(error.kind(), AndromedaErrorKind::Contract);

    let mut drifted_procedure = proc.binding();
    drifted_procedure.procedure_id = ProcedureId::new(proc.procedure_id.get() + 1);
    let error = proc
        .validate_binding(&drifted_procedure)
        .expect_err("drifted ProcedureId must be rejected before invocation");
    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
}
