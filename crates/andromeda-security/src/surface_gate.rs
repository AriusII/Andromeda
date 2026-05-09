//! Dispatch-time surface authorization.
//!
//! This module owns the transport-independent authorization rule that
//! procedure dispatch is only valid on the Application surface. Transport
//! crates adapt their listener-specific plane type into an observed
//! [`SurfaceScope`] before calling this gate.

use andromeda_audit::{SecurityAuditOutcome, SecurityAuditTrace, SurfaceScope};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observability::TraceId;

use crate::{
    AuthorizationDenialReason, AuthorizationOutcome, PrincipalRegistry, SurfaceAction,
    SurfaceAuthorizer, principal_binding::denied_security_outcome,
};

/// Dispatch-time security gate over observed surface scopes.
///
/// Wraps [`SurfaceAuthorizer`] and adds the Procedure dispatch invariant that
/// only the Application surface can mint an [`AuthorizedProcedureDispatch`]
/// capability.
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
    /// [`SurfaceAuthorizer::authorize`], so callers can branch on allow/deny
    /// while always handing the embedded [`SecurityAuditTrace`] to the
    /// observability sink.
    pub fn authorize_dispatch(
        &self,
        trace_id: TraceId,
        requested_scope: SurfaceScope,
        presented_fingerprint: &str,
        action: SurfaceAction,
    ) -> AndromedaResult<AuthorizationOutcome> {
        if matches!(action, SurfaceAction::ExecuteProcedure)
            && requested_scope != SurfaceScope::Application
            && self.registry.lookup(presented_fingerprint).is_some()
        {
            return self.deny_non_application_procedure_dispatch(
                trace_id,
                requested_scope,
                presented_fingerprint,
            );
        }

        self.inner
            .authorize(trace_id, requested_scope, presented_fingerprint, action)
    }

    /// Authorize the procedure invocation dispatch path and return a
    /// capability token that can be handed to the local execution runtime.
    ///
    /// The token is only constructible after an allowed `ExecuteProcedure`
    /// decision on the Application surface. Denials are returned as the typed
    /// [`AuthorizationOutcome`] so callers can emit the embedded audit trace
    /// and stop before transaction creation.
    pub fn authorize_procedure_dispatch<P>(
        &self,
        trace_id: TraceId,
        plane: P,
        requested_scope: SurfaceScope,
        presented_fingerprint: &str,
    ) -> AndromedaResult<Result<AuthorizedProcedureDispatch<P>, AuthorizationOutcome>> {
        let outcome = self.authorize_dispatch(
            trace_id,
            requested_scope,
            presented_fingerprint,
            SurfaceAction::ExecuteProcedure,
        )?;

        match outcome {
            AuthorizationOutcome::Allowed { ref audit, .. } => {
                Ok(Ok(AuthorizedProcedureDispatch {
                    plane,
                    audit: audit.clone(),
                }))
            },
            denied @ AuthorizationOutcome::Denied { .. } => Ok(Err(denied)),
        }
    }

    fn deny_non_application_procedure_dispatch(
        &self,
        trace_id: TraceId,
        requested_scope: SurfaceScope,
        presented_fingerprint: &str,
    ) -> AndromedaResult<AuthorizationOutcome> {
        let binding = self.registry.lookup(presented_fingerprint).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Security,
                "procedure dispatch surface denial lost principal binding evidence",
            )
        })?;
        let permission = SurfaceAction::ExecuteProcedure.required_permission();
        denied_security_outcome(
            trace_id,
            requested_scope,
            binding.certificate().clone(),
            binding.principal().clone(),
            permission,
            AuthorizationDenialReason::SurfaceDoesNotPermitPermission,
            format!(
                "surface={:?}:permission={:?}:action={}:procedure_dispatch_requires_application_surface",
                requested_scope,
                permission,
                SurfaceAction::ExecuteProcedure.evidence_label(),
            ),
        )
    }
}

/// Capability token proving that a procedure dispatch was authorized before
/// local transaction creation.
///
/// `P` is the caller-owned transport plane type. Security decisions are made
/// from [`SurfaceScope`], but retaining the original plane lets adapters keep
/// their public API without making this crate depend on a transport crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizedProcedureDispatch<P> {
    plane: P,
    audit: SecurityAuditTrace,
}

