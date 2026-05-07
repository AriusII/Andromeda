use andromeda_error::AndromedaErrorKind;
use andromeda_proto::{ResultCardinality, ResultStreamDescriptor, RowCountRequirement, generated};
use andromeda_types::ContractHash;
use generated::{
    contract::v1::{
        ColumnDescriptor as ProtoColumnDescriptor,
        ResultStreamDescriptor as ProtoResultStreamDescriptor, result_stream_descriptor,
    },
    protocol::v1::{
        InvocationCorrelation, InvocationResponse, ResultCompletionPolicy, RpcBatch, RpcCompletion,
        RpcMetadata, invocation_response, result_completion_policy, rpc_completion,
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
