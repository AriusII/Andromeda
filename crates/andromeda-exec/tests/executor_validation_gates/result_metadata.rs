use super::support::*;

#[test]
fn gate_exec_07_result_metadata_extraction_documents_current_pre_tx_gap() {
    let plan = valid_plan();
    assert!(plan.validate().is_ok());

    let err = SrplProcedureDispatcher::result_metadata_for_plan(&plan, &[])
        .expect_err("metadata extraction with empty result streams should fail");

    assert_eq!(err.kind(), AndromedaErrorKind::Srpl);
    assert!(
        err.message()
            .contains("procedure must declare at least one result stream")
    );
}

#[test]
fn gate_exec_07_resolved_manifest_carries_metadata_and_single_result_policy() {
    let dispatcher = srpl_dispatcher(Arc::new(MockValidResolver::new()));

    let resolved = dispatcher
        .resolve_procedure(&invocation_request(77))
        .expect("valid resolver response should satisfy request");

    assert_eq!(
        resolved.manifest.result_metadata_policy,
        ResultMetadataPolicy::RequireBeforePayload
    );
    assert_eq!(
        resolved.manifest.multi_result_policy,
        MultiResultPolicy::SingleResultOnly
    );
    assert_eq!(resolved.manifest.result_streams.len(), 1);
    assert_eq!(resolved.manifest.result_streams[0].stream_id, 1);

    let metadata = SrplProcedureDispatcher::result_metadata_for_plan(
        &resolved.plan,
        &resolved.manifest.result_streams,
    )
    .expect("metadata extraction should succeed");

    assert_eq!(metadata.stream_id, 1);
    assert_eq!(metadata.column_count, 1);
}

#[test]
fn gate_exec_07_multi_result_manifest_rejected_before_dispatch() {
    let mut response = valid_response();
    response
        .manifest
        .result_streams
        .push(result_stream(2, "Audit"));
    let dispatcher = srpl_dispatcher(Arc::new(MockValidResolver::with_response(response)));

    let err = dispatcher
        .resolve_procedure(&invocation_request(78))
        .expect_err("single-result manifest must reject multiple result streams");

    assert!(matches!(err, ProcedureResolveError::InvalidResponse { .. }));
    assert!(
        err.into_andromeda_error()
            .message()
            .contains("multi-result policy")
    );
}
