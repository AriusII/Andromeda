use andromeda_error::{AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::RowCountRequirement;
use andromeda_proto::generated;
use andromeda_proto_wire::{
    project_generated_structured_object_header, validate_generated_invocation_response_sequence,
    validate_generated_rpc_batch, validate_generated_rpc_completion,
    validate_generated_rpc_execute_request, validate_generated_rpc_metadata,
    validate_generated_structured_object_header,
};
use andromeda_structured_object::{StructuredObjectHeader, StructuredObjectLayout};
use andromeda_types::{ColumnDescriptor, ContractHash, ScalarType, TypeDescriptor};
use prost::Message;

use super::proto_wire_fixtures::{
    RESERVATION_RESULT, RESERVATION_STREAM, RESERVE_STOCK_PROCEDURE,
    committed_reservation_completion, generated_protocol_v1, invocation_correlation,
    reservation_batch, reserve_stock_contract_hash, reserve_stock_execute_request,
    reserve_stock_metadata,
};

#[test]
fn proto_wire_exports_quic_boundary_generated_validators() {
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
        .expect("valid ReserveStock execute fixture carries a request budget");

    assert!(validate_generated_rpc_execute_request(&valid).is_ok());

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
                        ..valid_budget
                    },
                ),
                ..valid.clone()
            },
        ),
    ];

    for (field, request) in invalid_cases {
        assert!(
            validate_generated_rpc_execute_request(&request).is_err(),
            "{field} should be rejected by generated typed Procedure execute validation"
        );
    }
}

#[test]
fn generated_frame_envelope_has_stable_wire_projection() {
    let envelope = generated::protocol::v1::FrameEnvelope {
        protocol_version: Some(generated_protocol_v1()),
        contract_hash: vec![7; ContractHash::LEN],
        catalog_version: 11,
        request_id: 101,
        session_id: 202,
        tx_id: Some(303),
        payload_kind: generated::protocol::v1::PayloadKind::RpcExecuteRequest as i32,
        payload: b"abc".to_vec(),
    };

    let encoded = envelope.encode_to_vec();
    let mut expected = vec![0x0a, 0x02, 0x08, 0x01, 0x12, 0x20];
    expected.extend_from_slice(&[7; ContractHash::LEN]);
    expected.extend_from_slice(&[
        0x18, 0x0b, 0x20, 0x65, 0x28, 0xca, 0x01, 0x30, 0xaf, 0x02, 0x38, 0x05, 0x42, 0x03, 0x61,
        0x62, 0x63,
    ]);

    assert_eq!(encoded, expected);

    let decoded = generated::protocol::v1::FrameEnvelope::decode(encoded.as_slice()).unwrap();
    assert_eq!(
        decoded.payload_kind,
        generated::protocol::v1::PayloadKind::RpcExecuteRequest as i32
    );
    assert_eq!(decoded.contract_hash, vec![7; ContractHash::LEN]);
    assert_eq!(decoded.request_id, 101);
    assert_eq!(decoded.session_id, 202);
    assert_eq!(decoded.tx_id, Some(303));
    assert_eq!(decoded.payload, b"abc");
}

#[test]
fn generated_rpc_completion_has_stable_wire_projection() {
    let completion = generated::protocol::v1::RpcCompletion {
        status: generated::protocol::v1::rpc_completion::Status::Committed as i32,
        rows_affected: Some(42),
        tx_id: Some(77),
        request_id: Some(101),
        session_id: Some(202),
        trace_id: Some("trace".to_string()),
        transaction_outcome: generated::protocol::v1::rpc_completion::TransactionOutcome::Committed
            as i32,
        durable_lsn: Some(7),
        result_row_counts: vec![
            generated::protocol::v1::rpc_completion::ResultRowCountSummary {
                result_name: RESERVATION_STREAM.to_string(),
                rows_emitted: 1,
                row_count_exact: Some(1),
            },
        ],
    };

    let encoded = completion.encode_to_vec();
    let expected = [
        0x08, 0x01, 0x10, 0x2a, 0x18, 0x4d, 0x80, 0x02, 0x65, 0x88, 0x02, 0xca, 0x01, 0x92, 0x02,
        0x05, 0x74, 0x72, 0x61, 0x63, 0x65, 0x98, 0x02, 0x02, 0xa0, 0x02, 0x07, 0xaa, 0x02, 0x11,
        0x0a, 0x0b, 0x52, 0x65, 0x73, 0x65, 0x72, 0x76, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x10, 0x01,
        0x18, 0x01,
    ];

    assert_eq!(encoded, expected);

    let decoded = generated::protocol::v1::RpcCompletion::decode(encoded.as_slice()).unwrap();
    assert_eq!(
        decoded.status,
        generated::protocol::v1::rpc_completion::Status::Committed as i32
    );
    assert_eq!(decoded.rows_affected, Some(42));
    assert_eq!(decoded.tx_id, Some(77));
    assert_eq!(decoded.durable_lsn, Some(7));
    assert_eq!(decoded.result_row_counts.len(), 1);
    assert_eq!(decoded.result_row_counts[0].row_count_exact, Some(1));
}

