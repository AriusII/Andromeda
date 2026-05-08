use andromeda_core::{
    AndromedaResult, CatalogVersion, ContractHash, Permission, SurfaceScope as CoreSurfaceScope,
};
use andromeda_proto::{
    FrameEnvelope as ProtoFrameEnvelope, PayloadKind, decode_generated_message, generated,
    validate_generated_rpc_execute_request,
};
use andromeda_security_contract::{FAMILY_ID_APPLICATION, PERMISSION_ID_EXECUTE_PROCEDURE};

use super::errors::{contract_error, protocol_error, security_error};
use super::route::ProcedureRouteExecuteRequest;
use crate::{CatalogProcedureManifest, FrameBytes, SurfacePlane, decode_typed_frame_envelope};

type GeneratedRpcExecuteRequest = generated::protocol::v1::RpcExecuteRequest;

pub(super) fn decode_and_validate_rpc_execute_request(
    frame: &FrameBytes,
    plane: SurfacePlane,
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<ProcedureRouteExecuteRequest> {
    validate_route_manifest_permissions(manifest)?;
    manifest.to_protobuf()?;

    let envelope = decode_rpc_execute_envelope(frame)?;
    if envelope.tx_id.is_some() {
        return Err(protocol_error(
            "procedure invocation envelope must not carry client transaction id",
        ));
    }

    let generated_request: GeneratedRpcExecuteRequest =
        decode_generated_message(envelope.payload.as_slice())?;
    validate_generated_rpc_execute_request(&generated_request)?;
    let execute_request = validate_rpc_execute_request(&generated_request, plane)?;

    validate_request_matches_manifest(&envelope, &execute_request, manifest)?;

    Ok(execute_request)
}

pub(super) fn core_surface_scope_for_plane(plane: SurfacePlane) -> CoreSurfaceScope {
    match plane {
        SurfacePlane::Application => CoreSurfaceScope::Application,
        SurfacePlane::Administration => CoreSurfaceScope::Administration,
        SurfacePlane::HighAvailability => CoreSurfaceScope::Cluster,
        SurfacePlane::Monitoring => CoreSurfaceScope::MonitoringAgent,
    }
}

pub(super) fn required_execute_permission(
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<Permission> {
    let declares_execute_permission = manifest.required_permissions.iter().any(|permission| {
        permission.family == FAMILY_ID_APPLICATION
            && permission.id == PERMISSION_ID_EXECUTE_PROCEDURE
    });

    if !declares_execute_permission {
        return Err(contract_error(
            "procedure manifest required_permissions must include andromeda.execute_procedure",
        ));
    }

    Ok(Permission::ExecuteProcedure(manifest.procedure_id))
}

fn decode_rpc_execute_envelope(frame: &FrameBytes) -> AndromedaResult<ProtoFrameEnvelope> {
    let envelope = decode_typed_frame_envelope(frame)?;

    if envelope.payload_kind != PayloadKind::RpcExecuteRequest {
        return Err(protocol_error(
            "procedure invocation envelope payload kind must be RpcExecuteRequest",
        ));
    }

    Ok(envelope)
}

fn validate_rpc_execute_request(
    request: &GeneratedRpcExecuteRequest,
    plane: SurfacePlane,
) -> AndromedaResult<ProcedureRouteExecuteRequest> {
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

    let expected_surface = surface_scope_wire_label(plane);
    if request.surface_scope != expected_surface {
        return Err(security_error(
            "RpcExecuteRequest surface_scope does not match Application surface",
        ));
    }

    Ok(ProcedureRouteExecuteRequest {
        procedure_name: request.procedure_name.clone(),
        expected_contract_hash,
        expected_catalog_version: CatalogVersion::new(request.expected_catalog_version),
        expected_stats_version,
        surface_scope: request.surface_scope.clone(),
        argument_count: request.arguments.len(),
    })
}

fn validate_request_matches_manifest(
    envelope: &ProtoFrameEnvelope,
    request: &ProcedureRouteExecuteRequest,
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<()> {
    validate_envelope_matches_request(envelope, request)?;
    validate_request_matches_catalog_identity(request, manifest)?;
    validate_request_matches_manifest_version(request, manifest)
}

fn validate_envelope_matches_request(
    envelope: &ProtoFrameEnvelope,
    request: &ProcedureRouteExecuteRequest,
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
    request: &ProcedureRouteExecuteRequest,
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
    request: &ProcedureRouteExecuteRequest,
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

fn validate_route_manifest_permissions(manifest: &CatalogProcedureManifest) -> AndromedaResult<()> {
    required_execute_permission(manifest).map(|_| ())
}

const fn surface_scope_wire_label(plane: SurfacePlane) -> &'static str {
    match plane {
        SurfacePlane::Application => "application",
        SurfacePlane::Administration => "administration",
        SurfacePlane::HighAvailability => "cluster",
        SurfacePlane::Monitoring => "monitoring",
    }
}
