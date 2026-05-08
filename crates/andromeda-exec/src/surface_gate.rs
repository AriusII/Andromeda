//! QUIC surface adapter for dispatch-time security authorization.
//!
//! The core authorization rules live in `andromeda-security`. This module
//! keeps the QUIC-specific [`SurfacePlane`] mapping local to `andromeda-exec`
//! and preserves the execution-facing capability token type.

use andromeda_observe::{
    AuthorizationOutcome, PrincipalRegistry, SurfaceAction, SurfaceScope, TraceId,
};
use andromeda_quic::SurfacePlane;
use andromeda_security::SurfacePlaneAuthorizer as SecuritySurfacePlaneAuthorizer;

/// Procedure dispatch capability specialized to the QUIC transport plane.
pub type AuthorizedProcedureDispatch =
    andromeda_security::AuthorizedProcedureDispatch<SurfacePlane>;

/// V0 mapping from a transport [`SurfacePlane`] to a security
/// [`SurfaceScope`].
///
/// The transport plane controls how a session is wired; the security scope
/// controls what the session may do. The mapping is intentionally total and
/// deterministic.
pub const fn surface_plane_to_scope(plane: SurfacePlane) -> SurfaceScope {
    match plane {
        SurfacePlane::Application => SurfaceScope::Application,
        SurfacePlane::Administration => SurfaceScope::Administration,
        SurfacePlane::HighAvailability => SurfaceScope::Cluster,
        SurfacePlane::Monitoring => SurfaceScope::MonitoringAgent,
    }
}

/// QUIC-plane dispatch-time security gate.
///
/// This adapter translates the QUIC [`SurfacePlane`] into the
/// transport-independent [`SurfaceScope`] consumed by `andromeda-security`.
#[derive(Debug, Clone)]
pub struct SurfacePlaneAuthorizer<'a> {
    inner: SecuritySurfacePlaneAuthorizer<'a>,
}

impl<'a> SurfacePlaneAuthorizer<'a> {
    /// Build a new gate over the supplied principal registry.
    pub fn new(registry: &'a PrincipalRegistry) -> Self {
        Self {
            inner: SecuritySurfacePlaneAuthorizer::new(registry),
        }
    }

    /// Authorize a request before frame dispatch.
    pub fn authorize_dispatch(
        &self,
        trace_id: TraceId,
        plane: SurfacePlane,
        presented_fingerprint: &str,
        action: SurfaceAction,
    ) -> andromeda_core::AndromedaResult<AuthorizationOutcome> {
        self.inner.authorize_dispatch(
            trace_id,
            surface_plane_to_scope(plane),
            presented_fingerprint,
            action,
        )
    }