#[test]
fn generated_rpc_metadata_has_stable_wire_projection() {
    let metadata = generated::protocol::v1::RpcMetadata {
        result_streams: vec![generated::contract::v1::ResultStreamDescriptor {
            stream_name: RESERVATION_STREAM.to_string(),
            columns: vec![generated::contract::v1::ColumnDescriptor {
                name: "Reserved".to_string(),
                ordinal: 1,
                type_name: "bool".to_string(),
            }],
            cardinality: generated::contract::v1::result_stream_descriptor::Cardinality::ExactlyOne
                as i32,
            row_count_requirement:
                generated::contract::v1::result_stream_descriptor::RowCountRequirement::ExactRequired
                    as i32,
            row_count_exact: Some(1),
            row_count_max: None,
        }],
        completion_policy: Some(generated::protocol::v1::ResultCompletionPolicy {
            completion_shape:
                generated::protocol::v1::result_completion_policy::CompletionShape::RequiresRowBatch
                    as i32,
            reason: "requires batch".to_string(),
        }),
    };

    let encoded = metadata.encode_to_vec();
    let expected = [
        0x0a, 0x27, 0x0a, 0x0b, 0x52, 0x65, 0x73, 0x65, 0x72, 0x76, 0x61, 0x74, 0x69, 0x6f, 0x6e,
        0x12, 0x12, 0x0a, 0x08, 0x52, 0x65, 0x73, 0x65, 0x72, 0x76, 0x65, 0x64, 0x10, 0x01, 0x1a,
        0x04, 0x62, 0x6f, 0x6f, 0x6c, 0x18, 0x04, 0x20, 0x03, 0x28, 0x01, 0x12, 0x12, 0x08, 0x01,
        0x12, 0x0e, 0x72, 0x65, 0x71, 0x75, 0x69, 0x72, 0x65, 0x73, 0x20, 0x62, 0x61, 0x74, 0x63,
        0x68,
    ];

    assert_eq!(encoded, expected);

    let decoded = generated::protocol::v1::RpcMetadata::decode(encoded.as_slice()).unwrap();
    assert_eq!(decoded.result_streams.len(), 1);
    assert_eq!(decoded.result_streams[0].stream_name, RESERVATION_STREAM);
    assert_eq!(decoded.result_streams[0].columns[0].type_name, "bool");
    assert_eq!(
        decoded.completion_policy.unwrap().completion_shape,
        generated::protocol::v1::result_completion_policy::CompletionShape::RequiresRowBatch as i32
    );
}

#[test]
fn generated_rpc_execute_request_is_binary_projection() {
    let execute = reserve_stock_execute_request();
    let encoded_execute = execute.encode_to_vec();
    let decoded_execute =
        generated::protocol::v1::RpcExecuteRequest::decode(encoded_execute.as_slice()).unwrap();
    validate_generated_rpc_execute_request(&decoded_execute).unwrap();
    assert_eq!(decoded_execute.procedure_name, RESERVE_STOCK_PROCEDURE);
    assert_eq!(
        decoded_execute.expected_contract_hash,
        reserve_stock_contract_hash()
    );
    assert_eq!(decoded_execute.expected_stats_version, Some(6));
    assert_eq!(decoded_execute.arguments[0].value, 3_i64.to_le_bytes());
    assert!(decoded_execute.budget.is_some());
}

#[test]
fn generated_rpc_batch_is_binary_projection() {
    let batch = reservation_batch();
    let encoded_batch = batch.encode_to_vec();
    let decoded_batch =
        generated::protocol::v1::RpcBatch::decode(encoded_batch.as_slice()).unwrap();
    validate_generated_rpc_batch(&decoded_batch).unwrap();
    assert_eq!(decoded_batch.result_name, RESERVATION_RESULT);
    assert_eq!(decoded_batch.structured_payload, b"\x01");
    assert_eq!(decoded_batch.row_count_exact, Some(1));
    assert!(decoded_batch.terminal_batch);
}

fn typed_structured_object_fields() -> Vec<ColumnDescriptor> {
    vec![
        ColumnDescriptor {
            name: "product_id".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I64),
            ordinal: 0,
        },
        ColumnDescriptor {
            name: "quantity".to_string(),
            data_type: TypeDescriptor::required(ScalarType::I32),
            ordinal: 1,
        },
    ]
}

fn generated_structured_object_header() -> generated::contract::v1::StructuredObjectHeader {
    let layout = StructuredObjectLayout::RowMajor;
    let fields = typed_structured_object_fields();
    let descriptor_hash = StructuredObjectHeader::compute_descriptor_hash(&fields, layout);

    generated::contract::v1::StructuredObjectHeader {
        name: "ReservationRows".to_string(),
        contract_hash: vec![7; ContractHash::LEN],
        descriptor_hash: descriptor_hash.as_bytes().to_vec(),
        row_count_exact: 1,
        column_count: fields.len() as u32,
        layout: generated::contract::v1::structured_object_header::Layout::RowMajor as i32,
        payload_length: 16,
        payload_checksum: Some(0xABCD),
        max_payload_length: Some(64),
        fields: vec![
            generated::contract::v1::ColumnDescriptor {
                name: "product_id".to_string(),
                ordinal: 0,
                type_name: "i64".to_string(),
            },
            generated::contract::v1::ColumnDescriptor {
                name: "quantity".to_string(),
                ordinal: 1,
                type_name: "i32".to_string(),
            },
        ],
        row_count_policy:
            generated::contract::v1::result_stream_descriptor::RowCountRequirement::ExactRequired
                as i32,
    }
}

