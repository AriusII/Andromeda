use super::*;

#[test]
fn test_authorized_route_rejects_non_application_surface_before_iam_or_dispatch() {
    let conn = setup_active_administration_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = valid_execute_frame(&manifest);
    let (registry, _) = registry_for_application_user();

    let err = gateway
        .bind_authorized_application_procedure_route(71, &frame, &manifest, &registry)
        .unwrap_err();

    assert_route_rejection_before_authorization(
        err,
        AndromedaErrorKind::Security,
        "Application surface",
    );
}

#[test]
fn test_authorized_route_rejects_manifest_permission_before_iam_or_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let mut manifest = route_manifest();
    manifest.required_permissions.clear();
    let frame = valid_execute_frame(&manifest);
    let (registry, _) = registry_for_application_user();

    let err = gateway
        .bind_authorized_application_procedure_route(72, &frame, &manifest, &registry)
        .unwrap_err();

    assert_route_rejection_before_authorization(
        err,
        AndromedaErrorKind::Contract,
        "required_permissions",
    );
}

#[test]
fn test_authorized_route_rejects_contract_mismatch_before_iam_or_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        &manifest.procedure_name,
        hash(0x99),
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        hash(0x99),
        manifest.catalog_version,
    );
    let (registry, _) = registry_for_application_user();

    let err = gateway
        .bind_authorized_application_procedure_route(73, &frame, &manifest, &registry)
        .unwrap_err();

    assert_route_rejection_before_authorization(err, AndromedaErrorKind::Contract, "ContractHash");
}

#[test]
fn test_authorized_route_rejects_catalog_mismatch_before_iam_or_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        &manifest.procedure_name,
        manifest.contract_hash,
        CatalogVersion::new(10),
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        CatalogVersion::new(10),
    );
    let (registry, _) = registry_for_application_user();

    let err = gateway
        .bind_authorized_application_procedure_route(74, &frame, &manifest, &registry)
        .unwrap_err();

    assert_route_rejection_before_authorization(
        err,
        AndromedaErrorKind::Contract,
        "CatalogVersion",
    );
}

#[test]
fn test_authorized_route_rejects_client_transaction_frame_before_iam_or_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut frame = valid_execute_frame(&manifest);
    frame.header.tx_id = Some(TransactionId::new(700));
    let (registry, _) = registry_for_application_user();

    let err = gateway
        .bind_authorized_application_procedure_route(75, &frame, &manifest, &registry)
        .unwrap_err();

    assert_route_rejection_before_authorization(
        err,
        AndromedaErrorKind::Protocol,
        "client transaction id",
    );
}
