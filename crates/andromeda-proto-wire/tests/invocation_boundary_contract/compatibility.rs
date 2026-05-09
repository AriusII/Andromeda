use andromeda_proto_wire::validate_generated_invocation_response_sequence;

use super::common::{
    invocation_response, response, result_stream_descriptor, valid_batch, valid_completion,
    valid_metadata,
};

#[test]
fn generated_invocation_response_sequence_rejects_late_batch_row_count_exact() {
    let mut metadata = valid_metadata();
    metadata.result_streams[0].row_count_requirement =
        result_stream_descriptor::RowCountRequirement::ExactIfKnown as i32;
    metadata.result_streams[0].row_count_exact = None;

    let sequence = vec![
        response(0, invocation_response::Response::Metadata(metadata)),
        response(1, invocation_response::Response::Batch(valid_batch(0))),
        response(
            2,
            invocation_response::Response::Completion(valid_completion()),
        ),
    ];

    assert!(validate_generated_invocation_response_sequence(&sequence).is_err());
}

#[test]
fn generated_invocation_response_sequence_rejects_late_completion_row_count_exact() {
    let mut metadata = valid_metadata();
    metadata.result_streams[0].row_count_requirement =
        result_stream_descriptor::RowCountRequirement::ExactIfKnown as i32;
    metadata.result_streams[0].row_count_exact = None;

    let mut batch = valid_batch(0);
    batch.row_count_exact = None;

    let sequence = vec![
        response(0, invocation_response::Response::Metadata(metadata)),
        response(1, invocation_response::Response::Batch(batch)),
        response(
            2,
            invocation_response::Response::Completion(valid_completion()),
        ),
    ];

    assert!(validate_generated_invocation_response_sequence(&sequence).is_err());
}
