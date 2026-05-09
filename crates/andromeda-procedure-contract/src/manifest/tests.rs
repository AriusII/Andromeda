use super::*;
use andromeda_error::AndromedaErrorKind;
use andromeda_types::{
    CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId, ScalarType, TextEncoding,
    TextType, TypeDescriptor,
};

fn sample_column(name: &str, ordinal: u32) -> ColumnDescriptor {
    ColumnDescriptor {
        name: name.to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal,
    }
}

fn sample_stream() -> ResultStreamDescriptor {
    ResultStreamDescriptor {
        stream_name: "Reservation".to_string(),
        columns: vec![sample_column("ProductId", 0)],
        cardinality: ResultCardinality::ExactlyOne,
        row_count_requirement: RowCountRequirement::ExactRequired,
        row_count_exact: Some(1),
        row_count_max: Some(1),
    }
}

fn sample_manifest() -> ProcedureManifest {
    ProcedureManifest {
        procedure_id: ProcedureId::new(42),
        procedure_name: "Inventory.ReserveStock".to_string(),
        contract_hash: ContractHash::test_vector(0x11),
        catalog_version: CatalogVersion::new(7),
        stats_version: 3,
        policy_version: ManifestPolicyVersion::test_vector(0x22),
        protocol_layout: ProtocolLayout {
            descriptor_set_hash: ContractHash::test_vector(0x33),
            frame_envelope_hash: ContractHash::test_vector(0x44),
        },
        result_streams: vec![sample_stream()],
        required_permissions: vec![RequiredPermission::new(
            "andromeda.execute_procedure",
            "application",
        )],
    }
}

