use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_security_contract::{
    FAMILY_ID_APPLICATION, PERMISSION_ID_EXECUTE_PROCEDURE, Permission as SecurityPermission,
    PermissionFamily, PrincipalPermission,
};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

/// Canonical Procedure manifest projection used at RPC gateway boundaries.
///
/// The type is owned by the Procedure contract crate so transport crates can
/// validate resolved Procedure identity without becoming the source of truth
/// for catalog/procedure manifest shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGatewayManifest {
    pub procedure_id: ProcedureId,
    pub procedure_name: String,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub protocol_layout: ProcedureGatewayProtocolLayout,
    pub result_streams: Vec<ProcedureGatewayResultStreamDescriptor>,
    pub stats_version: u64,
    pub policy_version: ContractHash,
    pub required_permissions: Vec<ProcedureGatewayRequiredPermission>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGatewayProtocolLayout {
    pub descriptor_set_hash: ContractHash,
    pub frame_envelope_hash: ContractHash,
    pub protocol_package: String,
    pub contract_package: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGatewayRequiredPermission {
    pub id: String,
    pub family: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGatewayResultStreamDescriptor {
    pub stream_name: String,
    pub columns: Vec<ProcedureGatewayColumnDescriptor>,
    pub cardinality: i32,
    pub row_count_requirement: i32,
    pub row_count_exact: Option<u64>,
    pub row_count_max: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGatewayColumnDescriptor {
    pub name: String,
    pub ordinal: u32,
    pub type_name: String,
}

/// Domain projection of an RPC execute request admitted against a manifest.
///
/// The protobuf decoder lives in the RPC codec crate. The contract meaning of
/// the projected request lives here beside the manifest shape it must match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureGatewayExecuteRequest {
    pub procedure_name: String,
    pub expected_contract_hash: ContractHash,
    pub expected_catalog_version: CatalogVersion,
    pub expected_stats_version: u64,
    pub surface_scope: String,
    pub argument_count: usize,
}

/// Returns the principal permission required to execute a resolved Procedure.
pub fn required_execute_permission(
    manifest: &ProcedureGatewayManifest,
) -> AndromedaResult<PrincipalPermission> {
    validate_procedure_gateway_manifest_permissions(manifest)?;
    Ok(PrincipalPermission::ExecuteProcedure(manifest.procedure_id))
}

/// Validates that a gateway manifest only declares Application-surface
/// permissions and includes the execute permission required by the route.
pub fn validate_procedure_gateway_manifest_permissions(
    manifest: &ProcedureGatewayManifest,
) -> AndromedaResult<()> {
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
    permission: &ProcedureGatewayRequiredPermission,
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

fn contract_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Contract, message)
}
