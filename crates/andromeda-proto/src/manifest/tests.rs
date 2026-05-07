use super::*;
use andromeda_error::AndromedaErrorKind;
use andromeda_types::{
    CatalogVersion, ColumnDescriptor, ContractHash, ProcedureId, ScalarType, TypeDescriptor,
};

#[test]
fn result_descriptor_validates_columns() {
    let descriptor = ResultStreamDescriptor {
        stream_name: "Reservation".to_string(),
        columns: vec![ColumnDescriptor {
            name: "ProductId".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal: 0,
        }],
        cardinality: ResultCardinality::ExactlyOne,
        row_count_requirement: RowCountRequirement::ExactRequired,
        row_count_exact: Some(1),
        row_count_max: Some(1),
    };

    assert!(descriptor.validate().is_ok());
}

#[test]
fn result_descriptor_rejects_inconsistent_row_count_max() {
    fn col() -> ColumnDescriptor {
        ColumnDescriptor {
            name: "x".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal: 0,
        }
    }

    // ExactlyOne with max > 1 is contradictory.
    let bad = ResultStreamDescriptor {
        stream_name: "S".to_string(),
        columns: vec![col()],
        cardinality: ResultCardinality::ExactlyOne,
        row_count_requirement: RowCountRequirement::ExactRequired,
        row_count_exact: Some(1),
        row_count_max: Some(2),
    };
    assert!(bad.validate().is_err());

    // OneOrMore with max 0 is contradictory (min row count is 1).
    let bad_min = ResultStreamDescriptor {
        stream_name: "S".to_string(),
        columns: vec![col()],
        cardinality: ResultCardinality::OneOrMore,
        row_count_requirement: RowCountRequirement::ExactIfKnown,
        row_count_exact: None,
        row_count_max: Some(0),
    };
    assert!(bad_min.validate().is_err());

    // exact > max is contradictory.
    let bad_dom = ResultStreamDescriptor {
        stream_name: "S".to_string(),
        columns: vec![col()],
        cardinality: ResultCardinality::ZeroOrMore,
        row_count_requirement: RowCountRequirement::ExactIfKnown,
        row_count_exact: Some(5),
        row_count_max: Some(3),
    };
    assert!(bad_dom.validate().is_err());

    // Bounded ZeroOrMore with consistent declarations validates.
    let ok = ResultStreamDescriptor {
        stream_name: "S".to_string(),
        columns: vec![col()],
        cardinality: ResultCardinality::ZeroOrMore,
        row_count_requirement: RowCountRequirement::ExactIfKnown,
        row_count_exact: None,
        row_count_max: Some(64),
    };
    assert!(ok.validate().is_ok());
}

