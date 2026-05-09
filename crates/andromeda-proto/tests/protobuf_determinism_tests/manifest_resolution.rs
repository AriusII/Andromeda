use prost::Message;

use andromeda_proto::generated::andromeda::contract::v1::CatalogProcedureManifestResolutionResponse;
use andromeda_proto_wire::{
    validate_catalog_procedure_manifest_resolution_request,
    validate_catalog_procedure_manifest_resolution_response,
};

use crate::proto_wire_fixtures::{
    HASH_LEN, generated_manifest_resolution_request, generated_reserve_stock_manifest,
    manifest_contract_hash, resolved_manifest_response, round_trip_generated,
    unresolved_manifest_response,
};

#[test]
fn catalog_manifest_resolution_request_roundtrips_without_json_or_grpc() {
    let request = generated_manifest_resolution_request();
    let decoded_request = round_trip_generated(&request);

    assert_eq!(request, decoded_request);
    validate_catalog_procedure_manifest_resolution_request(&request)
        .expect("manifest resolution request must satisfy boundary validation");
}

#[test]
fn catalog_manifest_resolution_response_is_deterministic_binary_projection() {
    let response = resolved_manifest_response(generated_reserve_stock_manifest());
    let response_bytes_1 = response.encode_to_vec();
    let response_bytes_2 = response.encode_to_vec();
    assert_eq!(
        response_bytes_1, response_bytes_2,
        "manifest resolution response encoding must be deterministic"
    );

    let decoded_response = round_trip_generated(&response);
    assert_eq!(response, decoded_response);
    validate_catalog_procedure_manifest_resolution_response(&response)
        .expect("manifest resolution response must satisfy boundary validation");
}

#[test]
fn generated_manifest_boundary_validation_rejects_invalid_result_stream_metadata() {
    let mut missing_exact = generated_reserve_stock_manifest();
    missing_exact.result_streams[0].row_count_exact = None;
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            missing_exact
        ))
        .is_err(),
        "exact-required result streams must carry row_count_exact"
    );

    let mut unknown_cardinality = generated_reserve_stock_manifest();
    unknown_cardinality.result_streams[0].cardinality = 99;
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            unknown_cardinality
        ))
        .is_err(),
        "unknown generated cardinality codes must be rejected"
    );

    let mut empty_column_type = generated_reserve_stock_manifest();
    empty_column_type.result_streams[0].columns[0].type_name = " ".to_string();
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            empty_column_type
        ))
        .is_err(),
        "generated column descriptors must not use empty type names"
    );

    let mut empty_columns = generated_reserve_stock_manifest();
    empty_columns.result_streams[0].columns.clear();
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            empty_columns
        ))
        .is_err(),
        "generated result stream descriptors must carry typed columns"
    );

    let mut sparse_columns = generated_reserve_stock_manifest();
    sparse_columns.result_streams[0].columns[0].ordinal = 1;
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            sparse_columns
        ))
        .is_err(),
        "generated result stream column ordinals must be dense and zero-based"
    );

    let mut contradictory_max = generated_reserve_stock_manifest();
    contradictory_max.result_streams[0].row_count_max = Some(2);
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            contradictory_max
        ))
        .is_err(),
        "exactly-one result streams must not declare row_count_max above one"
    );
}