#[test]
fn generated_structured_object_header_projects_to_typed_runtime_model() {
    let header = generated_structured_object_header();

    let projected = project_generated_structured_object_header(&header).unwrap();
    validate_generated_structured_object_header(&header).unwrap();

    assert_eq!(projected.name, "ReservationRows");
    assert_eq!(projected.layout, StructuredObjectLayout::RowMajor);
    assert_eq!(
        projected.row_count_policy,
        RowCountRequirement::ExactRequired
    );
    assert_eq!(projected.row_count_exact, Some(1));
    assert_eq!(projected.column_count, 2);
    assert_eq!(projected.payload_length, 16);
    assert_eq!(projected.max_payload_length, Some(64));
    assert_eq!(projected.fields, typed_structured_object_fields());
}

#[test]
fn generated_structured_object_header_rejects_hash_and_descriptor_mismatch() {
    let valid = generated_structured_object_header();

    let invalid_contract_hash = generated::contract::v1::StructuredObjectHeader {
        contract_hash: vec![7; ContractHash::LEN - 1],
        ..valid.clone()
    };
    assert_eq!(
        project_generated_structured_object_header(&invalid_contract_hash)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

    let zero_descriptor_hash = generated::contract::v1::StructuredObjectHeader {
        descriptor_hash: vec![0; ContractHash::LEN],
        ..valid.clone()
    };
    assert_eq!(
        project_generated_structured_object_header(&zero_descriptor_hash)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

    let descriptor_mismatch = generated::contract::v1::StructuredObjectHeader {
        descriptor_hash: vec![9; ContractHash::LEN],
        ..valid
    };
    assert_eq!(
        project_generated_structured_object_header(&descriptor_mismatch)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );
}

#[test]
fn generated_structured_object_header_rejects_layout_fields_and_type_ambiguity() {
    let valid = generated_structured_object_header();

    let unspecified_layout = generated::contract::v1::StructuredObjectHeader {
        layout: generated::contract::v1::structured_object_header::Layout::Unspecified as i32,
        ..valid.clone()
    };
    assert!(project_generated_structured_object_header(&unspecified_layout).is_err());

    let duplicate_field_name = generated::contract::v1::StructuredObjectHeader {
        fields: vec![
            generated::contract::v1::ColumnDescriptor {
                name: "product_id".to_string(),
                ordinal: 0,
                type_name: "i64".to_string(),
            },
            generated::contract::v1::ColumnDescriptor {
                name: "product_id".to_string(),
                ordinal: 1,
                type_name: "i32".to_string(),
            },
        ],
        ..valid.clone()
    };
    assert!(project_generated_structured_object_header(&duplicate_field_name).is_err());

    let sparse_field_ordinal = generated::contract::v1::StructuredObjectHeader {
        fields: vec![
            generated::contract::v1::ColumnDescriptor {
                name: "product_id".to_string(),
                ordinal: 0,
                type_name: "i64".to_string(),
            },
            generated::contract::v1::ColumnDescriptor {
                name: "quantity".to_string(),
                ordinal: 2,
                type_name: "i32".to_string(),
            },
        ],
        ..valid.clone()
    };
    assert!(project_generated_structured_object_header(&sparse_field_ordinal).is_err());

    let unsupported_type = generated::contract::v1::StructuredObjectHeader {
        fields: vec![generated::contract::v1::ColumnDescriptor {
            name: "payload".to_string(),
            ordinal: 0,
            type_name: "string".to_string(),
        }],
        column_count: 1,
        ..valid
    };
    let err = project_generated_structured_object_header(&unsupported_type).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("minimal contract-safe projection"));
}

#[test]
fn generated_structured_object_header_fails_closed_on_row_count_presence_ambiguity() {
    let ambiguous_zero = generated::contract::v1::StructuredObjectHeader {
        row_count_exact: 0,
        payload_length: 0,
        max_payload_length: Some(0),
        ..generated_structured_object_header()
    };

    let err = project_generated_structured_object_header(&ambiguous_zero).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(err.message().contains("explicit presence"));
}

#[test]
fn generated_structured_object_header_rejects_payload_limit_violations() {
    let over_declared_max = generated::contract::v1::StructuredObjectHeader {
        payload_length: 65,
        max_payload_length: Some(64),
        ..generated_structured_object_header()
    };
    assert_eq!(
        project_generated_structured_object_header(&over_declared_max)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Protocol
    );

    let over_projection_cap = generated::contract::v1::StructuredObjectHeader {
        payload_length: 1,
        max_payload_length: Some(u64::MAX),
        ..generated_structured_object_header()
    };
    assert_eq!(
        project_generated_structured_object_header(&over_projection_cap)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );
}
