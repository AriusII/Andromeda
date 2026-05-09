use super::*;

#[test]
fn test_gateway_rejects_client_transaction_id_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );
    frame.header.tx_id = Some(TransactionId::new(700));

    let err = gateway
        .bind_application_procedure_route(16, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(
        err.message().contains("client transaction id"),
        "client tx_id rejection should name client transaction id"
    );
}

#[test]
fn test_gateway_rejects_frame_envelope_context_mismatch_before_procedure_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );
    frame.header.request_id = RequestId::new(502);

    let err = gateway
        .bind_application_procedure_route(18, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(
        err.message().contains("context"),
        "route must reject frame/envelope context divergence"
    );
}

#[test]
fn test_gateway_rejects_frame_session_not_bound_to_connection_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut frame = execute_request_frame(
        "Inventory.ReserveStock",
        manifest.contract_hash,
        manifest.catalog_version,
        Some(manifest.stats_version),
        "application",
        manifest.contract_hash,
        manifest.catalog_version,
    );
    frame.header.session_id = SessionId::new(101);

    let err = gateway
        .bind_application_procedure_route(20, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(
        err.message().contains("authenticated connection"),
        "route must reject frames from a different authenticated session"
    );
}
