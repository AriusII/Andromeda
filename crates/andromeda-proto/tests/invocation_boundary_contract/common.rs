use andromeda_proto::generated::{
    contract::v1::{ColumnDescriptor, ResultStreamDescriptor, result_stream_descriptor},
    protocol::v1::{
        InvocationCorrelation, InvocationRequest, InvocationResponse, ResultCompletionPolicy,
        RpcBatch, RpcCompletion, RpcExecuteRequest, RpcMetadata, invocation_response,
        result_completion_policy, rpc_completion,
    },
};
use andromeda_types::ContractHash;

pub(crate) fn hash(byte: u8) -> Vec<u8> {
    vec![byte; ContractHash::LEN]
}

pub(crate) fn valid_execute_request() -> RpcExecuteRequest {
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

pub(crate) fn valid_correlation() -> InvocationCorrelation {
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

pub(crate) fn valid_invocation_request() -> InvocationRequest {
    InvocationRequest {
        correlation: Some(valid_correlation()),
        execute_request: Some(valid_execute_request()),
    }
}

pub(crate) fn valid_completion() -> RpcCompletion {
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

pub(crate) fn valid_metadata() -> RpcMetadata {
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

pub(crate) fn zero_row_metadata() -> RpcMetadata {
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

pub(crate) fn mutation_only_metadata() -> RpcMetadata {
    RpcMetadata {
        result_streams: Vec::new(),
        completion_policy: Some(ResultCompletionPolicy {
            completion_shape: result_completion_policy::CompletionShape::MutationOnly as i32,
            reason: "mutation only".to_string(),
        }),
    }
}

pub(crate) fn valid_batch(batch_index: u64) -> RpcBatch {
    RpcBatch {
        result_name: "Inventory.ReserveStock.Reservation".to_string(),
        batch_index,
        rows_emitted: 1,
        structured_payload: vec![0xAA],
        row_count_exact: Some(1),
        terminal_batch: true,
    }
}

pub(crate) fn zero_row_completion() -> RpcCompletion {
    RpcCompletion {
        result_row_counts: vec![rpc_completion::ResultRowCountSummary {
            result_name: "Inventory.ReserveStock.EmptyReservation".to_string(),
            rows_emitted: 0,
            row_count_exact: Some(0),
        }],
        ..valid_completion()
    }
}

pub(crate) fn mutation_only_completion() -> RpcCompletion {
    RpcCompletion {
        result_row_counts: Vec::new(),
        ..valid_completion()
    }
}

pub(crate) fn response(
    response_index: u64,
    response: invocation_response::Response,
) -> InvocationResponse {
    InvocationResponse {
        correlation: Some(valid_correlation()),
        response_index: Some(response_index),
        response: Some(response),
    }
}
