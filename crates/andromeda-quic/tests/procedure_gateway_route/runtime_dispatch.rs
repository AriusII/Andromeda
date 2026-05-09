use std::time::Duration;

use andromeda_quic::StreamConcurrencyManager;
use andromeda_rpc_codec::TypedResultStreamBounds;
use andromeda_rpc_protocol::BackpressureReason;

use super::*;

#[test]
fn test_gateway_binds_application_execute_route_to_manifest_before_dispatch() {
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
        manifest.catalog_version,
    );

    let binding = gateway
        .bind_application_procedure_route(42, &frame, &manifest)
        .expect("procedure route binding should validate");

    assert_eq!(binding.invocation_id, InvocationId::new(42));
    assert_eq!(binding.request_id, RequestId::new(501));
    assert_eq!(binding.session_id, SessionId::new(100));
    assert_eq!(binding.surface_plane, SurfacePlane::Application);
    assert_eq!(binding.procedure_id, manifest.procedure_id);
    assert_eq!(binding.contract_hash, manifest.contract_hash);
    assert_eq!(binding.catalog_version, manifest.catalog_version);
    assert_eq!(binding.stats_version, manifest.stats_version);
    assert_eq!(
        binding.execute_request.procedure_name,
        "Inventory.ReserveStock"
    );
    assert_eq!(
        binding.execute_request.expected_contract_hash,
        manifest.contract_hash
    );
    assert_eq!(
        binding.execute_request.expected_catalog_version,
        manifest.catalog_version
    );
    assert_eq!(
        binding.execute_request.expected_stats_version,
        manifest.stats_version
    );
    assert_eq!(
        binding.certificate_identity.surface_scope(),
        SurfaceScope::Application
    );
    assert_eq!(
        binding.typed_result_stream_context(),
        TypedResultStreamContext::new(
            RequestId::new(501),
            SessionId::new(100),
            None,
            manifest.contract_hash,
            manifest.catalog_version,
        )
    );
    let result_dispatch =
        binding.result_stream_dispatch_policy(ResultStreamMetadataPolicy::RowBatchRequired);
    assert_eq!(
        result_dispatch.typed_result_stream_context(),
        Some(binding.typed_result_stream_context()),
        "gateway-created result dispatch policy must carry admitted route context"
    );
}

#[test]
fn test_gateway_authorized_route_remains_pre_transaction_before_runtime_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = valid_execute_frame(&manifest);
    let (registry, principal_id) = registry_for_application_user();

    let authorized = gateway
        .bind_authorized_application_procedure_route(42, &frame, &manifest, &registry)
        .expect("authorized route should bind before runtime dispatch");

    assert_eq!(authorized.principal_id, principal_id);
    assert_eq!(authorized.route.invocation_id, InvocationId::new(42));
    assert_eq!(authorized.route.tx_id, None);
    assert_eq!(authorized.route.surface_plane, SurfacePlane::Application);
    assert_eq!(authorized.route.procedure_id, manifest.procedure_id);
    assert_common_authorization_evidence(
        &authorized.authorization_evidence,
        PrincipalAuthorizationOutcome::Allowed,
        "allowed",
        PrincipalAuthorizationEvaluationStage::Allowed,
        &registry,
    );
    assert_eq!(
        authorized.route.typed_result_stream_context(),
        TypedResultStreamContext::new(
            RequestId::new(501),
            SessionId::new(100),
            None,
            manifest.contract_hash,
            manifest.catalog_version,
        )
    );
    let result_dispatch = authorized
        .route
        .result_stream_dispatch_policy(ResultStreamMetadataPolicy::RowBatchRequired);
    assert_eq!(
        result_dispatch.typed_result_stream_context(),
        Some(authorized.route.typed_result_stream_context()),
        "authorized route evidence is still pre-transaction ResultStream context"
    );
}

#[test]
fn test_runtime_dispatch_backpressure_is_keyed_to_admitted_route_context() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = valid_execute_frame(&manifest);
    let (registry, _) = registry_for_application_user();

    let authorized = gateway
        .bind_authorized_application_procedure_route(42, &frame, &manifest, &registry)
        .expect("authorized route should bind before runtime dispatch");

    let mut streams =
        StreamConcurrencyManager::with_bounds(1, Duration::from_secs(30), Duration::from_secs(300));
    streams
        .create_stream(authorized.route.invocation_id)
        .expect("admitted invocation should enter runtime stream accounting");

    let backpressure = streams
        .backpressure_status()
        .expect("single admitted stream should saturate the bounded runtime");
    assert_eq!(
        backpressure.reason,
        BackpressureReason::ExecutionQueueSaturated
    );
    assert_eq!(
        backpressure.request_id, None,
        "capacity backpressure stays runtime soft state and is not a Procedure argument"
    );

    let err = streams
        .create_stream(InvocationId::new(43))
        .expect_err("runtime must reject new streams while saturated");
    assert_eq!(err.kind(), AndromedaErrorKind::Resource);

    let result_dispatch = authorized.route.bounded_result_stream_dispatch_policy(
        ResultStreamMetadataPolicy::RowBatchRequired,
        TypedResultStreamBounds::new(3, 4096),
    );
    assert_eq!(
        result_dispatch.typed_result_stream_context(),
        Some(authorized.route.typed_result_stream_context()),
        "runtime result dispatch must carry the pre-dispatch admitted route context"
    );
}

#[test]
fn test_gateway_rejects_grpc_contract_package_layout_before_runtime_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let mut manifest = route_manifest();
    manifest.protocol_layout.contract_package = "grpc.andromeda.contract.v1".to_string();
    let frame = valid_execute_frame(&manifest);

    let err = gateway
        .bind_application_procedure_route(42, &frame, &manifest)
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(
        err.message().contains("contract_package"),
        "gRPC contract package drift must be rejected before runtime dispatch: {}",
        err.message()
    );
}
