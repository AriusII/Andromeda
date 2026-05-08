#![forbid(unsafe_code)]

//! Runtime-free protobuf wire adapter helpers.

mod completion;
mod envelope;
mod generated;
mod hash;
mod protobuf;
mod result_metadata;

pub use envelope::{
    AUTH_WIRE_CODE, CONTRACT_REQUEST_WIRE_CODE, CONTRACT_RESPONSE_WIRE_CODE, ERROR_WIRE_CODE,
    FrameEnvelope, HELLO_WIRE_CODE, PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP, PayloadFrameFamily,
    PayloadFrameMapping, PayloadKind, ProtocolVersion, RPC_BATCH_WIRE_CODE,
    RPC_COMPLETION_WIRE_CODE, RPC_EXECUTE_REQUEST_WIRE_CODE, RPC_METADATA_WIRE_CODE,
    RpcResultStreamMetadataPolicy,
};
pub use generated::{
    GeneratedColumnDescriptor, GeneratedCompletionShape, GeneratedFrameEnvelope,
    GeneratedPayloadKind, GeneratedProtocolVersion, GeneratedResultCompletionPolicy,
    GeneratedResultRowCountSummary, GeneratedResultStreamCardinality,
    GeneratedResultStreamDescriptor, GeneratedRowCountRequirement, GeneratedRpcBatch,
    GeneratedRpcCompletion, GeneratedRpcCompletionStatus, GeneratedRpcMetadata,
    GeneratedTransactionOutcome,
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
