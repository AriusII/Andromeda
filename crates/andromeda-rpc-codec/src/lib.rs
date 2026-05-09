#![forbid(unsafe_code)]

//! Runtime-free RPC codec primitives.

mod catalog_manifest_resolution;
mod invocation_response;
mod procedure_gateway;
mod protobuf;
mod result_metadata;
pub mod typed_envelope;

pub use catalog_manifest_resolution::{
    CatalogColumnDescriptor, CatalogManifestResolutionFrameRequest,
    CatalogManifestResolutionRequest, CatalogManifestResolutionResponse,
    CatalogManifestResolutionStatus, CatalogManifestSelector, CatalogProcedureManifest,
    CatalogProcedureManifestResolutionRequest, CatalogProcedureManifestResolutionResponse,
    CatalogProcedureProtocolLayout, CatalogRequiredPermission, CatalogResultStreamDescriptor,
    catalog_manifest_resolution_request_frame, catalog_manifest_resolution_status_from_protobuf,
    catalog_manifest_resolution_status_from_protobuf_i32,
    catalog_manifest_resolution_status_to_protobuf,
    decode_catalog_manifest_resolution_request_frame,
    decode_catalog_manifest_resolution_response_frame,
    decode_catalog_manifest_resolution_route_request,
    encode_catalog_manifest_resolution_response_frame,
    validate_catalog_manifest_resolution_request_context,
    validate_catalog_manifest_resolution_response_context,
};
pub use invocation_response::{
    ExecutionResult, ResultFrame, ResultStreamDecoder, decode_invocation_response,
    decode_result_stream,
};
pub use procedure_gateway::decode_and_validate_rpc_execute_request;
pub use protobuf::{decode_protobuf_message, encode_protobuf_message};
pub use result_metadata::{
    StructuredPayloadByteTracker, validate_result_batch_payload_parts,
    validate_rpc_execute_argument_value, validate_rpc_execute_arguments_len,
    validate_rpc_response_sequence_len,
};
pub use typed_envelope::{
    DEFAULT_MAX_TYPED_RESULT_STREAM_ENVELOPE_BYTES, DEFAULT_MAX_TYPED_RESULT_STREAM_FRAMES,
    TypedResultStreamBounds, TypedResultStreamContext, decode_typed_frame_envelope,
    validate_typed_result_stream_sequence,
    validate_typed_result_stream_sequence_with_context_and_bounds,
    validate_typed_result_stream_sequence_with_metadata_policy,
};
