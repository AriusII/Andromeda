use andromeda_error::AndromedaErrorKind;
use andromeda_proto::generated::{
    self,
    contract::v1::result_stream_descriptor,
    protocol::v1::{
        InvocationResponse, RpcBatch, RpcCompletion, invocation_response, rpc_completion,
    },
};

use super::common::{response, valid_batch, valid_completion, valid_correlation, valid_metadata};

#[test]
fn generated_invocation_response_sequence_rejects_binding_identity_drift_between_frames() {
    let mut drifted_correlation = valid_correlation();
    drifted_correlation.stats_version = Some(6);

    let sequence = vec![
        response(0, invocation_response::Response::Metadata(valid_metadata())),
        InvocationResponse {
            correlation: Some(drifted_correlation),
            response_index: Some(1),
            response: Some(invocation_response::Response::Batch(valid_batch(0))),
        },
    ];

    let error = generated::validate_generated_invocation_response_sequence(&sequence)
        .expect_err("response sequence must keep ContractHash/CatalogVersion/StatsVersion stable");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(
        error.message().contains("correlation must remain stable"),
        "correlation drift error should identify the stable-correlation invariant"
    );
}

#[test]
fn generated_invocation_response_sequence_rejects_unbounded_frame_count() {
    let batch_count = 1_023_u64;
    let result_name = "Inventory.ReserveStock.Reservation".to_string();
    let mut metadata = valid_metadata();
    metadata.result_streams[0].cardinality =
        result_stream_descriptor::Cardinality::ZeroOrMore as i32;
    metadata.result_streams[0].row_count_exact = Some(batch_count);
    metadata.result_streams[0].row_count_max = Some(batch_count);

    let mut sequence = Vec::with_capacity(batch_count as usize + 2);
    sequence.push(response(
        0,
        invocation_response::Response::Metadata(metadata),
    ));
    for batch_index in 0..batch_count {
        sequence.push(response(
            batch_index + 1,
            invocation_response::Response::Batch(RpcBatch {
                result_name: result_name.clone(),
                batch_index,
                rows_emitted: 1,
                structured_payload: vec![1],
                row_count_exact: Some(batch_count),
                terminal_batch: batch_index + 1 == batch_count,
            }),
        ));
    }
    sequence.push(response(
        batch_count + 1,
        invocation_response::Response::Completion(RpcCompletion {
            result_row_counts: vec![rpc_completion::ResultRowCountSummary {
                result_name,
                rows_emitted: batch_count,
                row_count_exact: Some(batch_count),
            }],
            ..valid_completion()
        }),
    ));

    let error = generated::validate_generated_invocation_response_sequence(&sequence)
        .expect_err("generated response sequence frame count must be bounded");

    assert_eq!(error.kind(), AndromedaErrorKind::Protocol);
    assert!(
        error
            .message()
            .contains("frame count exceeds bounded limit"),
        "frame-count bound error should identify the bounded field"
    );
}

#[test]
fn generated_invocation_response_rejects_completion_correlation_drift() {
    let mut completion = valid_completion();
    completion.request_id = Some(999);

    let response = InvocationResponse {
        correlation: Some(valid_correlation()),
        response_index: Some(0),
        response: Some(invocation_response::Response::Completion(completion)),
    };

    assert!(generated::validate_generated_invocation_response(&response).is_err());
}
