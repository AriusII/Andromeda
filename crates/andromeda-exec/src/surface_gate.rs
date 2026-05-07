//! V0 dispatch-time security gate.
//!
//! Sits between the QUIC transport surface ([`SurfacePlane`]) and the
//! [`LocalExecutor`](crate::LocalExecutor) admission path. Maps the wire
//! plane to an [`SurfaceScope`] and runs the
//! [`SurfaceAuthorizer`] over the presented certificate fingerprint and the
//! requested [`SurfaceAction`] before any frame is dispatched into execution.
//!
//! ## Why it lives in `andromeda-exec`
//!
//! `andromeda-exec` is the only crate that already depends on both
//! `andromeda-quic` (for [`SurfacePlane`]) and `andromeda-observe` (for the
//! identity / permission / audit primitives), so the adapter can sit here
//! without introducing a new edge in the dependency graph.
//!
//! ## Doctrine alignment
//!
//! - Procedure-only execution: [`SurfaceAction`] is the only verb set
//!   admitted; raw / ad hoc SQL is unrepresentable.
//! - Critical security decisions are observable: every call returns an
//!   [`AuthorizationOutcome`] carrying a [`SecurityAuditTrace`].
//! - No secrets are logged: presented fingerprints are sanitized through the
//!   observe layer before they appear in audit envelopes.
//! - `forbid(unsafe_code)` is inherited from the crate root.

use andromeda_observe::{
    AuthorizationDenialReason, AuthorizationOutcome, PrincipalRegistry, SecurityAuditOutcome,
    SecurityAuditTrace, SurfaceAction, SurfaceAuthorizer, SurfaceScope, TraceId,
};
use andromeda_quic::SurfacePlane;

/// V0 mapping from a transport [`SurfacePlane`] to a security
/// [`SurfaceScope`].
///
/// The transport plane controls *how* a session is wired (which listener
/// admitted it); the security scope controls *what* the session may do. The
/// mapping is intentionally total and deterministic — there is one scope per
/// plane in V0.
pub const fn surface_plane_to_scope(plane: SurfacePlane) -> SurfaceScope {
    match plane {
        SurfacePlane::Application => SurfaceScope::Application,
        SurfacePlane::Administration => SurfaceScope::Administration,
        SurfacePlane::HighAvailability => SurfaceScope::Cluster,
        SurfacePlane::Monitoring => SurfaceScope::MonitoringAgent,
    }
}

/// Dispatch-time security gate.
///
/// Wraps a [`SurfaceAuthorizer`] and translates the transport plane into the
/// scope expected by the security model.
#[derive(Debug, Clone)]
pub struct SurfacePlaneAuthorizer<'a> {
    registry: &'a PrincipalRegistry,
    inner: SurfaceAuthorizer<'a>,
}

impl<'a> SurfacePlaneAuthorizer<'a> {
    /// Build a new gate over the supplied principal registry.
    pub fn new(registry: &'a PrincipalRegistry) -> Self {
        Self {
            registry,
            inner: SurfaceAuthorizer::new(registry),
        }
    }

    /// Authorize a request before frame dispatch.
    ///
    /// Returns the same [`AuthorizationOutcome`] surface as
    /// [`SurfaceAuthorizer::authorize`], so callers can branch on
    /// allow/deny while always handing the embedded [`SecurityAuditTrace`]
    /// to the observability sink.
    pub fn authorize_dispatch(
        &self,
        trace_id: TraceId,
        plane: SurfacePlane,
        presented_fingerprint: &str,
        action: SurfaceAction,
    ) -> andromeda_core::AndromedaResult<AuthorizationOutcome> {
        let scope = surface_plane_to_scope(plane);
        if matches!(action, SurfaceAction::ExecuteProcedure)
            && plane != SurfacePlane::Application
            && self.registry.lookup(presented_fingerprint).is_some()
        {
            return self.deny_non_application_procedure_dispatch(
                trace_id,
                scope,
                presented_fingerprint,
            );
        }

        let outcome = self
            .inner
            .authorize(trace_id, scope, presented_fingerprint, action)?;

        Ok(outcome)
    }

    /// Authorize the procedure invocation dispatch path and return a
    /// capability token that can be handed to the local execution runtime.
    ///
    /// The token is only constructible after an allowed `ExecuteProcedure`
    /// decision on the Application plane. Denials are returned as the typed
    /// [`AuthorizationOutcome`] so callers can emit the embedded audit trace
    /// and stop before transaction creation.
    pub fn authorize_procedure_dispatch(
        &self,
        trace_id: TraceId,
        plane: SurfacePlane,
        presented_fingerprint: &str,
    ) -> andromeda_core::AndromedaResult<Result<AuthorizedProcedureDispatch, AuthorizationOutcome>>
    {
        let outcome = self.authorize_dispatch(
            trace_id,
            plane,
            presented_fingerprint,
            SurfaceAction::ExecuteProcedure,
        )?;

        match outcome {
            AuthorizationOutcome::Allowed { ref audit, .. } => {
                Ok(Ok(AuthorizedProcedureDispatch {
                    plane,
                    audit: audit.clone(),
                }))
            }
            denied @ AuthorizationOutcome::Denied { .. } => Ok(Err(denied)),
        }
    }

