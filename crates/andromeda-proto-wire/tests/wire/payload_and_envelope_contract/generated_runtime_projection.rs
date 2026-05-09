use andromeda_error::AndromedaErrorKind;
use andromeda_procedure_contract::RowCountRequirement;
use andromeda_proto_wire::generated_validation::{
    GeneratedInvocationCorrelationView, GeneratedInvocationResponsePayload,
    GeneratedInvocationResponseView, GeneratedRpcExecuteArgumentView,
    GeneratedRpcExecuteRequestBudgetView, GeneratedRpcExecuteRequestView,
    GeneratedStructuredObjectHeaderView,
};
use andromeda_proto_wire::{
    GeneratedColumnDescriptor, GeneratedFrameEnvelope, GeneratedPayloadKind,
    GeneratedProtocolVersion, GeneratedRowCountRequirement, GeneratedRpcBatch,
    GeneratedRpcCompletion, GeneratedRpcCompletionStatus, GeneratedRpcMetadata,
    GeneratedTransactionOutcome, project_generated_structured_object_header,
    validate_generated_frame_envelope, validate_generated_invocation_response_sequence,
    validate_generated_rpc_batch, validate_generated_rpc_completion,
    validate_generated_rpc_execute_request, validate_generated_rpc_metadata,
    validate_generated_structured_object_header,
};
use andromeda_structured_object::{StructuredObjectHeader, StructuredObjectLayout};
use andromeda_types::{ColumnDescriptor, ContractHash, ScalarType, TypeDescriptor};
use prost::Message;

use super::proto_wire_fixtures::{
    RESERVATION_RESULT, RESERVATION_STREAM, RESERVE_STOCK_CATALOG_VERSION, RESERVE_STOCK_PROCEDURE,
    RESERVE_STOCK_STATS_VERSION, committed_reservation_completion, generated_protocol_v1,
    payload_kind_rpc_execute_request, reservation_batch, reserve_stock_contract_hash,
    reserve_stock_metadata,
};

#[test]
fn proto_wire_exports_generated_validators_for_wire_owned_dtos() {
    let frame = GeneratedFrameEnvelope {
        protocol_version: Some(generated_protocol_v1()),
        contract_hash: vec![7; ContractHash::LEN],
        catalog_version: 11,
        request_id: 101,
        session_id: 202,
        tx_id: Some(303),
        payload_kind: GeneratedPayloadKind::RpcExecuteRequest as i32,
        payload: b"abc".to_vec(),
    };
    validate_generated_frame_envelope(&frame).unwrap();

    let metadata = reserve_stock_metadata();
    validate_generated_rpc_metadata(&metadata).unwrap();

    let batch = reservation_batch();
    validate_generated_rpc_batch(&batch).unwrap();

    let completion = committed_reservation_completion();
    validate_generated_rpc_completion(&completion).unwrap();

    let correlation = TestInvocationCorrelation::valid();
    let responses = vec![
        TestInvocationResponse::metadata(0, correlation.clone(), metadata),
        TestInvocationResponse::batch(1, correlation.clone(), batch),
        TestInvocationResponse::completion(2, correlation, completion),
    ];
    validate_generated_invocation_response_sequence(&responses).unwrap();
}

