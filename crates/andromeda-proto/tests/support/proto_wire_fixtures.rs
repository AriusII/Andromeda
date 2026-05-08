#![allow(dead_code)]

// Each integration test crate includes this fixture file independently and uses
// only the helpers relevant to that contract surface.

use andromeda_core::{CatalogVersion, ContractHash, RequestId, SessionId, TransactionId};
use andromeda_proto::generated::{
    contract::v1 as contract_v1, decode_generated_message, encode_generated_message,
    protocol::v1 as protocol_v1,
};
use prost::Message;

pub(crate) const HASH_LEN: usize = ContractHash::LEN;
pub(crate) const RESERVE_STOCK_PROCEDURE: &str = "Inventory.ReserveStock";
pub(crate) const RESERVATION_STREAM: &str = "Reservation";
pub(crate) const RESERVATION_RESULT: &str = "Inventory.ReserveStock.Reservation";

pub(crate) const MANIFEST_CONTRACT_HASH_BYTE: u8 = 0x11;
pub(crate) const DESCRIPTOR_SET_HASH_BYTE: u8 = 0x22;
pub(crate) const FRAME_ENVELOPE_HASH_BYTE: u8 = 0x33;
pub(crate) const POLICY_VERSION_BYTE: u8 = 0x44;
pub(crate) const TEST_CONTRACT_HASH_BYTE: u8 = 42;
pub(crate) const TEST_DESCRIPTOR_HASH_BYTE: u8 = 43;
pub(crate) const RESERVE_STOCK_CONTRACT_HASH_BYTE: u8 = 9;

pub(crate) const RESERVE_STOCK_CATALOG_VERSION: u64 = 44;
pub(crate) const RESERVE_STOCK_STATS_VERSION: u64 = 6;
pub(crate) const RESERVE_STOCK_POLICY_VERSION: u64 = 11;
pub(crate) const RESERVE_STOCK_REQUEST_ID: u64 = 101;
pub(crate) const RESERVE_STOCK_SESSION_ID: u64 = 202;
pub(crate) const RESERVE_STOCK_INVOCATION_ID: u64 = 303;
pub(crate) const RESERVE_STOCK_TX_ID: u64 = 404;
pub(crate) const RESERVE_STOCK_DURABLE_LSN: u64 = 505;

pub(crate) fn generated_protocol_v1() -> protocol_v1::ProtocolVersion {
    protocol_v1::ProtocolVersion { major: 1, minor: 0 }
}

pub(crate) fn generated_hash(byte: u8) -> Vec<u8> {
    vec![byte; HASH_LEN]
}

pub(crate) fn manifest_contract_hash() -> Vec<u8> {
    generated_hash(MANIFEST_CONTRACT_HASH_BYTE)
}

pub(crate) fn reserve_stock_contract_hash() -> Vec<u8> {
    generated_hash(RESERVE_STOCK_CONTRACT_HASH_BYTE)
}

pub(crate) fn round_trip_generated<M>(message: &M) -> M
where
    M: Message + Default,
{
    let serialized = encode_generated_message(message);
    decode_generated_message(&serialized).expect("generated protobuf round trip should succeed")
}

pub(crate) fn proto_frame_envelope(
    contract_hash: ContractHash,
    catalog_version: CatalogVersion,
    request_id: RequestId,
    session_id: SessionId,
    tx_id: Option<TransactionId>,
    payload_kind: protocol_v1::PayloadKind,
    payload: impl Into<Vec<u8>>,
) -> protocol_v1::FrameEnvelope {
    protocol_v1::FrameEnvelope {
        protocol_version: Some(generated_protocol_v1()),
        contract_hash: contract_hash.as_bytes().to_vec(),
        catalog_version: catalog_version.get(),
        request_id: request_id.get(),
        session_id: session_id.get(),
        tx_id: tx_id.map(|id| id.get()),
        payload_kind: payload_kind as i32,
        payload: payload.into(),
    }
}

