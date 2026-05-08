use andromeda_core::{AndromedaResult, ContractHash};
use andromeda_proto::{
    generated, validate_generated_invocation_response_sequence, validate_generated_rpc_batch,
    validate_generated_rpc_completion, validate_generated_rpc_execute_request,
    validate_generated_rpc_metadata,
};

use super::proto_wire_fixtures::{
    committed_reservation_completion, invocation_correlation, reservation_batch,
    reserve_stock_execute_request, reserve_stock_metadata,
};

#[test]
fn crate_root_exports_quic_boundary_generated_validators() {
    let execute_validator: fn(&generated::protocol::v1::RpcExecuteRequest) -> AndromedaResult<()> =
        validate_generated_rpc_execute_request;
    let metadata_validator: fn(&generated::protocol::v1::RpcMetadata) -> AndromedaResult<()> =
        validate_generated_rpc_metadata;
    let batch_validator: fn(&generated::protocol::v1::RpcBatch) -> AndromedaResult<()> =
        validate_generated_rpc_batch;
    let completion_validator: fn(&generated::protocol::v1::RpcCompletion) -> AndromedaResult<()> =
        validate_generated_rpc_completion;
    let sequence_validator: fn(
        &[generated::protocol::v1::InvocationResponse],
    ) -> AndromedaResult<()> = validate_generated_invocation_response_sequence;

    let execute = reserve_stock_execute_request();
    execute_validator(&execute).unwrap();

    let metadata = reserve_stock_metadata();
    metadata_validator(&metadata).unwrap();

    let batch = reservation_batch();
    batch_validator(&batch).unwrap();

    let completion = committed_reservation_completion();
    completion_validator(&completion).unwrap();

    let correlation = invocation_correlation();
    let responses = vec![
        generated::protocol::v1::InvocationResponse {
            correlation: Some(correlation.clone()),
            response_index: Some(0),
            response: Some(
                generated::protocol::v1::invocation_response::Response::Metadata(metadata),
            ),
        },
        generated::protocol::v1::InvocationResponse {
            correlation: Some(correlation.clone()),
            response_index: Some(1),
            response: Some(generated::protocol::v1::invocation_response::Response::Batch(batch)),
        },
        generated::protocol::v1::InvocationResponse {
            correlation: Some(correlation),
            response_index: Some(2),
            response: Some(
                generated::protocol::v1::invocation_response::Response::Completion(completion),
            ),
        },
    ];
    sequence_validator(&responses).unwrap();
}

#[test]
fn generated_rpc_execute_request_validation_rejects_default_runtime_bindings() {
    let valid = reserve_stock_execute_request();
    let valid_budget = valid
        .budget
        .clone()
        .expect("valid ReserveStock execute fixture carries a request budget");

    assert!(generated::validate_generated_rpc_execute_request(&valid).is_ok());

    let invalid_cases = [
        (
            "procedure_name",
            generated::protocol::v1::RpcExecuteRequest {
                procedure_name: " ".to_string(),
                ..valid.clone()
            },
        ),
        (
            "expected_contract_hash",
            generated::protocol::v1::RpcExecuteRequest {
                expected_contract_hash: vec![9; ContractHash::LEN - 1],
                ..valid.clone()
            },
        ),
        (
            "expected_catalog_version",
            generated::protocol::v1::RpcExecuteRequest {
                expected_catalog_version: 0,
                ..valid.clone()
            },
        ),
        (
            "surface_scope",
            generated::protocol::v1::RpcExecuteRequest {
                surface_scope: String::new(),
                ..valid.clone()
            },
        ),
        (
            "expected_stats_version",
            generated::protocol::v1::RpcExecuteRequest {
                expected_stats_version: None,
                ..valid.clone()
            },
        ),
        (
            "argument value",
            generated::protocol::v1::RpcExecuteRequest {
                arguments: vec![generated::protocol::v1::rpc_execute_request::Argument {
                    name: "Quantity".to_string(),
                    type_name: "i64".to_string(),
                    value: Vec::new(),
                }],
                ..valid.clone()
            },
        ),
        (
            "budget priority_class",
            generated::protocol::v1::RpcExecuteRequest {
                budget: Some(
                    generated::protocol::v1::rpc_execute_request::RequestBudget {
                        priority_class: Some(0),
                        ..valid_budget.clone()
                    },
                ),
                ..valid.clone()
            },
        ),
    ];

    for (field, request) in invalid_cases {
        assert!(
            generated::validate_generated_rpc_execute_request(&request).is_err(),
            "{field} should be rejected by generated typed Procedure execute validation"
        );
    }
}