impl<P> AuthorizedProcedureDispatch<P> {
    /// Transport plane on which the procedure dispatch was authorized.
    pub const fn plane(&self) -> P
    where
        P: Copy,
    {
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
    use crate::PrincipalBinding;
    use andromeda_audit::{
        AdminOperation, CertificateIdentity, Permission, UserPrincipal, UserPrincipalKind,
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

    #[derive(Clone, Copy)]
    struct ForbiddenApplicationAdminCase {
        operation: AdminOperation,
        permission: Permission,
        audit_reason_fragment: &'static str,
    }

    fn forbidden_application_admin_cases() -> [ForbiddenApplicationAdminCase; 12] {
        [
            ForbiddenApplicationAdminCase {
                operation: AdminOperation::DebugProcedure,
                permission: Permission::DebugProcedure,
                audit_reason_fragment: "DebugProcedure",
            },
            ForbiddenApplicationAdminCase {
                operation: AdminOperation::ReadProcedureStore,
                permission: Permission::ReadProcedureStore,
                audit_reason_fragment: "ReadProcedureStore",
            },
            ForbiddenApplicationAdminCase {
                operation: AdminOperation::InspectPlans,
                permission: Permission::InspectPlans,
                audit_reason_fragment: "InspectPlans",
            },
            ForbiddenApplicationAdminCase {
                operation: AdminOperation::ManageSecurity,
                permission: Permission::ManageSecurity,
                audit_reason_fragment: "ManageSecurity",
            },
            ForbiddenApplicationAdminCase {
                operation: AdminOperation::RotateCertificate,
                permission: Permission::RotateCertificate,
                audit_reason_fragment: "RotateCertificate",
            },
            ForbiddenApplicationAdminCase {
                operation: AdminOperation::RevokeCertificateIdentity,
                permission: Permission::RevokeCertificateIdentity,
                audit_reason_fragment: "RevokeCertificateIdentity",
            },
            ForbiddenApplicationAdminCase {
                operation: AdminOperation::Backup,
                permission: Permission::Backup,
                audit_reason_fragment: "Backup",
            },
            ForbiddenApplicationAdminCase {
                operation: AdminOperation::Restore,
                permission: Permission::Restore,
                audit_reason_fragment: "Restore",
            },
            ForbiddenApplicationAdminCase {
                operation: AdminOperation::ForensicStart,
                permission: Permission::ForensicStart,
                audit_reason_fragment: "ForensicStart",
            },
            ForbiddenApplicationAdminCase {
                operation: AdminOperation::ClusterPromote,
                permission: Permission::ClusterPromote,
                audit_reason_fragment: "ClusterPromote",
            },
            ForbiddenApplicationAdminCase {
                operation: AdminOperation::FenceNode,
                permission: Permission::FenceNode,
                audit_reason_fragment: "FenceNode",
            },
            ForbiddenApplicationAdminCase {
                operation: AdminOperation::UpdateClusterManifest,
                permission: Permission::UpdateClusterManifest,
                audit_reason_fragment: "UpdateClusterManifest",
            },
        ]
    }

    #[test]
    fn application_surface_admits_procedure_for_bound_principal() {
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
                SurfaceScope::Application,
                "fp-app",
                SurfaceAction::ExecuteProcedure,
            )
            .unwrap();

        assert!(outcome.is_allowed());
        assert_eq!(outcome.audit().outcome, SecurityAuditOutcome::Allowed);
    }

    #[test]
    fn administration_surface_admits_admin_op_only_for_admin_certificate() {
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
                SurfaceScope::Administration,
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
                SurfaceScope::Administration,
                "fp-adm",
                SurfaceAction::Admin(AdminOperation::ManageSecurity),
            )
            .unwrap();
        assert!(allowed.is_allowed());
    }

    #[test]
    fn application_surface_rejects_every_admin_operation() {
        assert!(!SurfaceScope::Application.permits_admin_operation());

        for (index, case) in forbidden_application_admin_cases().into_iter().enumerate() {
            let fingerprint = format!("fp-app-admin-abuse-{index}");
            let principal_id = format!("svc-admin-abuse-{index}");

            assert!(
                !SurfaceScope::Application.permits_permission(case.permission),
                "Application surface must not permit {:?}",
                case.permission
            );

            let reg = registry(vec![binding(
                &fingerprint,
                SurfaceScope::Application,
                &principal_id,
                vec![case.permission],
            )]);
            let gate = SurfacePlaneAuthorizer::new(&reg);
            let action = SurfaceAction::Admin(case.operation);

            let denied = gate
                .authorize_dispatch(
                    TraceId::new(60 + index as u128),
                    SurfaceScope::Application,
                    &fingerprint,
                    action,
                )
                .unwrap();

            assert!(denied.is_denied());
            if let AuthorizationOutcome::Denied { reason, audit } = denied {
                assert_eq!(
                    reason,
                    AuthorizationDenialReason::SurfaceDoesNotPermitPermission
                );
                assert_eq!(audit.outcome, SecurityAuditOutcome::Denied);
                assert!(audit.reason.contains(case.audit_reason_fragment));
                assert!(audit.reason.contains(action.evidence_label()));
            }
        }
    }

    #[test]
    fn monitoring_surface_rejects_procedure_permission() {
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
                SurfaceScope::MonitoringAgent,
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
    fn cluster_surface_admits_cluster_operation() {
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
                SurfaceScope::Cluster,
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
                SurfaceScope::Application,
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
                SurfaceScope::Administration,
                SurfaceScope::Administration,
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
                SurfaceScope::Application,
                SurfaceScope::Application,
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
    fn cluster_procedure_dispatch_is_denied_before_token_issue() {
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
                SurfaceScope::Cluster,
                SurfaceScope::Cluster,
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
                SurfaceScope::Application,
                SurfaceScope::Application,
                "fp-app-proc",
            )
            .unwrap()
            .unwrap();

        assert_eq!(token.plane(), SurfaceScope::Application);
        assert_eq!(token.audit().outcome, SecurityAuditOutcome::Allowed);
    }
}
