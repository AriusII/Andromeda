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
        binding.certificate_identity.surface,
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
