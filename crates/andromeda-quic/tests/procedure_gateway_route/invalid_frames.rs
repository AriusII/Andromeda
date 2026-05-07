use super::*;

#[test]
fn test_gateway_rejects_duplicate_execute_argument_names_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut request = valid_generated_execute_request(&manifest);
    request.arguments = vec![
        generated::protocol::v1::rpc_execute_request::Argument {
            name: "Quantity".to_string(),
            type_name: "i64".to_string(),
            value: 3_i64.to_le_bytes().to_vec(),
        },
        generated::protocol::v1::rpc_execute_request::Argument {
            name: "Quantity".to_string(),
            type_name: "i64".to_string(),
            value: 4_i64.to_le_bytes().to_vec(),
        },
    ];
    let frame = execute_request_frame_from_generated(
        request,
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(18, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("argument names"),
        "canonical execute request validation should reject duplicate argument names"
    );
}

#[test]
fn test_gateway_rejects_empty_execute_argument_value_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut request = valid_generated_execute_request(&manifest);
    request.arguments = vec![generated::protocol::v1::rpc_execute_request::Argument {
        name: "Quantity".to_string(),
        type_name: "i64".to_string(),
        value: Vec::new(),
    }];
    let frame = execute_request_frame_from_generated(
        request,
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(19, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("argument value"),
        "canonical execute request validation should reject empty argument values"
    );
}

#[test]
fn test_gateway_rejects_zero_execute_budget_priority_before_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let mut request = valid_generated_execute_request(&manifest);
    request.budget = Some(
        generated::protocol::v1::rpc_execute_request::RequestBudget {
            cpu_micros: Some(5_000),
            memory_bytes: Some(64 * 1024),
            io_bytes: Some(128 * 1024),
            priority_class: Some(0),
        },
    );
    let frame = execute_request_frame_from_generated(
        request,
        manifest.contract_hash,
        manifest.catalog_version,
    );

    let err = gateway
        .bind_application_procedure_route(20, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("priority_class"),
        "canonical execute request validation should reject zero budget priority"
    );
}

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
