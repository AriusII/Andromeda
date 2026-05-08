#![forbid(unsafe_code)]

//! Runtime-free protobuf wire adapter helpers.

mod envelope;
mod generated;
mod hash;
mod protobuf;
mod result_metadata;

pub use envelope::{FrameEnvelope, PayloadKind, ProtocolVersion, RpcResultStreamMetadataPolicy};
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
