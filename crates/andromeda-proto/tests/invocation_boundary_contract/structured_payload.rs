use andromeda_core::AndromedaErrorKind;
use andromeda_proto::generated::{
    self,
    protocol::v1::{
        InvocationCorrelation, InvocationResponse, invocation_response, rpc_execute_request,
    },
};

use super::common::{valid_completion, valid_correlation, valid_execute_request};

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
