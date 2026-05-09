#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Protobuf Schema Boundary

This crate owns the `.proto` sources, prost build output, generated module
wrappers, descriptor-set bytes, and schema-governance hashes for Andromeda
protocol and contract schemas.

Runtime-free protocol ownership is intentionally split:

| Surface | Owner |
| --- | --- |
| Generated prost modules and descriptor hashes | `andromeda-proto` |
| Generated message validation and protobuf projection | `andromeda-proto-wire` |
| Frame, stream, envelope, backpressure, and protocol invariants | `andromeda-rpc-protocol` |
| Typed frame/envelope codec helpers | `andromeda-rpc-codec` |
| Procedure contract and gateway manifest semantics | `andromeda-procedure-contract` |
| Structured object layout and row-count descriptors | `andromeda-structured-object` |

`andromeda-proto` is kept as the schema and governance surface: generated prost
access, descriptor bytes, and schema hashes remain anchored at
`andromeda_proto::generated`. Compatibility adapters may remain while callers
are migrated, but new runtime code should import validation and wire behavior
from the owner crates above instead of treating this crate as a general
protocol surface.

Schema files must remain deterministic, service-free, and runtime-free. They do
not define gRPC services, HTTP transports, JSON mapping policy, or executor
behavior.
"#]

pub mod generated;
mod generated_validation;

pub use andromeda_procedure_contract::{
    CompletionEnvelopeVersion, CompletionProtocolVersion, CompletionTerminalCode,
    ManifestPolicyVersion, ProcedureManifest, ProcedureManifestBinding, ProtocolLayout,
    RequiredPermission, ResultCardinality, ResultRowCountSummary, ResultStreamDescriptor,
    RowCountRequirement, RpcCompletion, RpcCompletionStatus, TransactionOutcome,
};
pub use andromeda_proto_wire::{
    AUTH_WIRE_CODE, CONTRACT_REQUEST_WIRE_CODE, CONTRACT_RESPONSE_WIRE_CODE, ERROR_WIRE_CODE,
    FrameEnvelope, HELLO_WIRE_CODE, PAYLOAD_KIND_TRANSPORT_CODE_LOCKSTEP, PayloadFrameFamily,
    PayloadFrameMapping, PayloadKind, ProtocolVersion, RPC_BATCH_WIRE_CODE,
    RPC_COMPLETION_WIRE_CODE, RPC_EXECUTE_REQUEST_WIRE_CODE, RPC_METADATA_WIRE_CODE,
    RpcResultStreamMetadataPolicy,
};
pub use andromeda_proto_wire::{
    BackpressureMetadata, COMPLETION_ENVELOPE_VERSION, ErrorEnvelope, ErrorFamily,
    RetryDisposition, TransactionEffect,
};
pub use andromeda_proto_wire::{
    CONTRACT_PACKAGE, DESCRIPTOR_SET_HASH_ALGORITHM, PROTOCOL_FRAME_ENVELOPE_TYPE, PROTOCOL_PACKAGE,
};
pub use andromeda_structured_object::{
    RowCountPolicy, StructuredObjectHeader, StructuredObjectLayout,
};
pub use generated::{
    decode_generated_message, descriptor_set_bytes, descriptor_set_hash, encode_generated_message,
    frame_envelope_hash, project_generated_frame_envelope,
    project_generated_structured_object_header, protocol_layout,
    validate_catalog_procedure_manifest_resolution_request,
    validate_catalog_procedure_manifest_resolution_response, validate_generated_error_envelope,
    validate_generated_frame_envelope, validate_generated_invocation_request,
    validate_generated_invocation_response, validate_generated_invocation_response_sequence,
    validate_generated_rpc_batch, validate_generated_rpc_completion,
    validate_generated_rpc_execute_request, validate_generated_rpc_metadata,
    validate_generated_structured_object_header,
};