    fn deny_non_application_procedure_dispatch(
        &self,
        trace_id: TraceId,
        scope: SurfaceScope,
        presented_fingerprint: &str,
    ) -> andromeda_core::AndromedaResult<AuthorizationOutcome> {
        let binding = self.registry.lookup(presented_fingerprint).ok_or_else(|| {
            andromeda_core::AndromedaError::new(
                andromeda_core::AndromedaErrorKind::Security,
                "procedure dispatch surface denial lost principal binding evidence",
            )
        })?;
        let permission = SurfaceAction::ExecuteProcedure.required_permission();
        let audit = SecurityAuditTrace::new(
            trace_id,
            scope,
            binding.certificate().clone(),
            binding.principal().clone(),
            permission,
            SecurityAuditOutcome::Denied,
            format!(
                "denied:{}:surface={:?}:permission={:?}:action={}:procedure_dispatch_requires_application_surface",
                AuthorizationDenialReason::SurfaceDoesNotPermitPermission.label(),
                scope,
                permission,
                SurfaceAction::ExecuteProcedure.evidence_label(),
            ),
        )?;

        Ok(AuthorizationOutcome::Denied {
            reason: AuthorizationDenialReason::SurfaceDoesNotPermitPermission,
            audit,
        })
    }
}

/// Capability token proving that a procedure dispatch was authorized by the
/// exec-owned surface gate before local transaction creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedProcedureDispatch {
    plane: SurfacePlane,
    audit: SecurityAuditTrace,
}

impl AuthorizedProcedureDispatch {
    /// Transport plane on which the procedure dispatch was authorized.
    pub const fn plane(&self) -> SurfacePlane {
        self.plane
    }

    /// Typed security audit evidence for the allow decision.
    pub const fn audit(&self) -> &SecurityAuditTrace {
        &self.audit
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

        // Application certificate presented on Administration plane → scope mismatch.
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

        // Admin certificate on Administration plane with the right grant → allowed.
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
    fn monitoring_plane_rejects_non_diagnostics_permission() {
        let reg = registry(vec![binding(
            "fp-mon",
            SurfaceScope::MonitoringAgent,
            "obs-1",
            vec![Permission::ExecuteProcedure],
        )]);
        let gate = SurfacePlaneAuthorizer::new(&reg);

        let outcome = gate
            .authorize_dispatch(
                TraceId::new(1),
                SurfacePlane::Monitoring,
                "fp-mon",
                SurfaceAction::ExecuteProcedure,
            )
            .unwrap();

        assert!(outcome.is_denied());
        if let AuthorizationOutcome::Denied { reason, audit } = outcome {
            assert_eq!(
                reason,
                AuthorizationDenialReason::SurfaceDoesNotPermitPermission
            );
            assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
            assert!(audit.has_reason());
        }
    }

    #[test]
    fn high_availability_plane_maps_to_cluster_scope() {
        let reg = registry(vec![binding(
            "fp-cluster",
            SurfaceScope::Cluster,
            "ha-1",
            vec![Permission::ClusterPromote],
        )]);
        let gate = SurfacePlaneAuthorizer::new(&reg);

        let outcome = gate
            .authorize_dispatch(
                TraceId::new(7),
                SurfacePlane::HighAvailability,
                "fp-cluster",
                SurfaceAction::Admin(AdminOperation::ClusterPromote),
            )
            .unwrap();
        assert!(outcome.is_allowed());
    }

    #[test]
    fn unknown_certificate_at_dispatch_emits_deny_audit() {
        let reg = registry(vec![]);
        let gate = SurfacePlaneAuthorizer::new(&reg);

        let outcome = gate
            .authorize_dispatch(
                TraceId::new(9),
                SurfacePlane::Application,
                "fp-not-bound",
                SurfaceAction::ExecuteProcedure,
            )
            .unwrap();

        assert!(outcome.is_denied());
        if let AuthorizationOutcome::Denied { reason, audit } = outcome {
            assert_eq!(reason, AuthorizationDenialReason::UnknownCertificate);
            assert!(audit.has_identity_evidence());
            assert!(!audit.contains_sensitive_evidence());
        }
    }

    #[test]
    fn procedure_dispatch_token_requires_application_surface() {
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
    fn procedure_dispatch_token_requires_execute_permission() {
        let reg = registry(vec![binding(
            "fp-readonly-proc",
            SurfaceScope::Application,
            "svc-readonly",
            vec![Permission::ReadContract],
        )]);
        let gate = SurfacePlaneAuthorizer::new(&reg);

        let denied = gate
            .authorize_procedure_dispatch(
                TraceId::new(12),
                SurfacePlane::Application,
                "fp-readonly-proc",
            )
            .unwrap()
            .unwrap_err();

        assert!(denied.is_denied());
        if let AuthorizationOutcome::Denied { reason, audit } = denied {
            assert_eq!(
                reason,
                AuthorizationDenialReason::PrincipalMissingPermission
            );
            assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
            assert!(audit.reason.contains("principal_missing_permission"));
            assert!(audit.reason.contains("ExecuteProcedure"));
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
    fn procedure_dispatch_token_carries_allow_audit() {
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