#[test]
fn result_descriptor_rejects_missing_or_sparse_columns() {
    let mut empty = sample_stream();
    empty.columns.clear();
    assert_eq!(
        empty.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let mut sparse = sample_stream();
    sparse.columns[0].ordinal = 1;
    assert_eq!(
        sparse.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let mut duplicate_name = sample_stream();
    duplicate_name.columns.push(sample_column("ProductId", 1));
    assert_eq!(
        duplicate_name.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let mut dense = sample_stream();
    dense.columns.push(sample_column("Reserved", 1));
    assert!(dense.validate().is_ok());
}

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
fn manifest_hash_is_deterministic_and_field_sensitive() {
    let manifest = sample_manifest();
    assert!(!manifest.manifest_hash().is_zero());
    assert_eq!(manifest.manifest_hash(), manifest.clone().manifest_hash());

    // Mutating any participating field flips the hash.
    let mut renamed = manifest.clone();
    renamed.procedure_name.push_str("_v2");
    assert_ne!(manifest.manifest_hash(), renamed.manifest_hash());

    let mut bumped_policy = manifest.clone();
    bumped_policy.policy_version = ManifestPolicyVersion::test_vector(0x55);
    assert_ne!(manifest.manifest_hash(), bumped_policy.manifest_hash());

    let mut bumped_stats = manifest.clone();
    bumped_stats.stats_version += 1;
    assert_ne!(manifest.manifest_hash(), bumped_stats.manifest_hash());

    let mut extra_perm = manifest.clone();
    extra_perm
        .required_permissions
        .push(RequiredPermission::new(
            "andromeda.read_contract",
            "application",
        ));
    assert_ne!(manifest.manifest_hash(), extra_perm.manifest_hash());

    let mut wider_layout = manifest.clone();
    wider_layout.protocol_layout.descriptor_set_hash = ContractHash::test_vector(0x77);
    assert_ne!(manifest.manifest_hash(), wider_layout.manifest_hash());
}

#[test]
fn manifest_binding_projection_is_deterministic_and_full_identity() {
    let manifest = sample_manifest();
    let binding = manifest.binding();

    assert_eq!(binding, sample_manifest().binding());

    let mut bumped_catalog = manifest.clone();
    bumped_catalog.catalog_version = CatalogVersion::new(8);
    assert_ne!(binding, bumped_catalog.binding());

    let mut bumped_contract = manifest.clone();
    bumped_contract.contract_hash = ContractHash::test_vector(0x55);
    assert_ne!(binding, bumped_contract.binding());

    let mut bumped_stats = manifest.clone();
    bumped_stats.stats_version += 1;
    assert_ne!(binding, bumped_stats.binding());

    let mut bumped_policy = manifest.clone();
    bumped_policy.policy_version = ManifestPolicyVersion::test_vector(0x66);
    assert_ne!(binding, bumped_policy.binding());

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
fn manifest_validate_rejects_missing_or_invalid_fields() {
    let base = sample_manifest();
    assert!(base.validate().is_ok());

    let blank_name = ProcedureManifest {
        procedure_name: "   ".to_string(),
        ..base.clone()
    };
    assert_eq!(
        blank_name.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let zero_contract = ProcedureManifest {
        contract_hash: ContractHash::zero(),
        ..base.clone()
    };
    assert_eq!(
        zero_contract.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let dup_streams = ProcedureManifest {
        result_streams: vec![sample_stream(), sample_stream()],
        ..base.clone()
    };
    assert_eq!(
        dup_streams.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let dup_perms = ProcedureManifest {
        required_permissions: vec![
            RequiredPermission::new("andromeda.execute_procedure", "application"),
            RequiredPermission::new("andromeda.execute_procedure", "application"),
        ],
        ..base.clone()
    };
    assert_eq!(
        dup_perms.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let upper_perm = ProcedureManifest {
        required_permissions: vec![RequiredPermission::new(
            "Andromeda.Execute_Procedure",
            "application",
        )],
        ..base
    };
    assert_eq!(
        upper_perm.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}

#[test]
fn protocol_layout_separates_descriptor_and_frame_hashes() {
    let collide = ProtocolLayout {
        descriptor_set_hash: ContractHash::test_vector(0xAA),
        frame_envelope_hash: ContractHash::test_vector(0xAA),
    };
    assert_eq!(
        collide.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let zero = ProtocolLayout {
        descriptor_set_hash: ContractHash::zero(),
        frame_envelope_hash: ContractHash::test_vector(0xAA),
    };
    assert_eq!(
        zero.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let collide_with_contract = ProcedureManifest {
        contract_hash: ContractHash::test_vector(0x33),
        ..sample_manifest()
    };
    // contract_hash matches descriptor_set_hash in the sample manifest.
    assert_eq!(
        collide_with_contract.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}

#[test]
fn ensure_source_generator_ready_demands_explicit_metadata() {
    let manifest = sample_manifest();
    assert!(manifest.ensure_source_generator_ready().is_ok());

    let binding = manifest.binding();
    assert_eq!(binding.procedure_id, ProcedureId::new(42));
    assert_eq!(binding.catalog_version, CatalogVersion::new(7));
    assert_eq!(binding.contract_hash, ContractHash::test_vector(0x11));
    assert_eq!(binding.stats_version, 3);
    assert_eq!(
        binding.policy_version,
        ManifestPolicyVersion::test_vector(0x22)
    );
    assert!(binding.validate().is_ok());

    let zero_proc = ProcedureManifest {
        procedure_id: ProcedureId::new(0),
        ..manifest.clone()
    };
    assert_eq!(
        zero_proc
            .ensure_source_generator_ready()
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

    let zero_catalog = ProcedureManifest {
        catalog_version: CatalogVersion::new(0),
        ..manifest.clone()
    };
    assert_eq!(
        zero_catalog
            .ensure_source_generator_ready()
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

    let zero_stats = ProcedureManifest {
        stats_version: 0,
        ..manifest.clone()
    };
    assert_eq!(
        zero_stats
            .ensure_source_generator_ready()
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

    let zero_policy = ProcedureManifest {
        policy_version: ManifestPolicyVersion::zero(),
        ..manifest.clone()
    };
    assert_eq!(
        zero_policy
            .ensure_source_generator_ready()
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

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
        RequiredPermission::new("andromeda.execute_procedure", "")
            .validate()
            .is_err()
    );
    assert!(
        RequiredPermission::new("Andromeda.X", "application")
            .validate()
            .is_err()
    );
}
