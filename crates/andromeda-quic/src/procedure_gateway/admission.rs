use andromeda_core::{
    PrincipalAuthorizationEvidence, PrincipalId, PrincipalRegistry, SurfaceScope,
};
use andromeda_rpc_codec::required_execute_permission;

use crate::CatalogProcedureManifest;

use super::errors::{ProcedureRouteAdmissionError, security_error};
use super::route::ProcedureRouteBinding;

/// Authorized pre-dispatch route evidence for a Procedure invocation.
///
/// The authorization evidence is audit-ready and comes from the core IAM
/// decision path. It is attached before any executor dispatch or transaction
/// publication can occur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureAuthorizedRouteBinding {
    pub route: ProcedureRouteBinding,
    pub principal_id: PrincipalId,
    pub authorization_evidence: PrincipalAuthorizationEvidence,
}

pub(super) fn authorize_application_route(
    route: ProcedureRouteBinding,
    manifest: &CatalogProcedureManifest,
    principal_registry: &PrincipalRegistry,
) -> Result<ProcedureAuthorizedRouteBinding, ProcedureRouteAdmissionError> {
    let required_permission =
        required_execute_permission(manifest).map_err(ProcedureRouteAdmissionError::route)?;
    let authorization = principal_registry.authorize(
        core_surface_scope_for_plane(route.surface_plane),
        route.certificate_identity.fingerprint().as_str(),
        &required_permission,
    );

    if authorization.is_denied() {
        let reason = authorization
            .denial_reason
            .map(|reason| reason.as_str())
            .unwrap_or("denied");
        return Err(ProcedureRouteAdmissionError::authorization(
            security_error(format!(
                "procedure invocation authorization denied: {reason}"
            )),
            authorization,
        ));
    }

    let Some(principal_id) = authorization.principal_id else {
        return Err(ProcedureRouteAdmissionError::authorization(
            security_error("procedure invocation authorization missing principal evidence"),
            authorization,
        ));
    };

    Ok(ProcedureAuthorizedRouteBinding {
        route,
        principal_id,
        authorization_evidence: authorization.evidence,
    })
}

fn core_surface_scope_for_plane(plane: crate::SurfacePlane) -> SurfaceScope {
    crate::mtls_identity::plane_to_required_surface_scope(plane)
}
