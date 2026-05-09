use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_principal::Permission as CorePermission;
use andromeda_proto::{
    PayloadKind, decode_generated_message, generated, validate_generated_rpc_execute_request,
};
use andromeda_security_contract::{
    FAMILY_ID_APPLICATION, PERMISSION_ID_EXECUTE_PROCEDURE, Permission as SecurityPermission,
    PermissionFamily,
};
use andromeda_types::{CatalogVersion, ContractHash};

use crate::catalog_manifest_resolution::validate_catalog_procedure_manifest_projection;
use crate::{CatalogProcedureManifest, CatalogRequiredPermission, decode_typed_frame_envelope};
use andromeda_rpc_protocol::FrameBytes;

type GeneratedRpcExecuteRequest = generated::protocol::v1::RpcExecuteRequest;

/// Domain projection of the protobuf `RpcExecuteRequest` admitted by a route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureRouteExecuteRequest {
    pub procedure_name: String,
    pub expected_contract_hash: ContractHash,
    pub expected_catalog_version: CatalogVersion,
    pub expected_stats_version: u64,
    pub surface_scope: String,
    pub argument_count: usize,
}

/// Decodes and validates an Application Procedure execute request against a manifest.
pub fn decode_and_validate_rpc_execute_request(
    frame: &FrameBytes,
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<ProcedureRouteExecuteRequest> {
    validate_route_manifest_permissions(manifest)?;
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
        decode_generated_message(envelope.payload.as_slice())?;
    validate_generated_rpc_execute_request(&generated_request)?;
    let execute_request = validate_rpc_execute_request(&generated_request)?;

    validate_request_matches_manifest(&envelope, &execute_request, manifest)?;

    Ok(execute_request)
}

/// Returns the IAM permission required to execute the resolved Procedure.
pub fn required_execute_permission(
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<CorePermission> {
    validate_route_manifest_permissions(manifest)?;
    Ok(CorePermission::ExecuteProcedure(manifest.procedure_id))
}

fn validate_rpc_execute_request(
    request: &GeneratedRpcExecuteRequest,
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

    if request.surface_scope != APPLICATION_SURFACE_SCOPE {
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
    envelope: &andromeda_rpc_protocol::FrameEnvelope,
    request: &ProcedureRouteExecuteRequest,
    manifest: &CatalogProcedureManifest,
) -> AndromedaResult<()> {
    validate_envelope_matches_request(envelope, request)?;
    validate_request_matches_catalog_identity(request, manifest)?;
    validate_request_matches_manifest_version(request, manifest)
}

fn validate_envelope_matches_request(
    envelope: &andromeda_rpc_protocol::FrameEnvelope,
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
    let mut declares_execute_permission = false;

    for permission in &manifest.required_permissions {
        if permission.family == FAMILY_ID_APPLICATION
            && permission.id == PERMISSION_ID_EXECUTE_PROCEDURE
        {
            declares_execute_permission = true;
            continue;
        }

        validate_manifest_permission_is_application_only(permission)?;
    }

    if !declares_execute_permission {
        return Err(contract_error(
            "procedure manifest required_permissions must include andromeda.execute_procedure",
        ));
    }

    Ok(())
}

fn validate_manifest_permission_is_application_only(
    permission: &CatalogRequiredPermission,
) -> AndromedaResult<()> {
    let Some(security_permission) = SecurityPermission::from_canonical_id(&permission.id) else {
        if permission.family == FAMILY_ID_APPLICATION {
            return Ok(());
        }

        return Err(contract_error(format!(
            "Application Procedure manifest required_permissions cannot include non-Application permission family {:?} for {:?}",
            permission.family, permission.id
        )));
    };

    let expected_family = security_permission.family();
    if permission.family != expected_family.as_str() {
        return Err(contract_error(format!(
            "procedure manifest required_permissions permission {:?} must use canonical family {:?}",
            permission.id,
            expected_family.as_str()
        )));
    }

    if !matches!(expected_family, PermissionFamily::Application) {
        return Err(contract_error(format!(
            "Application Procedure manifest required_permissions cannot include non-Application permission {:?} from family {:?}",
            permission.id, permission.family
        )));
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
