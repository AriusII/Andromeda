use super::*;

#[test]
fn test_gateway_rejects_contract_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        hash(0x99),
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        hash(0x99),
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(8, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("ContractHash"),
        "contract mismatch error should name ContractHash"
    );
}

#[test]
fn test_gateway_rejects_surface_scope_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "administration",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(9, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(
        err.message().contains("surface_scope"),
        "surface mismatch error should name the protobuf surface_scope"
    );
}

#[test]
fn test_gateway_rejects_catalog_version_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        CatalogVersion::new(10),
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        CatalogVersion::new(10),
    );

    let err = gateway
        .bind_application_procedure_route(10, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("CatalogVersion"),
        "catalog version mismatch error should name CatalogVersion"
    );
}

#[test]
fn test_gateway_rejects_stats_version_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version + 1),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(11, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("StatsVersion"),
        "stats version mismatch error should name StatsVersion"
    );
}

#[test]
fn test_gateway_rejects_missing_stats_version_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        None,
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(12, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("expected_stats_version"),
        "missing stats version error should name expected_stats_version"
    );
}

#[test]
fn test_gateway_rejects_zero_manifest_stats_version_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let mut manifest = route_manifest();
    manifest.stats_version = 0;
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(5),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(17, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("stats_version") || err.message().contains("stats version"),
        "manifest binding error should name StatsVersion"
    );
}

#[test]
fn test_gateway_rejects_procedure_name_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReleaseStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(13, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("Procedure name"),
        "procedure name mismatch error should name Procedure name"
    );
}

#[test]
fn test_gateway_rejects_envelope_contract_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        hash(0x77),
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(14, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("envelope ContractHash"),
        "envelope mismatch error should name envelope ContractHash"
    );
}

#[test]
fn test_gateway_rejects_envelope_catalog_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        CatalogVersion::new(77),
    );

    let err = gateway
        .bind_application_procedure_route(15, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("envelope CatalogVersion"),
        "envelope mismatch error should name envelope CatalogVersion"
    );
}
