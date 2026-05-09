use andromeda_error::AndromedaErrorKind;
use andromeda_procedure_contract::{
    ResultCardinality, ResultStreamDescriptor, RowCountRequirement,
};
use andromeda_proto::generated;
use andromeda_types::ContractHash;
use generated::{
    contract::v1::{
        ColumnDescriptor as ProtoColumnDescriptor,
        ResultStreamDescriptor as ProtoResultStreamDescriptor, result_stream_descriptor,
    },
    protocol::v1::{
        ErrorEnvelope, InvocationCorrelation, InvocationResponse, ResultCompletionPolicy, RpcBatch,
        RpcCompletion, RpcMetadata, error_envelope, invocation_response, result_completion_policy,
        rpc_completion,
    },
};

use super::support::{
    CONTRACT_SCHEMAS, PROTOCOL_SCHEMAS, declared_message_names,
    governance_result_stream_descriptor, schema_contains,
};

fn hash(byte: u8) -> Vec<u8> {
    vec![byte; ContractHash::LEN]
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

fn valid_metadata() -> RpcMetadata {
    RpcMetadata {
        result_streams: vec![ProtoResultStreamDescriptor {
            stream_name: "Inventory.ReserveStock.Reservation".to_string(),
            columns: vec![ProtoColumnDescriptor {
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

fn bounded_many_metadata(row_count_max: u64) -> RpcMetadata {
    let mut metadata = valid_metadata();
    let stream = &mut metadata.result_streams[0];
    stream.cardinality = result_stream_descriptor::Cardinality::ZeroOrMore as i32;
    stream.row_count_requirement =
        result_stream_descriptor::RowCountRequirement::ExactIfKnown as i32;
    stream.row_count_exact = None;
    stream.row_count_max = Some(row_count_max);
    metadata
}

fn zero_row_cancellable_metadata() -> RpcMetadata {
    let mut metadata = valid_metadata();
    let stream = &mut metadata.result_streams[0];
    stream.cardinality = result_stream_descriptor::Cardinality::ZeroOrMore as i32;
    stream.row_count_exact = Some(0);
    stream.row_count_max = Some(0);
    let policy = metadata.completion_policy.as_mut().unwrap();
    policy.completion_shape =
        result_completion_policy::CompletionShape::AllowsZeroRowCompletion as i32;
    metadata
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

fn bounded_many_batch(batch_index: u64, terminal_batch: bool) -> RpcBatch {
    RpcBatch {
        row_count_exact: None,
        terminal_batch,
        ..valid_batch(batch_index)
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

fn bounded_many_completion(rows_emitted: u64) -> RpcCompletion {
    RpcCompletion {
        rows_affected: Some(rows_emitted),
        result_row_counts: vec![rpc_completion::ResultRowCountSummary {
            result_name: "Inventory.ReserveStock.Reservation".to_string(),
            rows_emitted,
            row_count_exact: None,
        }],
        ..valid_completion()
    }
}

fn cancelled_completion() -> RpcCompletion {
    RpcCompletion {
        status: rpc_completion::Status::Cancelled as i32,
        rows_affected: None,
        transaction_outcome: rpc_completion::TransactionOutcome::Cancelled as i32,
        durable_lsn: None,
        result_row_counts: vec![rpc_completion::ResultRowCountSummary {
            result_name: "Inventory.ReserveStock.Reservation".to_string(),
            rows_emitted: 0,
            row_count_exact: Some(0),
        }],
        ..valid_completion()
    }
}

fn backpressure_error() -> ErrorEnvelope {
    ErrorEnvelope {
        request_id: Some(101),
        session_id: Some(202),
        trace_id: Some("trace-proto-101".to_string()),
        family: error_envelope::ErrorFamily::Resource as i32,
        code: "RESULT_STREAM_BACKPRESSURE".to_string(),
        message: "ResultStream producer must slow down".to_string(),
        transaction_effect: error_envelope::TransactionEffect::NoTransaction as i32,
        retry_disposition: error_envelope::RetryDisposition::Backpressure as i32,
        retry_after_ms: Some(10),
        backpressure: Some(error_envelope::BackpressureMetadata {
            retry_after_ms: Some(10),
            capacity_percent: Some(90),
            shed_load: false,
        }),
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
fn result_stream_schema_declares_metadata_batch_completion_sequence_contracts() {
    let declared_messages = declared_message_names();

    for message in [
        "RpcMetadata",
        "RpcBatch",
        "ResultCompletionPolicy",
        "InvocationResponse",
    ] {
        assert!(
            declared_messages.contains(message),
            "protocol schema must include {message}"
        );
    }
    assert!(
        declared_messages.contains("ResultStreamDescriptor"),
        "contract schema must include ResultStreamDescriptor"
    );

    for field in [
        "CompletionShape completion_shape = 1;",
        "bytes structured_payload = 4;",
        "bool terminal_batch = 6;",
    ] {
        assert!(
            schema_contains(PROTOCOL_SCHEMAS, field),
            "protocol schema missing ResultStream sequence field: {field}"
        );
    }

    assert!(
        schema_contains(CONTRACT_SCHEMAS, "optional uint64 row_count_max = 6;"),
        "contract schema missing ResultStream row_count_max bound"
    );
}

#[test]
fn result_stream_descriptor_aligns_cardinality_and_exact_row_count_requirement() {
    let descriptor = governance_result_stream_descriptor();

    assert!(descriptor.validate().is_ok());

    let missing_exact = ResultStreamDescriptor {
        row_count_exact: None,
        ..descriptor.clone()
    };
    assert_eq!(
        missing_exact.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );

    let wrong_cardinality = ResultStreamDescriptor {
        row_count_exact: Some(2),
        row_count_max: Some(2),
        ..descriptor
    };
    assert_eq!(
        wrong_cardinality.validate().unwrap_err().kind(),
        AndromedaErrorKind::Contract
    );
}

#[test]
fn generated_result_stream_sequence_accepts_metadata_batch_completion() {
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
fn generated_result_stream_sequence_accepts_bounded_many_batches_with_terminal_completion() {
    let sequence = vec![
        response(
            0,
            invocation_response::Response::Metadata(bounded_many_metadata(2)),
        ),
        response(
            1,
            invocation_response::Response::Batch(bounded_many_batch(0, false)),
        ),
        response(
            2,
            invocation_response::Response::Batch(bounded_many_batch(1, true)),
        ),
        response(
            3,
            invocation_response::Response::Completion(bounded_many_completion(2)),
        ),
    ];

    generated::validate_generated_invocation_response_sequence(&sequence).unwrap();
}

#[test]
fn generated_result_stream_sequence_rejects_rows_beyond_metadata_max() {
    let sequence = vec![
        response(
            0,
            invocation_response::Response::Metadata(bounded_many_metadata(1)),
        ),
        response(
            1,
            invocation_response::Response::Batch(bounded_many_batch(0, false)),
        ),
        response(
            2,
            invocation_response::Response::Batch(bounded_many_batch(1, true)),
        ),
    ];

    let error = generated::validate_generated_invocation_response_sequence(&sequence)
        .expect_err("bounded ResultStream metadata must cap emitted rows");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(
        error.message().contains("row_count_max"),
        "bounded row-count rejection should identify row_count_max"
    );
}

#[test]
fn generated_result_stream_sequence_rejects_batch_after_terminal_batch() {
    let sequence = vec![
        response(
            0,
            invocation_response::Response::Metadata(bounded_many_metadata(2)),
        ),
        response(
            1,
            invocation_response::Response::Batch(bounded_many_batch(0, true)),
        ),
        response(
            2,
            invocation_response::Response::Batch(bounded_many_batch(1, true)),
        ),
    ];

    let error = generated::validate_generated_invocation_response_sequence(&sequence)
        .expect_err("terminal batch must close payload emission for that result stream");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(
        error.message().contains("batch after terminal batch"),
        "terminal-batch ordering rejection should identify the late batch"
    );
}

#[test]
fn generated_result_stream_sequence_accepts_cancelled_completion_as_terminal_evidence() {
    let sequence = vec![
        response(
            0,
            invocation_response::Response::Metadata(zero_row_cancellable_metadata()),
        ),
        response(
            1,
            invocation_response::Response::Completion(cancelled_completion()),
        ),
    ];

    generated::validate_generated_invocation_response_sequence(&sequence).unwrap();
}

#[test]
fn generated_result_stream_sequence_rejects_payload_after_backpressure_error() {
    let sequence = vec![
        response(
            0,
            invocation_response::Response::Error(backpressure_error()),
        ),
        response(1, invocation_response::Response::Metadata(valid_metadata())),
    ];

    let error = generated::validate_generated_invocation_response_sequence(&sequence)
        .expect_err("backpressure error is terminal ResultStream evidence");

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
    assert!(
        error.message().contains("payload after terminal response"),
        "terminal backpressure error should reject later ResultStream payloads"
    );
}

#[test]
fn generated_result_stream_sequence_rejects_batch_before_metadata() {
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
fn result_stream_exact_required_descriptor_is_contract_versioned() {
    let descriptor = governance_result_stream_descriptor();

    assert_eq!(descriptor.cardinality, ResultCardinality::ExactlyOne);
    assert_eq!(
        descriptor.row_count_requirement,
        RowCountRequirement::ExactRequired
    );
    assert_eq!(descriptor.row_count_exact, Some(1));
    assert_eq!(descriptor.row_count_max, Some(1));
}