pub(crate) fn generated_reservation_result_stream() -> contract_v1::ResultStreamDescriptor {
    contract_v1::ResultStreamDescriptor {
        stream_name: RESERVATION_STREAM.to_string(),
        columns: vec![contract_v1::ColumnDescriptor {
            name: "Reserved".to_string(),
            ordinal: 0,
            type_name: "bool".to_string(),
        }],
        cardinality: contract_v1::result_stream_descriptor::Cardinality::ExactlyOne as i32,
        row_count_requirement:
            contract_v1::result_stream_descriptor::RowCountRequirement::ExactRequired as i32,
        row_count_exact: Some(1),
        row_count_max: Some(1),
    }
}

pub(crate) fn generated_reserve_stock_manifest() -> contract_v1::ProcedureManifest {
    contract_v1::ProcedureManifest {
        procedure_id: 42,
        procedure_name: RESERVE_STOCK_PROCEDURE.to_string(),
        contract_hash: manifest_contract_hash(),
        catalog_version: 9,
        protocol_layout: Some(contract_v1::ProtocolLayout {
            descriptor_set_hash: generated_hash(DESCRIPTOR_SET_HASH_BYTE),
            frame_envelope_hash: generated_hash(FRAME_ENVELOPE_HASH_BYTE),
            protocol_package: "andromeda.protocol.v1".to_string(),
            contract_package: "andromeda.contract.v1".to_string(),
        }),
        result_streams: vec![generated_reservation_result_stream()],
        policy_version: generated_hash(POLICY_VERSION_BYTE),
        required_permissions: vec![contract_v1::RequiredPermission {
            id: "andromeda.execute_procedure".to_string(),
            family: "application".to_string(),
        }],
        stats_version: Some(5),
    }
}

pub(crate) fn generated_manifest_resolution_request()
-> contract_v1::CatalogProcedureManifestResolutionRequest {
    contract_v1::CatalogProcedureManifestResolutionRequest {
        protocol_major: 1,
        protocol_minor: 0,
        request_id: 77,
        trace_id: Some("trace-manifest-77".to_string()),
        selector: Some(
            contract_v1::catalog_procedure_manifest_resolution_request::Selector::ProcedureName(
                RESERVE_STOCK_PROCEDURE.to_string(),
            ),
        ),
        expected_contract_hash: Some(manifest_contract_hash()),
        expected_catalog_version: Some(9),
        require_source_generator_ready: true,
    }
}

pub(crate) fn resolved_manifest_response(
    manifest: contract_v1::ProcedureManifest,
) -> contract_v1::CatalogProcedureManifestResolutionResponse {
    contract_v1::CatalogProcedureManifestResolutionResponse {
        protocol_major: 1,
        protocol_minor: 0,
        request_id: 77,
        trace_id: Some("trace-manifest-77".to_string()),
        status: 1,
        manifest: Some(manifest),
        resolved_contract_hash: Some(manifest_contract_hash()),
        resolved_catalog_version: Some(9),
        current_catalog_version: Some(9),
        diagnostic_code: None,
    }
}

pub(crate) fn unresolved_manifest_response(
    status: i32,
    diagnostic_code: impl Into<String>,
) -> contract_v1::CatalogProcedureManifestResolutionResponse {
    contract_v1::CatalogProcedureManifestResolutionResponse {
        protocol_major: 1,
        protocol_minor: 0,
        request_id: 78,
        trace_id: Some(format!("trace-status-{status}")),
        status,
        manifest: None,
        resolved_contract_hash: None,
        resolved_catalog_version: None,
        current_catalog_version: Some(9),
        diagnostic_code: Some(diagnostic_code.into()),
    }
}

