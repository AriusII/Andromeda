#![allow(dead_code)]

use andromeda_proto_wire::{
    GeneratedColumnDescriptor, GeneratedCompletionShape, GeneratedPayloadKind,
    GeneratedProtocolVersion, GeneratedResultCompletionPolicy, GeneratedResultRowCountSummary,
    GeneratedResultStreamCardinality, GeneratedResultStreamDescriptor,
    GeneratedRowCountRequirement, GeneratedRpcBatch, GeneratedRpcCompletion,
    GeneratedRpcCompletionStatus, GeneratedRpcMetadata, GeneratedTransactionOutcome,
    decode_protobuf_message, encode_protobuf_message,
};
use andromeda_types::ContractHash;
use prost::Message;

pub(crate) const HASH_LEN: usize = ContractHash::LEN;
pub(crate) const RESERVE_STOCK_PROCEDURE: &str = "Inventory.ReserveStock";
pub(crate) const RESERVATION_STREAM: &str = "Reservation";
pub(crate) const RESERVATION_RESULT: &str = "Inventory.ReserveStock.Reservation";

pub(crate) const RESERVE_STOCK_CONTRACT_HASH_BYTE: u8 = 9;
pub(crate) const RESERVE_STOCK_CATALOG_VERSION: u64 = 44;
pub(crate) const RESERVE_STOCK_STATS_VERSION: u64 = 6;
pub(crate) const RESERVE_STOCK_REQUEST_ID: u64 = 101;
pub(crate) const RESERVE_STOCK_SESSION_ID: u64 = 202;
pub(crate) const RESERVE_STOCK_TX_ID: u64 = 404;
pub(crate) const RESERVE_STOCK_DURABLE_LSN: u64 = 505;

pub(crate) fn generated_protocol_v1() -> GeneratedProtocolVersion {
    GeneratedProtocolVersion { major: 1, minor: 0 }
}

pub(crate) fn generated_hash(byte: u8) -> Vec<u8> {
    vec![byte; HASH_LEN]
}

pub(crate) fn reserve_stock_contract_hash() -> Vec<u8> {
    generated_hash(RESERVE_STOCK_CONTRACT_HASH_BYTE)
}

pub(crate) fn round_trip_generated<M>(message: &M) -> M
where
    M: Message + Default,
{
    let serialized = encode_protobuf_message(message);
    decode_protobuf_message(&serialized, "generated protobuf")
        .expect("generated protobuf round trip should succeed")
}

pub(crate) fn reserve_stock_metadata() -> GeneratedRpcMetadata {
    GeneratedRpcMetadata {
        result_streams: vec![GeneratedResultStreamDescriptor {
            stream_name: RESERVATION_RESULT.to_string(),
            columns: vec![GeneratedColumnDescriptor {
                name: "reservation_id".to_string(),
                ordinal: 0,
                type_name: "u64".to_string(),
            }],
            cardinality: GeneratedResultStreamCardinality::ExactlyOne as i32,
            row_count_requirement: GeneratedRowCountRequirement::ExactRequired as i32,
            row_count_exact: Some(1),
            row_count_max: Some(1),
        }],
        completion_policy: Some(GeneratedResultCompletionPolicy {
            completion_shape: GeneratedCompletionShape::RequiresRowBatch as i32,
            reason: "reservation row required".to_string(),
        }),
    }
}

pub(crate) fn reservation_batch() -> GeneratedRpcBatch {
    GeneratedRpcBatch {
        result_name: RESERVATION_RESULT.to_string(),
        batch_index: 0,
        rows_emitted: 1,
        structured_payload: b"\x01".to_vec(),
        row_count_exact: Some(1),
        terminal_batch: true,
    }
}

pub(crate) fn reservation_row_count_summary() -> GeneratedResultRowCountSummary {
    GeneratedResultRowCountSummary {
        result_name: RESERVATION_RESULT.to_string(),
        rows_emitted: 1,
        row_count_exact: Some(1),
    }
}

pub(crate) fn committed_reservation_completion() -> GeneratedRpcCompletion {
    GeneratedRpcCompletion {
        status: GeneratedRpcCompletionStatus::Committed as i32,
        rows_affected: Some(1),
        tx_id: Some(RESERVE_STOCK_TX_ID),
        request_id: Some(RESERVE_STOCK_REQUEST_ID),
        session_id: Some(RESERVE_STOCK_SESSION_ID),
        trace_id: Some("trace-proto-101".to_string()),
        transaction_outcome: GeneratedTransactionOutcome::Committed as i32,
        durable_lsn: Some(RESERVE_STOCK_DURABLE_LSN),
        result_row_counts: vec![reservation_row_count_summary()],
    }
}

pub(crate) fn payload_kind_rpc_execute_request() -> i32 {
    GeneratedPayloadKind::RpcExecuteRequest as i32
}
