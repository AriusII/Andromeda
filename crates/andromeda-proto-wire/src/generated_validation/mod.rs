mod catalog_manifest_resolution;
mod common;
mod error;
mod frame_envelope;
mod invocation;
mod procedure_manifest;
mod result_stream;
mod rpc_result;
mod structured_object;
mod views;

pub use catalog_manifest_resolution::{
    validate_catalog_procedure_manifest_resolution_request,
    validate_catalog_procedure_manifest_resolution_response,
};
pub use error::validate_generated_error_envelope;
pub use frame_envelope::{
    project_generated_frame_envelope, validate_generated_frame_envelope,
    validate_generated_protocol_version,
};
pub use invocation::{
    validate_generated_invocation_request, validate_generated_invocation_response,
    validate_generated_invocation_response_sequence, validate_generated_rpc_execute_request,
};
pub use procedure_manifest::validate_generated_procedure_manifest;
pub use result_stream::validate_generated_result_streams;
pub use rpc_result::{
    validate_generated_rpc_batch, validate_generated_rpc_completion,
    validate_generated_rpc_metadata,
};
pub use structured_object::{
    project_generated_structured_object_header, validate_generated_structured_object_header,
};
pub use views::{
    GeneratedBackpressureMetadataView, GeneratedCatalogManifestResolutionRequestView,
    GeneratedCatalogManifestResolutionResponseView, GeneratedCatalogManifestResolutionSelector,
    GeneratedColumnDescriptorView, GeneratedErrorEnvelopeView, GeneratedFrameEnvelopeView,
    GeneratedInvocationCorrelationView, GeneratedInvocationRequestView,
    GeneratedInvocationResponsePayload, GeneratedInvocationResponsePayloadFor,
    GeneratedInvocationResponseView, GeneratedProcedureManifestView, GeneratedProtocolLayoutView,
    GeneratedProtocolVersionView, GeneratedRequiredPermissionView,
    GeneratedResultCompletionPolicyView, GeneratedResultRowCountSummaryView,
    GeneratedResultStreamDescriptorView, GeneratedRpcBatchView, GeneratedRpcCompletionView,
    GeneratedRpcExecuteArgumentView, GeneratedRpcExecuteRequestBudgetView,
    GeneratedRpcExecuteRequestView, GeneratedRpcMetadataView, GeneratedStructuredObjectHeaderView,
};
