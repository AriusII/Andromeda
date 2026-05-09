use andromeda_error::AndromedaErrorKind;
use andromeda_proto_wire::validate_generated_invocation_response_sequence;

use super::common::{
    invocation_response, mutation_only_completion, mutation_only_metadata, response,
    result_completion_policy, result_stream_descriptor, valid_batch, valid_completion,
    valid_metadata, zero_row_completion, zero_row_metadata,
};

const GENERATED_INVOCATION_RESPONSE_PAYLOAD_BUDGET_BYTES: usize = 64 * 1024 * 1024;

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

    validate_generated_invocation_response_sequence(&sequence).unwrap();
}

#[test]
fn generated_invocation_response_sequence_rejects_completion_before_metadata() {
    let sequence = vec![response(
        0,
        invocation_response::Response::Completion(valid_completion()),
    )];

    assert!(validate_generated_invocation_response_sequence(&sequence).is_err());
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

    assert!(validate_generated_invocation_response_sequence(&sequence).is_err());
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

    assert!(validate_generated_invocation_response_sequence(&sequence).is_err());
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

    validate_generated_invocation_response_sequence(&sequence).unwrap();
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

    validate_generated_invocation_response_sequence(&sequence).unwrap();
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

    assert!(validate_generated_invocation_response_sequence(&sequence).is_err());
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

    assert!(validate_generated_invocation_response_sequence(&sequence).is_err());
}

#[test]
fn generated_invocation_response_sequence_rejects_undeclared_batch_result_name() {
    let mut batch = valid_batch(0);
    batch.result_name = "Inventory.ReserveStock.Undeclared".to_string();

    let sequence = vec![
        response(0, invocation_response::Response::Metadata(valid_metadata())),
        response(1, invocation_response::Response::Batch(batch)),
    ];

    let error = validate_generated_invocation_response_sequence(&sequence)
        .expect_err("batch result_name must be declared by metadata");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(
        error.message().contains("declared by metadata"),
        "undeclared batch result rejection should identify metadata declaration drift"
    );
}

#[test]
fn generated_invocation_response_sequence_rejects_total_payload_budget_overflow() {
    let mut metadata = valid_metadata();
    metadata.result_streams[0].cardinality =
        result_stream_descriptor::Cardinality::ZeroOrMore as i32;
    metadata.result_streams[0].row_count_exact = Some(2);
    metadata.result_streams[0].row_count_max = Some(2);

    let mut first_batch = valid_batch(0);
    first_batch.row_count_exact = Some(2);
    first_batch.terminal_batch = false;
    first_batch.structured_payload = vec![0xAA; GENERATED_INVOCATION_RESPONSE_PAYLOAD_BUDGET_BYTES];

    let mut overflow_batch = valid_batch(1);
    overflow_batch.row_count_exact = Some(2);
    overflow_batch.terminal_batch = true;
    overflow_batch.structured_payload = vec![0xBB];

    let sequence = vec![
        response(0, invocation_response::Response::Metadata(metadata)),
        response(1, invocation_response::Response::Batch(first_batch)),
        response(2, invocation_response::Response::Batch(overflow_batch)),
    ];

    let error = validate_generated_invocation_response_sequence(&sequence)
        .expect_err("total structured payload bytes must stay within the generated V0 budget");

    assert_eq!(error.kind(), AndromedaErrorKind::Protocol);
    assert!(
        error
            .message()
            .contains("structured_payload bytes exceed bounded limit"),
        "payload budget rejection should identify total structured payload bytes"
    );
}

#[test]
fn generated_invocation_response_sequence_rejects_repeated_metadata_before_terminal() {
    let sequence = vec![
        response(0, invocation_response::Response::Metadata(valid_metadata())),
        response(1, invocation_response::Response::Metadata(valid_metadata())),
    ];

    let error = validate_generated_invocation_response_sequence(&sequence)
        .expect_err("metadata must not drift or repeat inside one response sequence");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(
        error.message().contains("must not repeat metadata"),
        "metadata drift rejection should identify repeated metadata"
    );
}

#[test]
fn generated_invocation_response_sequence_rejects_metadata_after_terminal() {
    let sequence = vec![
        response(
            0,
            invocation_response::Response::Metadata(zero_row_metadata()),
        ),
        response(
            1,
            invocation_response::Response::Completion(zero_row_completion()),
        ),
        response(2, invocation_response::Response::Metadata(valid_metadata())),
    ];

    let error = validate_generated_invocation_response_sequence(&sequence)
        .expect_err("metadata cannot appear after the terminal response");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(
        error.message().contains("payload after terminal response"),
        "terminal drift rejection should identify payload after terminal"
    );
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

    assert!(validate_generated_invocation_response_sequence(&sequence).is_err());
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

    assert!(validate_generated_invocation_response_sequence(&sequence).is_err());
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

    assert!(validate_generated_invocation_response_sequence(&sequence).is_err());
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

    assert!(validate_generated_invocation_response_sequence(&sequence).is_err());
}
