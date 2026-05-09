use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{
    ProcedureGatewayExecuteRequest, validate_procedure_gateway_manifest_permissions,
};
use andromeda_proto::generated;
use andromeda_proto_wire::{
    PayloadKind, decode_protobuf_message, validate_generated_rpc_execute_request,
};
use andromeda_types::{CatalogVersion, ContractHash};

use crate::catalog_manifest_resolution::validate_catalog_procedure_manifest_projection;
use crate::{CatalogProcedureManifest, decode_typed_frame_envelope};
use andromeda_rpc_protocol::FrameBytes;

type GeneratedRpcExecuteRequest = generated::protocol::v1::RpcExecuteRequest;

/// Decodes and validates an Application Procedure execute request against a manifest.
pub fn decode_and_validate_rpc_execute_request(
    frame: &FrameBytes,
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<ProcedureGatewayExecuteRequest> {
    validate_procedure_gateway_manifest_permissions(manifest)?;
    validate_catalog_procedure_manifest_projection(manifest)?;

    let envelope = decode_typed_frame_envelope(frame)?;
    if envelope.payload_kind != PayloadKind::RpcExecuteRequest {
        return Err(protocol_error(
            "procedure invocation envelope payload kind must be RpcExecuteRequest",
        ));
    }
    if envelope.tx_id.is_some() {
        return Err(protocol_error(
            "procedure invocation envelope must not carry client transaction id",
        ));
    }

    let generated_request: GeneratedRpcExecuteRequest =
        decode_protobuf_message(envelope.payload.as_slice(), "generated protobuf")?;
    validate_generated_rpc_execute_request(&generated_request)?;
    let execute_request = validate_rpc_execute_request(&generated_request)?;

    validate_request_matches_manifest(&envelope, &execute_request, manifest)?;

    Ok(execute_request)
}

fn validate_rpc_execute_request(
    request: &GeneratedRpcExecuteRequest,
) -> AndromedaResult<ProcedureGatewayExecuteRequest> {
    if request.procedure_name.trim().is_empty() {
        return Err(contract_error(
            "RpcExecuteRequest procedure_name must be non-empty",
        ));
    }

    let expected_contract_hash = ContractHash::from_slice(&request.expected_contract_hash)?;
    if expected_contract_hash.is_zero() {
        return Err(contract_error(
            "RpcExecuteRequest expected_contract_hash must not be zero",
        ));
    }

    if request.expected_catalog_version == 0 {
        return Err(contract_error(
            "RpcExecuteRequest expected_catalog_version must be nonzero",
        ));
    }

    let Some(expected_stats_version) = request.expected_stats_version else {
        return Err(contract_error(
            "RpcExecuteRequest expected_stats_version must be present",
        ));
    };
    if expected_stats_version == 0 {
        return Err(contract_error(
            "RpcExecuteRequest expected_stats_version must be nonzero",
        ));
    }

    if request.surface_scope != APPLICATION_SURFACE_SCOPE {
        return Err(security_error(
            "RpcExecuteRequest surface_scope does not match Application surface",
        ));
    }

    Ok(ProcedureGatewayExecuteRequest {
        procedure_name: request.procedure_name.clone(),
        expected_contract_hash,
        expected_catalog_version: CatalogVersion::new(request.expected_catalog_version),
        expected_stats_version,
        surface_scope: request.surface_scope.clone(),
        argument_count: request.arguments.len(),
    })
}

fn validate_request_matches_manifest(
    envelope: &andromeda_rpc_protocol::FrameEnvelope,
    request: &ProcedureGatewayExecuteRequest,
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<()> {
    validate_envelope_matches_request(envelope, request)?;
    validate_request_matches_catalog_identity(request, manifest)?;
    validate_request_matches_manifest_version(request, manifest)
}

fn validate_envelope_matches_request(
    envelope: &andromeda_rpc_protocol::FrameEnvelope,
    request: &ProcedureGatewayExecuteRequest,
) -> AndromedaResult<()> {
    if envelope.contract_hash != request.expected_contract_hash {
        return Err(contract_error(
            "procedure invocation envelope ContractHash does not match request expectation",
        ));
    }

    if envelope.catalog_version != request.expected_catalog_version {
        return Err(contract_error(
            "procedure invocation envelope CatalogVersion does not match request expectation",
        ));
    }

    Ok(())
}

fn validate_request_matches_catalog_identity(
    request: &ProcedureGatewayExecuteRequest,
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<()> {
    if request.procedure_name != manifest.procedure_name {
        return Err(contract_error(
            "procedure invocation Procedure name does not match resolved manifest",
        ));
    }

    Ok(())
}

fn validate_request_matches_manifest_version(
    request: &ProcedureGatewayExecuteRequest,
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<()> {
    if request.expected_contract_hash != manifest.contract_hash {
        return Err(contract_error(
            "procedure invocation ContractHash does not match resolved manifest",
        ));
    }

    if request.expected_catalog_version != manifest.catalog_version {
        return Err(contract_error(
            "procedure invocation CatalogVersion does not match resolved manifest",
        ));
    }

    if request.expected_stats_version != manifest.stats_version {
        return Err(contract_error(
            "procedure invocation StatsVersion does not match resolved manifest",
        ));
    }

    Ok(())
}

fn protocol_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Protocol, message)
}

fn contract_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Contract, message)
}

fn security_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}

const APPLICATION_SURFACE_SCOPE: &str = "application";
