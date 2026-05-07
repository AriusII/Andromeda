#![forbid(unsafe_code)]

use andromeda_core::{AndromedaErrorKind, ContractHash};
use andromeda_proto::generated::{
    self,
    contract::v1::{ColumnDescriptor, ResultStreamDescriptor, result_stream_descriptor},
    protocol::v1::{
        InvocationCorrelation, InvocationRequest, InvocationResponse, ResultCompletionPolicy,
        RpcBatch, RpcCompletion, RpcExecuteRequest, RpcMetadata, invocation_response,
        result_completion_policy, rpc_completion,
    },
};

fn hash(byte: u8) -> Vec<u8> {
    vec![byte; ContractHash::LEN]
}

fn valid_execute_request() -> RpcExecuteRequest {
    RpcExecuteRequest {
        procedure_name: "Inventory.ReserveStock".to_string(),
        expected_contract_hash: hash(0x11),
        expected_catalog_version: 7,
        surface_scope: "application".to_string(),
        arguments: Vec::new(),
        budget: None,
        expected_stats_version: Some(5),
    }
}

fn valid_correlation() -> InvocationCorrelation {
    InvocationCorrelation {
        request_id: Some(101),
        session_id: Some(202),
        trace_id: Some("trace-proto-101".to_string()),
        contract_hash: Some(hash(0x11)),
        catalog_version: Some(7),
        invocation_id: Some(303),
        stats_version: Some(5),
        expected_policy_version: Some(11),
    }
}

fn valid_invocation_request() -> InvocationRequest {
    InvocationRequest {
        correlation: Some(valid_correlation()),
        execute_request: Some(valid_execute_request()),
    }
}

fn valid_completion() -> RpcCompletion {
    RpcCompletion {
        status: rpc_completion::Status::Committed as i32,
        rows_affected: Some(1),
        tx_id: Some(404),
        request_id: Some(101),
        session_id: Some(202),
        trace_id: Some("trace-proto-101".to_string()),
        transaction_outcome: rpc_completion::TransactionOutcome::Committed as i32,
        durable_lsn: Some(505),
        result_row_counts: vec![rpc_completion::ResultRowCountSummary {
            result_name: "Inventory.ReserveStock.Reservation".to_string(),
            rows_emitted: 1,
            row_count_exact: Some(1),
        }],
    }
}

#[test]
fn generated_invocation_request_requires_binding_identity_match() {
    let valid = valid_invocation_request();
    generated::validate_generated_invocation_request(&valid).unwrap();

    let mut wrong_hash = valid.clone();
    wrong_hash.correlation.as_mut().unwrap().contract_hash = Some(hash(0x22));
    let wrong_hash_error = generated::validate_generated_invocation_request(&wrong_hash)
        .expect_err("mismatched ContractHash must be rejected");
    assert_eq!(wrong_hash_error.kind(), AndromedaErrorKind::Contract);
    assert!(
        wrong_hash_error.message().contains("contract_hash"),
        "error should name the mismatched ContractHash field"
    );

    let mut wrong_catalog = valid.clone();
    wrong_catalog.correlation.as_mut().unwrap().catalog_version = Some(8);
    let wrong_catalog_error = generated::validate_generated_invocation_request(&wrong_catalog)
        .expect_err("mismatched CatalogVersion must be rejected");
    assert_eq!(wrong_catalog_error.kind(), AndromedaErrorKind::Contract);
    assert!(
        wrong_catalog_error.message().contains("catalog_version"),
        "error should name the mismatched CatalogVersion field"
    );

    let mut wrong_stats = valid.clone();
    wrong_stats.correlation.as_mut().unwrap().stats_version = Some(6);
    let wrong_stats_error = generated::validate_generated_invocation_request(&wrong_stats)
        .expect_err("mismatched StatsVersion must be rejected");
    assert_eq!(wrong_stats_error.kind(), AndromedaErrorKind::Contract);
    assert!(
        wrong_stats_error.message().contains("stats_version"),
        "error should name the mismatched StatsVersion field"
    );

    let mut missing_stats = valid;
    missing_stats.correlation.as_mut().unwrap().stats_version = None;
    assert!(generated::validate_generated_invocation_request(&missing_stats).is_err());
}

