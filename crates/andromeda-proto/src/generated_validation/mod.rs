use andromeda_error::AndromedaResult;

use crate::FrameEnvelope;
use crate::StructuredObjectHeader;
use crate::generated::{contract, protocol};

mod views;

pub fn validate_catalog_procedure_manifest_resolution_request(
    request: &contract::v1::CatalogProcedureManifestResolutionRequest,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_catalog_procedure_manifest_resolution_request(request)
}

pub fn validate_catalog_procedure_manifest_resolution_response(
    response: &contract::v1::CatalogProcedureManifestResolutionResponse,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_catalog_procedure_manifest_resolution_response(response)
}

pub fn project_generated_frame_envelope(
    envelope: &protocol::v1::FrameEnvelope,
) -> AndromedaResult<FrameEnvelope> {
    andromeda_proto_wire::generated_validation::project_generated_frame_envelope(envelope)
}

pub fn validate_generated_frame_envelope(
    envelope: &protocol::v1::FrameEnvelope,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_frame_envelope(envelope)
}

pub fn validate_generated_rpc_metadata(
    metadata: &protocol::v1::RpcMetadata,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_rpc_metadata(metadata)
}

pub fn validate_generated_rpc_batch(batch: &protocol::v1::RpcBatch) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_rpc_batch(batch)
}

pub fn validate_generated_rpc_completion(
    completion: &protocol::v1::RpcCompletion,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_rpc_completion(completion)
}

pub fn validate_generated_error_envelope(
    error: &protocol::v1::ErrorEnvelope,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_error_envelope(error)
}

pub fn validate_generated_rpc_execute_request(
    request: &protocol::v1::RpcExecuteRequest,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_rpc_execute_request(request)
}

pub fn validate_generated_invocation_request(
    request: &protocol::v1::InvocationRequest,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_invocation_request(request)
}

pub fn validate_generated_invocation_response(
    response: &protocol::v1::InvocationResponse,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_invocation_response(response)
}

pub fn validate_generated_invocation_response_sequence(
    responses: &[protocol::v1::InvocationResponse],
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_invocation_response_sequence(responses)
}

pub fn project_generated_structured_object_header(
    header: &contract::v1::StructuredObjectHeader,
) -> AndromedaResult<StructuredObjectHeader> {
    andromeda_proto_wire::project_generated_structured_object_header(header)
}

pub fn validate_generated_structured_object_header(
    header: &contract::v1::StructuredObjectHeader,
) -> AndromedaResult<()> {
    andromeda_proto_wire::validate_generated_structured_object_header(header)
}