pub(crate) fn reserve_stock_execute_request() -> protocol_v1::RpcExecuteRequest {
    protocol_v1::RpcExecuteRequest {
        procedure_name: RESERVE_STOCK_PROCEDURE.to_string(),
        expected_contract_hash: reserve_stock_contract_hash(),
        expected_catalog_version: RESERVE_STOCK_CATALOG_VERSION,
        surface_scope: "application".to_string(),
        arguments: vec![protocol_v1::rpc_execute_request::Argument {
            name: "Quantity".to_string(),
            type_name: "i64".to_string(),
            value: 3_i64.to_le_bytes().to_vec(),
        }],
        budget: Some(protocol_v1::rpc_execute_request::RequestBudget {
            cpu_micros: Some(5_000),
            memory_bytes: Some(64 * 1024),
            io_bytes: Some(128 * 1024),
            priority_class: Some(1),
        }),
        expected_stats_version: Some(RESERVE_STOCK_STATS_VERSION),
    }
}

pub(crate) fn reserve_stock_metadata() -> protocol_v1::RpcMetadata {
    protocol_v1::RpcMetadata {
        result_streams: vec![contract_v1::ResultStreamDescriptor {
            stream_name: RESERVATION_RESULT.to_string(),
            columns: vec![contract_v1::ColumnDescriptor {
                name: "reservation_id".to_string(),
                ordinal: 0,
                type_name: "u64".to_string(),
            }],
            cardinality: contract_v1::result_stream_descriptor::Cardinality::ExactlyOne as i32,
            row_count_requirement:
                contract_v1::result_stream_descriptor::RowCountRequirement::ExactRequired as i32,
            row_count_exact: Some(1),
            row_count_max: Some(1),
        }],
        completion_policy: Some(protocol_v1::ResultCompletionPolicy {
            completion_shape:
                protocol_v1::result_completion_policy::CompletionShape::RequiresRowBatch as i32,
            reason: "reservation row required".to_string(),
        }),
    }
}

pub(crate) fn reservation_batch() -> protocol_v1::RpcBatch {
    protocol_v1::RpcBatch {
        result_name: RESERVATION_RESULT.to_string(),
        batch_index: 0,
        rows_emitted: 1,
        structured_payload: b"\x01".to_vec(),
        row_count_exact: Some(1),
        terminal_batch: true,
    }
}

pub(crate) fn reservation_row_count_summary() -> protocol_v1::rpc_completion::ResultRowCountSummary
{
    protocol_v1::rpc_completion::ResultRowCountSummary {
        result_name: RESERVATION_RESULT.to_string(),
        rows_emitted: 1,
        row_count_exact: Some(1),
    }
}

pub(crate) fn committed_reservation_completion() -> protocol_v1::RpcCompletion {
    protocol_v1::RpcCompletion {
        status: protocol_v1::rpc_completion::Status::Committed as i32,
        rows_affected: Some(1),
        tx_id: Some(RESERVE_STOCK_TX_ID),
        request_id: Some(RESERVE_STOCK_REQUEST_ID),
        session_id: Some(RESERVE_STOCK_SESSION_ID),
        trace_id: Some("trace-proto-101".to_string()),
        transaction_outcome: protocol_v1::rpc_completion::TransactionOutcome::Committed as i32,
        durable_lsn: Some(RESERVE_STOCK_DURABLE_LSN),
        result_row_counts: vec![reservation_row_count_summary()],
    }
}

pub(crate) fn invocation_correlation() -> protocol_v1::InvocationCorrelation {
    protocol_v1::InvocationCorrelation {
        request_id: Some(RESERVE_STOCK_REQUEST_ID),
        session_id: Some(RESERVE_STOCK_SESSION_ID),
        trace_id: Some("trace-proto-101".to_string()),
        contract_hash: Some(reserve_stock_contract_hash()),
        catalog_version: Some(RESERVE_STOCK_CATALOG_VERSION),
        invocation_id: Some(RESERVE_STOCK_INVOCATION_ID),
        stats_version: Some(RESERVE_STOCK_STATS_VERSION),
        expected_policy_version: Some(RESERVE_STOCK_POLICY_VERSION),
    }
}
