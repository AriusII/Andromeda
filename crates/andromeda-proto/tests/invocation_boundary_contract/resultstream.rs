use andromeda_proto::generated::{
    self,
    protocol::v1::{invocation_response, result_completion_policy},
};

use super::common::{
    mutation_only_completion, mutation_only_metadata, response, valid_batch, valid_completion,
    valid_metadata, zero_row_completion, zero_row_metadata,
};

#[test]
fn generated_invocation_response_sequence_accepts_metadata_batch_completion() {
    let sequence = vec![
        response(0, invocation_response::Response::Metadata(valid_metadata())),
        response(1, invocation_response::Response::Batch(valid_batch(0))),
        response(
            2,
            invocation_response::Response::Completion(valid_completion()),
        ),
    ];

    generated::validate_generated_invocation_response_sequence(&sequence).unwrap();
}

#[test]
fn generated_invocation_response_sequence_rejects_completion_before_metadata() {
    let sequence = vec![response(
        0,
        invocation_response::Response::Completion(valid_completion()),
    )];

    assert!(generated::validate_generated_invocation_response_sequence(&sequence).is_err());
}

#[test]
fn generated_invocation_response_sequence_rejects_batch_before_metadata() {
    let sequence = vec![
        response(0, invocation_response::Response::Batch(valid_batch(0))),
        response(
            1,
            invocation_response::Response::Completion(valid_completion()),
        ),
    ];

    assert!(generated::validate_generated_invocation_response_sequence(&sequence).is_err());
}

#[test]
fn generated_invocation_response_sequence_rejects_requires_row_batch_without_batch() {
    let sequence = vec![
        response(0, invocation_response::Response::Metadata(valid_metadata())),
        response(
            1,
            invocation_response::Response::Completion(valid_completion()),
        ),
    ];

    assert!(generated::validate_generated_invocation_response_sequence(&sequence).is_err());
}

#[test]
fn generated_invocation_response_sequence_allows_zero_row_completion_with_metadata_policy() {
    let sequence = vec![
        response(
            0,
            invocation_response::Response::Metadata(zero_row_metadata()),
        ),
        response(
            1,
            invocation_response::Response::Completion(zero_row_completion()),
        ),
    ];

    generated::validate_generated_invocation_response_sequence(&sequence).unwrap();
}

#[test]
fn generated_invocation_response_sequence_allows_mutation_only_completion_without_streams() {
    let sequence = vec![
        response(
            0,
            invocation_response::Response::Metadata(mutation_only_metadata()),
        ),
        response(
            1,
            invocation_response::Response::Completion(mutation_only_completion()),
        ),
    ];

    generated::validate_generated_invocation_response_sequence(&sequence).unwrap();
}

#[test]
fn generated_invocation_response_sequence_rejects_mutation_only_with_row_streams() {
    let mut metadata = valid_metadata();
    metadata
        .completion_policy
        .as_mut()
        .unwrap()
        .completion_shape = result_completion_policy::CompletionShape::MutationOnly as i32;
    let sequence = vec![
        response(0, invocation_response::Response::Metadata(metadata)),
        response(
            1,
            invocation_response::Response::Completion(mutation_only_completion()),
        ),
    ];

    assert!(generated::validate_generated_invocation_response_sequence(&sequence).is_err());
}

#[test]
fn generated_invocation_response_sequence_rejects_response_index_gap() {
    let sequence = vec![
        response(0, invocation_response::Response::Metadata(valid_metadata())),
        response(2, invocation_response::Response::Batch(valid_batch(0))),
        response(
            3,
            invocation_response::Response::Completion(valid_completion()),
        ),
    ];

    assert!(generated::validate_generated_invocation_response_sequence(&sequence).is_err());
}

#[test]
fn generated_invocation_response_sequence_rejects_payload_after_terminal() {
    let sequence = vec![
        response(0, invocation_response::Response::Metadata(valid_metadata())),
        response(
            1,
            invocation_response::Response::Completion(valid_completion()),
        ),
        response(2, invocation_response::Response::Batch(valid_batch(0))),
    ];

    assert!(generated::validate_generated_invocation_response_sequence(&sequence).is_err());
}

#[test]
fn generated_invocation_response_sequence_rejects_completion_count_drift() {
    let mut completion = valid_completion();
    completion.result_row_counts[0].rows_emitted = 2;

    let sequence = vec![
        response(0, invocation_response::Response::Metadata(valid_metadata())),
        response(1, invocation_response::Response::Batch(valid_batch(0))),
        response(2, invocation_response::Response::Completion(completion)),
    ];

    assert!(generated::validate_generated_invocation_response_sequence(&sequence).is_err());
}

#[test]
fn generated_invocation_response_sequence_rejects_missing_completion_summary() {
    let mut completion = valid_completion();
    completion.result_row_counts.clear();

    let sequence = vec![
        response(0, invocation_response::Response::Metadata(valid_metadata())),
        response(1, invocation_response::Response::Batch(valid_batch(0))),
        response(2, invocation_response::Response::Completion(completion)),
    ];

    assert!(generated::validate_generated_invocation_response_sequence(&sequence).is_err());
}

#[test]
fn generated_invocation_response_sequence_rejects_completion_before_terminal_batch() {
    let mut batch = valid_batch(0);
    batch.terminal_batch = false;

    let sequence = vec![
        response(0, invocation_response::Response::Metadata(valid_metadata())),
        response(1, invocation_response::Response::Batch(batch)),
        response(
            2,
            invocation_response::Response::Completion(valid_completion()),
        ),
    ];

    assert!(generated::validate_generated_invocation_response_sequence(&sequence).is_err());
}