#[test]
fn generated_invocation_request_requires_expected_policy_version() {
    let valid = valid_invocation_request();
    generated::validate_generated_invocation_request(&valid).unwrap();

    let mut missing_policy = valid.clone();
    missing_policy
        .correlation
        .as_mut()
        .unwrap()
        .expected_policy_version = None;
    assert!(generated::validate_generated_invocation_request(&missing_policy).is_err());

    let mut zero_policy = valid;
    zero_policy
        .correlation
        .as_mut()
        .unwrap()
        .expected_policy_version = Some(0);
    assert!(generated::validate_generated_invocation_request(&zero_policy).is_err());
}

#[test]
fn generated_rpc_execute_request_rejects_unbounded_argument_count() {
    let mut request = valid_execute_request();
    request.arguments = (0..129)
        .map(
            |index| generated::protocol::v1::rpc_execute_request::Argument {
                name: format!("arg_{index}"),
                type_name: "bytes".to_string(),
                value: vec![1],
            },
        )
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

fn valid_metadata() -> RpcMetadata {
    RpcMetadata {
        result_streams: vec![ResultStreamDescriptor {
            stream_name: "Inventory.ReserveStock.Reservation".to_string(),
            columns: vec![ColumnDescriptor {
                name: "reservation_id".to_string(),
                ordinal: 0,
                type_name: "u64".to_string(),
            }],
            cardinality: result_stream_descriptor::Cardinality::ExactlyOne as i32,
            row_count_requirement: result_stream_descriptor::RowCountRequirement::ExactRequired
                as i32,
            row_count_exact: Some(1),
            row_count_max: Some(1),
        }],
        completion_policy: Some(ResultCompletionPolicy {
            completion_shape: result_completion_policy::CompletionShape::RequiresRowBatch as i32,
            reason: "reservation row required".to_string(),
        }),
    }
}

fn zero_row_metadata() -> RpcMetadata {
    RpcMetadata {
        result_streams: vec![ResultStreamDescriptor {
            stream_name: "Inventory.ReserveStock.EmptyReservation".to_string(),
            columns: vec![ColumnDescriptor {
                name: "reservation_id".to_string(),
                ordinal: 0,
                type_name: "u64".to_string(),
            }],
            cardinality: result_stream_descriptor::Cardinality::ZeroOrMore as i32,
            row_count_requirement: result_stream_descriptor::RowCountRequirement::ExactRequired
                as i32,
            row_count_exact: Some(0),
            row_count_max: Some(0),
        }],
        completion_policy: Some(ResultCompletionPolicy {
            completion_shape: result_completion_policy::CompletionShape::AllowsZeroRowCompletion
                as i32,
            reason: "empty result allowed".to_string(),
        }),
    }
}

fn mutation_only_metadata() -> RpcMetadata {
    RpcMetadata {
        result_streams: Vec::new(),
        completion_policy: Some(ResultCompletionPolicy {
            completion_shape: result_completion_policy::CompletionShape::MutationOnly as i32,
            reason: "mutation only".to_string(),
        }),
    }
}

fn valid_batch(batch_index: u64) -> RpcBatch {
    RpcBatch {
        result_name: "Inventory.ReserveStock.Reservation".to_string(),
        batch_index,
        rows_emitted: 1,
        structured_payload: vec![0xAA],
        row_count_exact: Some(1),
        terminal_batch: true,
    }
}

fn zero_row_completion() -> RpcCompletion {
    RpcCompletion {
        result_row_counts: vec![rpc_completion::ResultRowCountSummary {
            result_name: "Inventory.ReserveStock.EmptyReservation".to_string(),
            rows_emitted: 0,
            row_count_exact: Some(0),
        }],
        ..valid_completion()
    }
}

fn mutation_only_completion() -> RpcCompletion {
    RpcCompletion {
        result_row_counts: Vec::new(),
        ..valid_completion()
    }
}

fn response(response_index: u64, response: invocation_response::Response) -> InvocationResponse {
    InvocationResponse {
        correlation: Some(valid_correlation()),
        response_index: Some(response_index),
        response: Some(response),
    }
}

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

    assert!(generated::validate_generated_invocation_response_sequence(&sequence).is_err());
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

    assert!(generated::validate_generated_invocation_response_sequence(&sequence).is_err());
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
