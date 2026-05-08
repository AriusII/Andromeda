use andromeda_error::AndromedaErrorKind;
use andromeda_proto::generated::{
    self,
    protocol::v1::{
        InvocationCorrelation, InvocationResponse, invocation_response, rpc_execute_request,
    },
};

use super::common::{
    response, valid_batch, valid_completion, valid_correlation, valid_execute_request,
    valid_metadata,
};

const GENERATED_RPC_ARGUMENT_VALUE_BUDGET_BYTES: usize = 16 * 1024 * 1024;

#[test]
fn generated_rpc_execute_request_rejects_unbounded_argument_count() {
    let mut request = valid_execute_request();
    request.arguments = (0..129)
        .map(|index| rpc_execute_request::Argument {
            name: format!("arg_{index}"),
            type_name: "bytes".to_string(),
            value: vec![1],
        })
        .collect();

    let error = generated::validate_generated_rpc_execute_request(&request)
        .expect_err("execute request argument count must be bounded");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(
        error.message().contains("arguments exceed bounded limit"),
        "argument-count bound error should identify the bounded field"
    );
}

#[test]
fn generated_rpc_execute_request_rejects_unbounded_binary_argument_value() {
    let mut request = valid_execute_request();
    request.arguments = vec![rpc_execute_request::Argument {
        name: "ReservationRows".to_string(),
        type_name: "StructuredObject".to_string(),
        value: vec![0xAB; GENERATED_RPC_ARGUMENT_VALUE_BUDGET_BYTES + 1],
    }];

    let error = generated::validate_generated_rpc_execute_request(&request)
        .expect_err("execute argument binary payload must stay within bounded projection limits");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(
        error
            .message()
            .contains("argument value exceeds bounded limit"),
        "argument-value bound error should identify the bounded binary field"
    );
}

#[test]
fn generated_invocation_response_requires_typed_payload_and_correlation() {
    let valid = InvocationResponse {
        correlation: Some(valid_correlation()),
        response_index: Some(0),
        response: Some(invocation_response::Response::Completion(valid_completion())),
    };
    generated::validate_generated_invocation_response(&valid).unwrap();

    let missing_payload = InvocationResponse {
        response: None,
        ..valid.clone()
    };
    assert!(generated::validate_generated_invocation_response(&missing_payload).is_err());

    let missing_correlation = InvocationResponse {
        correlation: None,
        ..valid.clone()
    };
    assert!(generated::validate_generated_invocation_response(&missing_correlation).is_err());

    let zero_request_id = InvocationResponse {
        correlation: Some(InvocationCorrelation {
            request_id: Some(0),
            ..valid_correlation()
        }),
        ..valid
    };
    assert!(generated::validate_generated_invocation_response(&zero_request_id).is_err());
}

#[test]
fn generated_invocation_response_sequence_requires_metadata_before_structured_payload() {
    let sequence = vec![
        response(0, invocation_response::Response::Batch(valid_batch(0))),
        response(1, invocation_response::Response::Metadata(valid_metadata())),
        response(
            2,
            invocation_response::Response::Completion(valid_completion()),
        ),
    ];

    let error = generated::validate_generated_invocation_response_sequence(&sequence)
        .expect_err("structured row payload must not be accepted before metadata");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(
        error.message().contains("batch must follow metadata"),
        "metadata-before-payload rejection should identify the batch ordering violation"
    );
}

#[test]
fn generated_invocation_response_sequence_rejects_batch_row_count_shape_drift() {
    let mut batch = valid_batch(0);
    batch.row_count_exact = Some(2);

    let sequence = vec![
        response(0, invocation_response::Response::Metadata(valid_metadata())),
        response(1, invocation_response::Response::Batch(batch)),
        response(
            2,
            invocation_response::Response::Completion(valid_completion()),
        ),
    ];

    let error = generated::validate_generated_invocation_response_sequence(&sequence)
        .expect_err("batch row-count metadata must match the declared result stream shape");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(
        error
            .message()
            .contains("row_count_exact must match metadata"),
        "row-count shape drift should be reported against metadata"
    );
}