#[test]
fn result_descriptor_validates_cardinality_bounds_and_columns() {
    assert!(sample_stream().validate().is_ok());

    let too_wide = ResultStreamDescriptor {
        row_count_max: Some(2),
        ..sample_stream()
    };
    assert_eq!(
        too_wide.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let mut sparse = sample_stream();
    sparse.columns[0].ordinal = 1;
    assert_eq!(
        sparse.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}

#[test]
fn manifest_hash_is_deterministic_and_field_sensitive() {
    let manifest = sample_manifest();
    assert!(!manifest.manifest_hash().is_zero());
    assert_eq!(manifest.manifest_hash(), manifest.clone().manifest_hash());

    let mut bumped_policy = manifest.clone();
    bumped_policy.policy_version = ManifestPolicyVersion::test_vector(0x55);
    assert_ne!(manifest.manifest_hash(), bumped_policy.manifest_hash());

    let mut bumped_stats = manifest.clone();
    bumped_stats.stats_version += 1;
    assert_ne!(manifest.manifest_hash(), bumped_stats.manifest_hash());
}

#[test]
fn manifest_hash_tracks_column_type_descriptor_semantics() {
    let baseline = sample_manifest();

    let mut changed = baseline.clone();
    changed.result_streams[0].columns[0].data_type =
        TypeDescriptor::optional(ScalarType::Text(TextType {
            encoding: TextEncoding::Utf16,
            max_length: Some(128),
            collation: Some("fr_FR".to_string()),
        }));

    assert_ne!(baseline.manifest_hash(), changed.manifest_hash());
}

#[test]
fn manifest_binding_projection_is_deterministic_and_full_identity() {
    let manifest = sample_manifest();
    let binding = manifest.binding();

    assert_eq!(binding, sample_manifest().binding());

    let mut bumped_catalog = manifest.clone();
    bumped_catalog.catalog_version = CatalogVersion::new(8);
    assert_ne!(binding, bumped_catalog.binding());

    let zero_contract = ProcedureManifest {
        contract_hash: ContractHash::zero(),
        ..manifest
    };
    assert_eq!(
        zero_contract.binding().validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}

#[test]
fn ensure_source_generator_ready_demands_explicit_metadata() {
    let manifest = sample_manifest();
    assert!(manifest.ensure_source_generator_ready().is_ok());

    let no_perms = ProcedureManifest {
        required_permissions: Vec::new(),
        ..manifest
    };
    assert_eq!(
        no_perms.ensure_source_generator_ready().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}

#[test]
fn required_permission_validation_rejects_blank_or_uppercase_ids() {
    assert!(
        RequiredPermission::new("andromeda.execute_procedure", "application")
            .validate()
            .is_ok()
    );
    assert!(
        RequiredPermission::new("", "application")
            .validate()
            .is_err()
    );
    assert!(
        RequiredPermission::new("Andromeda.X", "application")
            .validate()
            .is_err()
    );
}

fn sample_gateway_manifest() -> ProcedureGatewayManifest {
    ProcedureGatewayManifest {
        procedure_id: ProcedureId::new(42),
        procedure_name: "Inventory.ReserveStock".to_string(),
        contract_hash: ContractHash::test_vector(0x11),
        catalog_version: CatalogVersion::new(7),
        protocol_layout: ProcedureGatewayProtocolLayout {
            descriptor_set_hash: ContractHash::test_vector(0x22),
            frame_envelope_hash: ContractHash::test_vector(0x33),
            protocol_package: "andromeda.protocol.v1".to_string(),
            contract_package: "andromeda.contract.v1".to_string(),
        },
        result_streams: Vec::new(),
        stats_version: 3,
        policy_version: ContractHash::test_vector(0x44),
        required_permissions: vec![ProcedureGatewayRequiredPermission {
            id: "andromeda.execute_procedure".to_string(),
            family: "application".to_string(),
        }],
    }
}

#[test]
fn gateway_manifest_requires_canonical_execute_permission() {
    let manifest = sample_gateway_manifest();
    assert!(validate_procedure_gateway_manifest_permissions(&manifest).is_ok());
    assert_eq!(
        required_execute_permission(&manifest).unwrap(),
        andromeda_security_contract::PrincipalPermission::ExecuteProcedure(ProcedureId::new(42))
    );

    let mut missing_execute = sample_gateway_manifest();
    missing_execute.required_permissions.clear();
    let err = validate_procedure_gateway_manifest_permissions(&missing_execute).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("andromeda.execute_procedure"));
}

#[test]
fn gateway_manifest_rejects_non_canonical_execute_permissions() {
    for (id, family) in [
        ("execute_procedure", "application"),
        (" andromeda.execute_procedure ", "application"),
        ("andromeda.execute_procedure", " security "),
        ("andromeda.execute_procedure", "security"),
    ] {
        let mut manifest = sample_gateway_manifest();
        manifest.required_permissions = vec![ProcedureGatewayRequiredPermission {
            id: id.to_string(),
            family: family.to_string(),
        }];

        let err = validate_procedure_gateway_manifest_permissions(&manifest).unwrap_err();

        assert_eq!(
            err.kind(),
            AndromedaErrorKind::Contract,
            "non-canonical execute permission {id:?}/{family:?} must be rejected"
        );
        assert!(err.message().contains("andromeda.execute_procedure"));
    }
}

#[test]
fn gateway_manifest_rejects_privileged_permissions() {
    for (family, id) in [
        ("security", "andromeda.security.manage_security"),
        ("definition", "andromeda.definition.create_procedure"),
        ("cluster", "andromeda.cluster.promote"),
        ("recovery", "andromeda.recovery.restore"),
        ("recovery", "andromeda.recovery.forensic_start"),
    ] {
        let mut manifest = sample_gateway_manifest();
        manifest
            .required_permissions
            .push(ProcedureGatewayRequiredPermission {
                id: id.to_string(),
                family: family.to_string(),
            });

        let err = validate_procedure_gateway_manifest_permissions(&manifest).unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
        assert!(err.message().contains("non-Application permission"));
        assert!(err.message().contains(id));
    }
}