#[test]
fn generated_manifest_boundary_validation_rejects_invalid_permission_policy() {
    let mut missing_permissions = generated_reserve_stock_manifest();
    missing_permissions.required_permissions.clear();
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            missing_permissions
        ))
        .is_err(),
        "resolved manifests must carry explicit required permissions"
    );

    let mut uppercase_permission = generated_reserve_stock_manifest();
    uppercase_permission.required_permissions[0].id = "Andromeda.Execute".to_string();
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            uppercase_permission
        ))
        .is_err(),
        "permission ids must stay canonical lower-case boundary metadata"
    );

    let mut conflicting_permission_family = generated_reserve_stock_manifest();
    let mut duplicate_permission = conflicting_permission_family.required_permissions[0].clone();
    duplicate_permission.family = "security".to_string();
    conflicting_permission_family
        .required_permissions
        .push(duplicate_permission);
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            conflicting_permission_family
        ))
        .is_err(),
        "permission ids must not be repeated with a conflicting family"
    );

    let mut policy_hash_collision = generated_reserve_stock_manifest();
    policy_hash_collision.policy_version = policy_hash_collision.contract_hash.clone();
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            policy_hash_collision
        ))
        .is_err(),
        "policy_version must remain distinct from contract_hash"
    );

    let mut empty_policy_version = generated_reserve_stock_manifest();
    empty_policy_version.policy_version.clear();
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            empty_policy_version
        ))
        .is_err(),
        "resolved manifests must carry a 32-byte policy_version"
    );

    let mut zero_policy_version = generated_reserve_stock_manifest();
    zero_policy_version.policy_version = vec![0; HASH_LEN];
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            zero_policy_version
        ))
        .is_err(),
        "resolved manifests must reject zero policy_version"
    );

    let mut zero_catalog_version = generated_reserve_stock_manifest();
    zero_catalog_version.catalog_version = 0;
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            zero_catalog_version
        ))
        .is_err(),
        "resolved manifests must reject zero CatalogVersion"
    );

    let mut missing_stats_version = generated_reserve_stock_manifest();
    missing_stats_version.stats_version = None;
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            missing_stats_version
        ))
        .is_err(),
        "resolved manifests must carry ProcedureContractBinding stats_version"
    );

    let mut zero_stats_version = generated_reserve_stock_manifest();
    zero_stats_version.stats_version = Some(0);
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&resolved_manifest_response(
            zero_stats_version
        ))
        .is_err(),
        "resolved manifests must reject zero ProcedureContractBinding stats_version"
    );
}

#[test]
fn catalog_manifest_resolution_resolved_response_must_match_manifest_binding_identity() {
    let manifest = generated_reserve_stock_manifest();

    let mut wrong_hash = resolved_manifest_response(manifest.clone());
    wrong_hash.resolved_contract_hash = Some(vec![0x55; HASH_LEN]);
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&wrong_hash).is_err(),
        "resolved_contract_hash must match the resolved ProcedureManifest contract_hash"
    );

    let mut wrong_catalog = resolved_manifest_response(manifest);
    wrong_catalog.resolved_catalog_version = Some(10);
    assert!(
        validate_catalog_procedure_manifest_resolution_response(&wrong_catalog).is_err(),
        "resolved_catalog_version must match the resolved ProcedureManifest catalog_version"
    );
}

#[test]
fn catalog_manifest_resolution_status_policy_accepts_governed_error_statuses() {
    let governed_error_statuses = [
        (2, "STATUS_NOT_FOUND"),
        (3, "STATUS_CATALOG_VERSION_MISMATCH"),
        (4, "STATUS_CONTRACT_HASH_MISMATCH"),
        (5, "STATUS_NOT_SOURCE_GENERATOR_READY"),
        (6, "STATUS_PERMISSION_DENIED"),
        (7, "STATUS_UNSUPPORTED"),
        (8, "STATUS_MALFORMED"),
        (9, "STATUS_INTERNAL"),
        (10, "STATUS_CATALOG_NOT_READY"),
        (11, "STATUS_AUTH_REQUIRED"),
    ];

    for (status, name) in governed_error_statuses {
        let response = unresolved_manifest_response(status, name);

        validate_catalog_procedure_manifest_resolution_response(&response)
            .unwrap_or_else(|error| panic!("{name} should satisfy response validation: {error}"));
    }
}

#[test]
fn catalog_manifest_resolution_error_statuses_reject_resolved_payload_fields() {
    let base = unresolved_manifest_response(2, "STATUS_NOT_FOUND");

    let cases = vec![
        (
            "manifest",
            CatalogProcedureManifestResolutionResponse {
                manifest: Some(generated_reserve_stock_manifest()),
                ..base.clone()
            },
        ),
        (
            "resolved_contract_hash",
            CatalogProcedureManifestResolutionResponse {
                resolved_contract_hash: Some(manifest_contract_hash()),
                ..base.clone()
            },
        ),
        (
            "resolved_catalog_version",
            CatalogProcedureManifestResolutionResponse {
                resolved_catalog_version: Some(9),
                ..base.clone()
            },
        ),
    ];

    for (field, response) in cases {
        assert!(
            validate_catalog_procedure_manifest_resolution_response(&response).is_err(),
            "non-resolved response must reject {field}"
        );
    }
}

#[test]
fn catalog_manifest_resolution_status_policy_rejects_unspecified_and_unknown_statuses() {
    for (status, name) in [(0, "STATUS_UNSPECIFIED"), (12, "future unknown status")] {
        let response = unresolved_manifest_response(status, name);

        assert!(
            validate_catalog_procedure_manifest_resolution_response(&response).is_err(),
            "{name} must not satisfy response validation"
        );
    }
}