    /// Authorize procedure dispatch and return the QUIC-specialized
    /// capability token.
    pub fn authorize_procedure_dispatch(
        &self,
        trace_id: TraceId,
        plane: SurfacePlane,
        presented_fingerprint: &str,
    ) -> andromeda_core::AndromedaResult<Result<AuthorizedProcedureDispatch, AuthorizationOutcome>>
    {
        self.inner.authorize_procedure_dispatch(
            trace_id,
            plane,
            surface_plane_to_scope(plane),
            presented_fingerprint,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_observe::{
        AdminOperation, AuthorizationDenialReason, CertificateIdentity, Permission,
        PrincipalBinding, SecurityAuditOutcome, UserPrincipal, UserPrincipalKind,
    };

    fn registry(bindings: Vec<PrincipalBinding>) -> PrincipalRegistry {
        let mut r = PrincipalRegistry::new();
        for b in bindings {
            r.register(b).unwrap();
        }
        r
    }

    fn binding(
        fp: &str,
        scope: SurfaceScope,
        principal_id: &str,
        permissions: Vec<Permission>,
    ) -> PrincipalBinding {
        let cert = CertificateIdentity::new(fp, format!("CN={fp}"), scope).unwrap();
        let principal = UserPrincipal::new(principal_id, UserPrincipalKind::Service).unwrap();
        PrincipalBinding::new(cert, principal, permissions).unwrap()
    }

    #[test]
    fn plane_scope_mapping_is_total() {
        assert_eq!(
            surface_plane_to_scope(SurfacePlane::Application),
            SurfaceScope::Application
        );
        assert_eq!(
            surface_plane_to_scope(SurfacePlane::Administration),
            SurfaceScope::Administration
        );
        assert_eq!(
            surface_plane_to_scope(SurfacePlane::HighAvailability),
            SurfaceScope::Cluster
        );
        assert_eq!(
            surface_plane_to_scope(SurfacePlane::Monitoring),
            SurfaceScope::MonitoringAgent
        );
    }

    #[test]
    fn application_plane_admits_procedure_for_bound_principal() {
        let reg = registry(vec![binding(
            "fp-app",
            SurfaceScope::Application,
            "svc-app",
            vec![Permission::ExecuteProcedure],
        )]);
        let gate = SurfacePlaneAuthorizer::new(&reg);

        let outcome = gate
            .authorize_dispatch(
                TraceId::new(1),
                SurfacePlane::Application,
                "fp-app",
                SurfaceAction::ExecuteProcedure,
            )
            .unwrap();

        assert!(outcome.is_allowed());
        assert_eq!(outcome.audit().outcome, SecurityAuditOutcome::Allowed);
    }

    #[test]
    fn administration_plane_admits_admin_op_only_for_admin_certificate() {
        let reg = registry(vec![
            binding(
                "fp-app",
                SurfaceScope::Application,
                "svc-app",
                vec![Permission::ManageSecurity],
            ),
            binding(
                "fp-adm",
                SurfaceScope::Administration,
                "ops-1",
                vec![Permission::ManageSecurity],
            ),
        ]);
        let gate = SurfacePlaneAuthorizer::new(&reg);

        let denied = gate
            .authorize_dispatch(
                TraceId::new(1),
                SurfacePlane::Administration,
                "fp-app",
                SurfaceAction::Admin(AdminOperation::ManageSecurity),
            )
            .unwrap();
        assert!(denied.is_denied());
        if let AuthorizationOutcome::Denied { reason, .. } = denied {
            assert_eq!(reason, AuthorizationDenialReason::SurfaceScopeMismatch);
        }

        let allowed = gate
            .authorize_dispatch(
                TraceId::new(2),
                SurfacePlane::Administration,
                "fp-adm",
                SurfaceAction::Admin(AdminOperation::ManageSecurity),
            )
            .unwrap();
        assert!(allowed.is_allowed());
    }

    #[test]
    fn procedure_dispatch_token_requires_application_plane() {
        let reg = registry(vec![binding(
            "fp-adm-proc",
            SurfaceScope::Administration,
            "ops-proc",
            vec![Permission::ExecuteProcedure],
        )]);
        let gate = SurfacePlaneAuthorizer::new(&reg);

        let denied = gate
            .authorize_procedure_dispatch(
                TraceId::new(10),
                SurfacePlane::Administration,
                "fp-adm-proc",
            )
            .unwrap()
            .unwrap_err();

        assert!(denied.is_denied());
        if let AuthorizationOutcome::Denied { reason, audit } = denied {
            assert_eq!(
                reason,
                AuthorizationDenialReason::SurfaceDoesNotPermitPermission
            );
            assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
            assert!(audit.reason.contains("requires_application_surface"));
        }
    }

    #[test]
    fn high_availability_procedure_dispatch_is_denied_before_token_issue() {
        let reg = registry(vec![binding(
            "fp-ha-proc",
            SurfaceScope::Cluster,
            "ha-proc",
            vec![Permission::ExecuteProcedure],
        )]);
        let gate = SurfacePlaneAuthorizer::new(&reg);

        let denied = gate
            .authorize_procedure_dispatch(
                TraceId::new(13),
                SurfacePlane::HighAvailability,
                "fp-ha-proc",
            )
            .unwrap()
            .unwrap_err();

        assert!(denied.is_denied());
        if let AuthorizationOutcome::Denied { reason, audit } = denied {
            assert_eq!(
                reason,
                AuthorizationDenialReason::SurfaceDoesNotPermitPermission
            );
            assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
            assert!(audit.reason.contains("surface_does_not_permit_permission"));
            assert!(audit.reason.contains("requires_application_surface"));
        }
    }

    #[test]
    fn procedure_dispatch_token_carries_quic_plane_and_allow_audit() {
        let reg = registry(vec![binding(
            "fp-app-proc",
            SurfaceScope::Application,
            "svc-proc",
            vec![Permission::ExecuteProcedure],
        )]);
        let gate = SurfacePlaneAuthorizer::new(&reg);

        let token = gate
            .authorize_procedure_dispatch(
                TraceId::new(11),
                SurfacePlane::Application,
                "fp-app-proc",
            )
            .unwrap()
            .unwrap();

        assert_eq!(token.plane(), SurfacePlane::Application);
        assert_eq!(token.audit().outcome, SecurityAuditOutcome::Allowed);
    }
}