#[test]
fn generated_rpc_execute_request_validation_rejects_default_runtime_bindings() {
    let valid = TestRpcExecuteRequest::valid();
    assert!(validate_generated_rpc_execute_request(&valid).is_ok());

    let invalid_cases = [
        (
            "procedure_name",
            TestRpcExecuteRequest {
                procedure_name: " ".to_string(),
                ..valid.clone()
            },
        ),
        (
            "expected_contract_hash",
            TestRpcExecuteRequest {
                expected_contract_hash: vec![9; ContractHash::LEN - 1],
                ..valid.clone()
            },
        ),
        (
            "expected_catalog_version",
            TestRpcExecuteRequest {
                expected_catalog_version: 0,
                ..valid.clone()
            },
        ),
        (
            "surface_scope",
            TestRpcExecuteRequest {
                surface_scope: String::new(),
                ..valid.clone()
            },
        ),
        (
            "expected_stats_version",
            TestRpcExecuteRequest {
                expected_stats_version: None,
                ..valid.clone()
            },
        ),
        (
            "argument value",
            TestRpcExecuteRequest {
                arguments: vec![TestRpcExecuteArgument {
                    name: "Quantity".to_string(),
                    type_name: "i64".to_string(),
                    value: Vec::new(),
                }],
                ..valid.clone()
            },
        ),
        (
            "budget priority_class",
            TestRpcExecuteRequest {
                budget: Some(TestRequestBudget {
                    priority_class: Some(0),
                    ..valid.budget.clone().unwrap()
                }),
                ..valid
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
    let envelope = GeneratedFrameEnvelope {
        protocol_version: Some(GeneratedProtocolVersion { major: 1, minor: 0 }),
        contract_hash: vec![7; ContractHash::LEN],
        catalog_version: 11,
        request_id: 101,
        session_id: 202,
        tx_id: Some(303),
        payload_kind: payload_kind_rpc_execute_request(),
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

    let decoded = GeneratedFrameEnvelope::decode(encoded.as_slice()).unwrap();
    assert_eq!(
        decoded.payload_kind,
        GeneratedPayloadKind::RpcExecuteRequest as i32
    );
    assert_eq!(decoded.contract_hash, vec![7; ContractHash::LEN]);
    assert_eq!(decoded.request_id, 101);
    assert_eq!(decoded.session_id, 202);
    assert_eq!(decoded.tx_id, Some(303));
    assert_eq!(decoded.payload, b"abc");
}

#[test]
fn generated_rpc_completion_has_stable_wire_projection() {
    let completion = GeneratedRpcCompletion {
        status: GeneratedRpcCompletionStatus::Committed as i32,
        rows_affected: Some(42),
        tx_id: Some(77),
        request_id: Some(101),
        session_id: Some(202),
        trace_id: Some("trace".to_string()),
        transaction_outcome: GeneratedTransactionOutcome::Committed as i32,
        durable_lsn: Some(7),
        result_row_counts: vec![andromeda_proto_wire::GeneratedResultRowCountSummary {
            result_name: RESERVATION_STREAM.to_string(),
            rows_emitted: 1,
            row_count_exact: Some(1),
        }],
    };

    let encoded = completion.encode_to_vec();
    let expected = [
        0x08, 0x01, 0x10, 0x2a, 0x18, 0x4d, 0x80, 0x02, 0x65, 0x88, 0x02, 0xca, 0x01, 0x92, 0x02,
        0x05, 0x74, 0x72, 0x61, 0x63, 0x65, 0x98, 0x02, 0x02, 0xa0, 0x02, 0x07, 0xaa, 0x02, 0x11,
        0x0a, 0x0b, 0x52, 0x65, 0x73, 0x65, 0x72, 0x76, 0x61, 0x74, 0x69, 0x6f, 0x6e, 0x10, 0x01,
        0x18, 0x01,
    ];

    assert_eq!(encoded, expected);

    let decoded = GeneratedRpcCompletion::decode(encoded.as_slice()).unwrap();
    assert_eq!(
        decoded.status,
        GeneratedRpcCompletionStatus::Committed as i32
    );
    assert_eq!(decoded.rows_affected, Some(42));
    assert_eq!(decoded.tx_id, Some(77));
    assert_eq!(decoded.durable_lsn, Some(7));
    assert_eq!(decoded.result_row_counts.len(), 1);
    assert_eq!(decoded.result_row_counts[0].row_count_exact, Some(1));
}

#[test]
fn generated_rpc_execute_request_is_valid_wire_view_projection() {
    let execute = TestRpcExecuteRequest::valid();
    validate_generated_rpc_execute_request(&execute).unwrap();
    assert_eq!(execute.procedure_name, RESERVE_STOCK_PROCEDURE);
    assert_eq!(
        execute.expected_contract_hash,
        reserve_stock_contract_hash()
    );
    assert_eq!(
        execute.expected_stats_version,
        Some(RESERVE_STOCK_STATS_VERSION)
    );
    assert_eq!(execute.arguments[0].value, 3_i64.to_le_bytes());
    assert!(execute.budget.is_some());
}

#[test]
fn generated_rpc_batch_is_binary_projection() {
    let batch = reservation_batch();
    let encoded_batch = batch.encode_to_vec();
    let decoded_batch = GeneratedRpcBatch::decode(encoded_batch.as_slice()).unwrap();
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

fn generated_structured_object_header() -> TestStructuredObjectHeader {
    let layout = StructuredObjectLayout::RowMajor;
    let fields = typed_structured_object_fields();
    let descriptor_hash = StructuredObjectHeader::compute_descriptor_hash(&fields, layout);

    TestStructuredObjectHeader {
        name: "ReservationRows".to_string(),
        contract_hash: vec![7; ContractHash::LEN],
        descriptor_hash: descriptor_hash.as_bytes().to_vec(),
        row_count_exact: 1,
        column_count: fields.len() as u32,
        layout: 1,
        payload_length: 16,
        payload_checksum: Some(0xABCD),
        max_payload_length: Some(64),
        fields: vec![
            GeneratedColumnDescriptor {
                name: "product_id".to_string(),
                ordinal: 0,
                type_name: "i64".to_string(),
            },
            GeneratedColumnDescriptor {
                name: "quantity".to_string(),
                ordinal: 1,
                type_name: "i32".to_string(),
            },
        ],
        row_count_policy: GeneratedRowCountRequirement::ExactRequired as i32,
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

    let invalid_contract_hash = TestStructuredObjectHeader {
        contract_hash: vec![7; ContractHash::LEN - 1],
        ..valid.clone()
    };
    assert_eq!(
        project_generated_structured_object_header(&invalid_contract_hash)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

    let zero_descriptor_hash = TestStructuredObjectHeader {
        descriptor_hash: vec![0; ContractHash::LEN],
        ..valid.clone()
    };
    assert_eq!(
        project_generated_structured_object_header(&zero_descriptor_hash)
            .unwrap_err()
            .kind(),
        AndromedaErrorKind::Contract
    );

    let descriptor_mismatch = TestStructuredObjectHeader {
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

    let unspecified_layout = TestStructuredObjectHeader {
        layout: 0,
        ..valid.clone()
    };
    assert!(project_generated_structured_object_header(&unspecified_layout).is_err());

    let duplicate_field_name = TestStructuredObjectHeader {
        fields: vec![
            GeneratedColumnDescriptor {
                name: "product_id".to_string(),
                ordinal: 0,
                type_name: "i64".to_string(),
            },
            GeneratedColumnDescriptor {
                name: "product_id".to_string(),
                ordinal: 1,
                type_name: "i32".to_string(),
            },
        ],
        ..valid.clone()
    };
    assert!(project_generated_structured_object_header(&duplicate_field_name).is_err());

    let sparse_field_ordinal = TestStructuredObjectHeader {
        fields: vec![
            GeneratedColumnDescriptor {
                name: "product_id".to_string(),
                ordinal: 0,
                type_name: "i64".to_string(),
            },
            GeneratedColumnDescriptor {
                name: "quantity".to_string(),
                ordinal: 2,
                type_name: "i32".to_string(),
            },
        ],
        ..valid.clone()
    };
    assert!(project_generated_structured_object_header(&sparse_field_ordinal).is_err());

    let unsupported_type = TestStructuredObjectHeader {
        fields: vec![GeneratedColumnDescriptor {
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
    let ambiguous_zero = TestStructuredObjectHeader {
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
    let over_declared_max = TestStructuredObjectHeader {
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

    let over_projection_cap = TestStructuredObjectHeader {
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

#[derive(Clone)]
struct TestRpcExecuteRequest {
    procedure_name: String,
    expected_contract_hash: Vec<u8>,
    expected_catalog_version: u64,
    surface_scope: String,
    arguments: Vec<TestRpcExecuteArgument>,
    budget: Option<TestRequestBudget>,
    expected_stats_version: Option<u64>,
}

impl TestRpcExecuteRequest {
    fn valid() -> Self {
        Self {
            procedure_name: RESERVE_STOCK_PROCEDURE.to_string(),
            expected_contract_hash: reserve_stock_contract_hash(),
            expected_catalog_version: RESERVE_STOCK_CATALOG_VERSION,
            surface_scope: "application".to_string(),
            arguments: vec![TestRpcExecuteArgument {
                name: "Quantity".to_string(),
                type_name: "i64".to_string(),
                value: 3_i64.to_le_bytes().to_vec(),
            }],
            budget: Some(TestRequestBudget {
                cpu_micros: Some(5_000),
                memory_bytes: Some(64 * 1024),
                io_bytes: Some(128 * 1024),
                priority_class: Some(1),
            }),
            expected_stats_version: Some(RESERVE_STOCK_STATS_VERSION),
        }
    }
}

impl GeneratedRpcExecuteRequestView for TestRpcExecuteRequest {
    type Argument = TestRpcExecuteArgument;
    type Budget = TestRequestBudget;

    fn procedure_name(&self) -> &str {
        &self.procedure_name
    }

    fn expected_contract_hash(&self) -> &[u8] {
        &self.expected_contract_hash
    }

    fn expected_catalog_version(&self) -> u64 {
        self.expected_catalog_version
    }

    fn surface_scope(&self) -> &str {
        &self.surface_scope
    }

    fn arguments(&self) -> &[Self::Argument] {
        &self.arguments
    }

    fn budget(&self) -> Option<&Self::Budget> {
        self.budget.as_ref()
    }

    fn expected_stats_version(&self) -> Option<u64> {
        self.expected_stats_version
    }
}

#[derive(Clone)]
struct TestRpcExecuteArgument {
    name: String,
    type_name: String,
    value: Vec<u8>,
}

impl GeneratedRpcExecuteArgumentView for TestRpcExecuteArgument {
    fn name(&self) -> &str {
        &self.name
    }

    fn type_name(&self) -> &str {
        &self.type_name
    }

    fn value(&self) -> &[u8] {
        &self.value
    }
}

#[derive(Clone)]
struct TestRequestBudget {
    cpu_micros: Option<u64>,
    memory_bytes: Option<u64>,
    io_bytes: Option<u64>,
    priority_class: Option<u32>,
}

impl GeneratedRpcExecuteRequestBudgetView for TestRequestBudget {
    fn cpu_micros(&self) -> Option<u64> {
        self.cpu_micros
    }

    fn memory_bytes(&self) -> Option<u64> {
        self.memory_bytes
    }

    fn io_bytes(&self) -> Option<u64> {
        self.io_bytes
    }

    fn priority_class(&self) -> Option<u32> {
        self.priority_class
    }
}

#[derive(Clone)]
struct TestInvocationCorrelation {
    request_id: Option<u64>,
    session_id: Option<u64>,
    trace_id: Option<String>,
    contract_hash: Option<Vec<u8>>,
    catalog_version: Option<u64>,
    invocation_id: Option<u64>,
    stats_version: Option<u64>,
    expected_policy_version: Option<u64>,
}

impl TestInvocationCorrelation {
    fn valid() -> Self {
        Self {
            request_id: Some(101),
            session_id: Some(202),
            trace_id: Some("trace-proto-101".to_string()),
            contract_hash: Some(reserve_stock_contract_hash()),
            catalog_version: Some(RESERVE_STOCK_CATALOG_VERSION),
            invocation_id: Some(303),
            stats_version: Some(RESERVE_STOCK_STATS_VERSION),
            expected_policy_version: Some(11),
        }
    }
}

impl GeneratedInvocationCorrelationView for TestInvocationCorrelation {
    fn request_id(&self) -> Option<u64> {
        self.request_id
    }

    fn session_id(&self) -> Option<u64> {
        self.session_id
    }

    fn trace_id(&self) -> Option<&str> {
        self.trace_id.as_deref()
    }

    fn contract_hash(&self) -> Option<&[u8]> {
        self.contract_hash.as_deref()
    }

    fn catalog_version(&self) -> Option<u64> {
        self.catalog_version
    }

    fn invocation_id(&self) -> Option<u64> {
        self.invocation_id
    }

    fn stats_version(&self) -> Option<u64> {
        self.stats_version
    }

    fn expected_policy_version(&self) -> Option<u64> {
        self.expected_policy_version
    }
}

enum TestInvocationPayload {
    Metadata(GeneratedRpcMetadata),
    Batch(GeneratedRpcBatch),
    Completion(GeneratedRpcCompletion),
}

struct TestInvocationResponse {
    correlation: Option<TestInvocationCorrelation>,
    response_index: Option<u64>,
    payload: Option<TestInvocationPayload>,
}

impl TestInvocationResponse {
    fn metadata(
        index: u64,
        correlation: TestInvocationCorrelation,
        metadata: GeneratedRpcMetadata,
    ) -> Self {
        Self {
            correlation: Some(correlation),
            response_index: Some(index),
            payload: Some(TestInvocationPayload::Metadata(metadata)),
        }
    }

    fn batch(index: u64, correlation: TestInvocationCorrelation, batch: GeneratedRpcBatch) -> Self {
        Self {
            correlation: Some(correlation),
            response_index: Some(index),
            payload: Some(TestInvocationPayload::Batch(batch)),
        }
    }

    fn completion(
        index: u64,
        correlation: TestInvocationCorrelation,
        completion: GeneratedRpcCompletion,
    ) -> Self {
        Self {
            correlation: Some(correlation),
            response_index: Some(index),
            payload: Some(TestInvocationPayload::Completion(completion)),
        }
    }
}

impl GeneratedInvocationResponseView for TestInvocationResponse {
    type Correlation = TestInvocationCorrelation;
    type Metadata = GeneratedRpcMetadata;
    type Batch = GeneratedRpcBatch;
    type Completion = GeneratedRpcCompletion;
    type Error = TestErrorEnvelope;

    fn correlation(&self) -> Option<&Self::Correlation> {
        self.correlation.as_ref()
    }

    fn response_index(&self) -> Option<u64> {
        self.response_index
    }

    fn payload(
        &self,
    ) -> Option<
        GeneratedInvocationResponsePayload<
            '_,
            Self::Metadata,
            Self::Batch,
            Self::Completion,
            Self::Error,
        >,
    > {
        match self.payload.as_ref()? {
            TestInvocationPayload::Metadata(metadata) => {
                Some(GeneratedInvocationResponsePayload::Metadata(metadata))
            },
            TestInvocationPayload::Batch(batch) => {
                Some(GeneratedInvocationResponsePayload::Batch(batch))
            },
            TestInvocationPayload::Completion(completion) => {
                Some(GeneratedInvocationResponsePayload::Completion(completion))
            },
        }
    }
}

struct TestErrorEnvelope;

impl andromeda_proto_wire::generated_validation::GeneratedErrorEnvelopeView for TestErrorEnvelope {
    type Backpressure = TestBackpressure;

    fn request_id(&self) -> Option<u64> {
        Some(101)
    }

    fn session_id(&self) -> Option<u64> {
        Some(202)
    }

    fn trace_id(&self) -> Option<&str> {
        Some("trace-proto-101")
    }

    fn family(&self) -> i32 {
        1
    }

    fn code(&self) -> &str {
        "CONTRACT"
    }

    fn message(&self) -> &str {
        "contract error"
    }

    fn transaction_effect(&self) -> i32 {
        1
    }

    fn retry_disposition(&self) -> i32 {
        1
    }

    fn retry_after_ms(&self) -> Option<u64> {
        None
    }

    fn backpressure(&self) -> Option<&Self::Backpressure> {
        None
    }
}

struct TestBackpressure;

impl andromeda_proto_wire::generated_validation::GeneratedBackpressureMetadataView
    for TestBackpressure
{
    fn retry_after_ms(&self) -> Option<u64> {
        None
    }

    fn capacity_percent(&self) -> Option<u32> {
        None
    }
}

#[derive(Clone)]
struct TestStructuredObjectHeader {
    name: String,
    contract_hash: Vec<u8>,
    descriptor_hash: Vec<u8>,
    row_count_exact: u64,
    column_count: u32,
    layout: i32,
    payload_length: u64,
    payload_checksum: Option<u64>,
    max_payload_length: Option<u64>,
    fields: Vec<GeneratedColumnDescriptor>,
    row_count_policy: i32,
}

impl GeneratedStructuredObjectHeaderView for TestStructuredObjectHeader {
    type Column = GeneratedColumnDescriptor;

    fn name(&self) -> &str {
        &self.name
    }

    fn contract_hash(&self) -> &[u8] {
        &self.contract_hash
    }

    fn descriptor_hash(&self) -> &[u8] {
        &self.descriptor_hash
    }

    fn row_count_exact(&self) -> u64 {
        self.row_count_exact
    }

    fn column_count(&self) -> u32 {
        self.column_count
    }

    fn layout(&self) -> i32 {
        self.layout
    }

    fn payload_length(&self) -> u64 {
        self.payload_length
    }

    fn payload_checksum(&self) -> Option<u64> {
        self.payload_checksum
    }

    fn max_payload_length(&self) -> Option<u64> {
        self.max_payload_length
    }

    fn fields(&self) -> &[Self::Column] {
        &self.fields
    }

    fn row_count_policy(&self) -> i32 {
        self.row_count_policy
    }
}
