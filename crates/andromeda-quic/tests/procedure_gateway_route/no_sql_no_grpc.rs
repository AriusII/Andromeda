use super::*;

#[test]
fn test_gateway_rejects_ad_hoc_sql_selector_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = execute_request_frame(
        "SELECT * FROM Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(21, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("Procedure name"),
        "ad hoc SQL text must not satisfy the cataloged Procedure selector"
    );
}

#[test]
fn test_gateway_rejects_grpc_manifest_protocol_layout_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let mut manifest = route_manifest();
    manifest.protocol_layout.protocol_package = "grpc.andromeda.protocol.v1".to_string();
    let frame = valid_execute_frame(&manifest);

    let err = gateway
        .bind_application_procedure_route(22, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("protocol_package"),
        "gRPC protocol layout drift must be rejected before dispatch"
    );
}
