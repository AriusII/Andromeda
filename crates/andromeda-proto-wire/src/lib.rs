#![forbid(unsafe_code)]

//! Runtime-free protobuf wire adapter helpers.

mod envelope;
mod generated;
pub mod generated_validation;
mod hash;
mod protobuf;
mod result_metadata;

pub use envelope::{
    AUTH_WIRE_CODE, CONTRACT_REQUEST_WIRE_CODE, CONTRACT_RESPONSE_WIRE_CODE, ERROR_WIRE_CODE,
    FrameEnvelope, HELLO_WIRE_CODE, PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP, PayloadFrameFamily,
    PayloadFrameMapping, PayloadKind, ProtocolVersion, RPC_BATCH_WIRE_CODE,
    RPC_COMPLETION_WIRE_CODE, RPC_EXECUTE_REQUEST_WIRE_CODE, RPC_METADATA_WIRE_CODE,
    RpcResultStreamMetadataPolicy, project_generated_frame_envelope,
    project_generated_payload_kind, project_generated_protocol_version,
};
pub use andromeda_rpc_protocol::{
    BackpressureMetadata, ErrorEnvelope, ErrorFamily, RetryDisposition, TransactionEffect,
};
pub use generated::{
    CONTRACT_PACKAGE, DESCRIPTOR_SET_HASH_ALGORITHM, GeneratedColumnDescriptor,
    GeneratedCompletionShape, GeneratedFrameEnvelope, GeneratedPayloadKind,
    GeneratedProtocolVersion, GeneratedResultCompletionPolicy, GeneratedResultRowCountSummary,
    GeneratedResultStreamCardinality, GeneratedResultStreamDescriptor,
    GeneratedRowCountRequirement, GeneratedRpcBatch, GeneratedRpcCompletion,
    GeneratedRpcCompletionStatus, GeneratedRpcMetadata, GeneratedTransactionOutcome,
    PROTOCOL_FRAME_ENVELOPE_TYPE, PROTOCOL_PACKAGE, descriptor_set_hash, frame_envelope_hash,
    protocol_layout,
};
pub use generated_validation::{
    project_generated_structured_object_header,
    validate_catalog_procedure_manifest_resolution_request,
    validate_catalog_procedure_manifest_resolution_response, validate_generated_error_envelope,
    validate_generated_frame_envelope, validate_generated_invocation_request,
    validate_generated_invocation_response, validate_generated_invocation_response_sequence,
    validate_generated_procedure_manifest, validate_generated_protocol_version,
    validate_generated_result_streams, validate_generated_rpc_batch,
    validate_generated_rpc_completion, validate_generated_rpc_execute_request,
    validate_generated_rpc_metadata, validate_generated_structured_object_header,
};
pub use hash::{
    stable_contract_hash, stable_hash_256, validate_optional_contract_hash,
    validate_required_contract_hash,
};
pub use protobuf::{decode_protobuf_message, encode_protobuf_message};
pub use result_metadata::{
    StructuredPayloadByteTracker, validate_result_batch_payload_parts,
    validate_rpc_execute_argument_value, validate_rpc_execute_arguments_len,
    validate_rpc_response_sequence_len,
};
