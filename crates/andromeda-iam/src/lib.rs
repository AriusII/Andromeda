#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda IAM

Pre-transaction IAM runtime for security admission.

This crate evaluates authenticated principal permission evidence against the
runtime-free security contract vocabulary. It emits typed admission and audit
event vocabulary, but it does not own certificate parsing, mutable registry
storage, audit sinks, transactions, WAL, storage, or recovery logic.
"#]

use andromeda_audit::SecurityAdmissionAuditEventV0;
use andromeda_security_contract::{
    AdmissionDecision, Permission, PermissionRequest, SecurityAdmissionEvidenceCodeV0,
    SecurityAdmissionReasonCodeV0, SecurityAdmissionStepV0, SecurityAdmissionV0,
    SecurityPolicyEvidence, SecuritySurface, SurfaceClass,
};

/// Runtime view of permissions granted to an authenticated principal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrincipalAuthorization<'a> {
    granted_permissions: &'a [Permission],
}

impl<'a> PrincipalAuthorization<'a> {
    pub const fn new(granted_permissions: &'a [Permission]) -> Self {
        Self {
            granted_permissions,
        }
    }

    pub const fn empty() -> Self {
        Self {
            granted_permissions: &[],
        }
    }

    pub const fn granted_permissions(self) -> &'a [Permission] {
        self.granted_permissions
    }

    pub fn grants(self, permission: Permission) -> bool {
        self.granted_permissions
            .iter()
            .any(|granted| *granted == permission)
    }
}

/// Inputs required before transaction creation or Procedure runtime dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreTransactionAdmissionRequest<'a> {
    surface: SecuritySurface,
    class: SurfaceClass,
    permission: Permission,
    principal: Option<PrincipalAuthorization<'a>>,
    policy_evidence: Option<SecurityPolicyEvidence>,
}

impl<'a> PreTransactionAdmissionRequest<'a> {
    pub const fn new(
        surface: SecuritySurface,
        class: SurfaceClass,
        permission: Permission,
    ) -> Self {
        Self {
            surface,
            class,
            permission,
            principal: None,
            policy_evidence: None,
        }
    }

    pub const fn with_principal(mut self, principal: PrincipalAuthorization<'a>) -> Self {
        self.principal = Some(principal);
        self
    }

    pub const fn with_policy_evidence(mut self, policy_evidence: SecurityPolicyEvidence) -> Self {
        self.policy_evidence = Some(policy_evidence);
        self
    }

    pub const fn surface(self) -> SecuritySurface {
        self.surface
    }

    pub const fn class(self) -> SurfaceClass {
        self.class
    }

    pub const fn permission(self) -> Permission {
        self.permission
    }

    pub const fn principal(self) -> Option<PrincipalAuthorization<'a>> {
        self.principal
    }

    pub const fn policy_evidence(self) -> Option<SecurityPolicyEvidence> {
        self.policy_evidence
    }
}

/// Proof that IAM admission completed before transaction creation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreTransactionAdmissionReceipt {
    request: PermissionRequest,
    policy_evidence: SecurityPolicyEvidence,
}

impl PreTransactionAdmissionReceipt {
    pub const fn request(self) -> PermissionRequest {
        self.request
    }

    pub const fn policy_evidence(self) -> SecurityPolicyEvidence {
        self.policy_evidence
    }

    pub const fn permission(self) -> Permission {
        self.request.permission()
    }

    pub const fn surface(self) -> SecuritySurface {
        self.request.surface()
    }

    pub const fn class(self) -> SurfaceClass {
        self.request.class()
    }
}

/// Fail-closed IAM admission result with audit vocabulary attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreTransactionAdmissionDecision {
    admission: SecurityAdmissionV0,
    audit_event: SecurityAdmissionAuditEventV0,
    receipt: Option<PreTransactionAdmissionReceipt>,
}

impl PreTransactionAdmissionDecision {
    pub const fn admission(self) -> SecurityAdmissionV0 {
        self.admission
    }

    pub const fn audit_event(self) -> SecurityAdmissionAuditEventV0 {
        self.audit_event
    }

    pub const fn receipt(self) -> Option<PreTransactionAdmissionReceipt> {
        self.receipt
    }

    pub const fn is_admitted(self) -> bool {
        self.receipt.is_some() && self.admission.is_allowed()
    }

    pub const fn is_denied(self) -> bool {
        !self.is_admitted()
    }
}

/// Stateless IAM runtime admission evaluator.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IamAdmissionRuntime;

impl IamAdmissionRuntime {
    pub fn evaluate<'a>(
        request: PreTransactionAdmissionRequest<'a>,
    ) -> PreTransactionAdmissionDecision {
        let boundary_decision =
            AdmissionDecision::evaluate(request.surface, request.class, request.permission);
        let Some(permission_request) = boundary_decision.request() else {
            return Self::deny(request, boundary_decision.security_admission_v0());
        };

        let Some(principal) = request.principal else {
            return Self::deny(
                request,
                SecurityAdmissionV0::denied(
                    SecurityAdmissionStepV0::PrincipalBinding,
                    SecurityAdmissionEvidenceCodeV0::PrincipalBinding,
                    SecurityAdmissionReasonCodeV0::PrincipalBindingMissing,
                ),
            );
        };

        let Some(policy_evidence) = request.policy_evidence else {
            return Self::deny(
                request,
                SecurityAdmissionV0::missing_evidence(SecurityAdmissionStepV0::PolicyEvidence),
            );
        };

        if !principal.grants(request.permission) {
            return Self::deny(
                request,
                SecurityAdmissionV0::denied(
                    SecurityAdmissionStepV0::PermissionBoundary,
                    SecurityAdmissionEvidenceCodeV0::PermissionFamily,
                    SecurityAdmissionReasonCodeV0::PermissionNotGranted,
                ),
            );
        }

        let admission = SecurityAdmissionV0::allowed(
            SecurityAdmissionStepV0::DecisionEvidence,
            SecurityAdmissionEvidenceCodeV0::OutcomeReason,
        );
        PreTransactionAdmissionDecision {
            admission,
            audit_event: Self::audit_event(request, admission),
            receipt: Some(PreTransactionAdmissionReceipt {
                request: permission_request,
                policy_evidence,
            }),
        }
    }

    fn deny<'a>(
        request: PreTransactionAdmissionRequest<'a>,
        admission: SecurityAdmissionV0,
    ) -> PreTransactionAdmissionDecision {
        PreTransactionAdmissionDecision {
            admission,
            audit_event: Self::audit_event(request, admission),
            receipt: None,
        }
    }

    const fn audit_event<'a>(
        request: PreTransactionAdmissionRequest<'a>,
        admission: SecurityAdmissionV0,
    ) -> SecurityAdmissionAuditEventV0 {
        SecurityAdmissionAuditEventV0::new(
            admission,
            request.surface,
            request.class,
            request.permission,
            request.policy_evidence.is_some(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_security_contract::{
        SecurityAdmissionOutcomeV0, SecurityAdmissionReasonCodeV0, SecurityPolicyVersion,
    };

    fn policy_evidence() -> SecurityPolicyEvidence {
        match SecurityPolicyEvidence::for_policy_version(SecurityPolicyVersion::test_vector(0x27)) {
            Ok(evidence) => evidence,
            Err(error) => panic!("test policy evidence should be valid: {error}"),
        }
    }

    #[test]
    fn test_admission_allows_principal_with_permission_and_policy_evidence() {
        let permissions = [Permission::ExecuteProcedure];
        let request = PreTransactionAdmissionRequest::new(
            SecuritySurface::Application,
            SurfaceClass::Application,
            Permission::ExecuteProcedure,
        )
        .with_principal(PrincipalAuthorization::new(&permissions))
        .with_policy_evidence(policy_evidence());

        let decision = IamAdmissionRuntime::evaluate(request);

        assert!(decision.is_admitted());
        assert_eq!(
            decision.admission().outcome(),
            SecurityAdmissionOutcomeV0::Allowed
        );
        assert!(decision.audit_event().is_allowed());
        assert!(decision.audit_event().policy_evidence_present());
        assert_eq!(
            decision
                .receipt()
                .map(PreTransactionAdmissionReceipt::permission),
            Some(Permission::ExecuteProcedure)
        );
    }

    #[test]
    fn test_admission_denies_without_principal_binding() {
        let request = PreTransactionAdmissionRequest::new(
            SecuritySurface::Application,
            SurfaceClass::Application,
            Permission::ExecuteProcedure,
        )
        .with_policy_evidence(policy_evidence());

        let decision = IamAdmissionRuntime::evaluate(request);

        assert!(decision.is_denied());
        assert_eq!(decision.receipt(), None);
        assert_eq!(
            decision.admission().reason_code(),
            SecurityAdmissionReasonCodeV0::PrincipalBindingMissing
        );
        assert!(decision.audit_event().is_denied());
    }

    #[test]
    fn test_admission_denies_without_policy_evidence() {
        let permissions = [Permission::ExecuteProcedure];
        let request = PreTransactionAdmissionRequest::new(
            SecuritySurface::Application,
            SurfaceClass::Application,
            Permission::ExecuteProcedure,
        )
        .with_principal(PrincipalAuthorization::new(&permissions));

        let decision = IamAdmissionRuntime::evaluate(request);

        assert!(decision.is_denied());
        assert_eq!(decision.receipt(), None);
        assert_eq!(
            decision.admission().reason_code(),
            SecurityAdmissionReasonCodeV0::MissingEvidence
        );
        assert!(!decision.audit_event().policy_evidence_present());
    }

    #[test]
    fn test_admission_denies_missing_permission_even_with_policy_evidence() {
        let permissions = [Permission::ReadContract];
        let request = PreTransactionAdmissionRequest::new(
            SecuritySurface::Application,
            SurfaceClass::Application,
            Permission::ExecuteProcedure,
        )
        .with_principal(PrincipalAuthorization::new(&permissions))
        .with_policy_evidence(policy_evidence());

        let decision = IamAdmissionRuntime::evaluate(request);

        assert!(decision.is_denied());
        assert_eq!(decision.receipt(), None);
        assert_eq!(
            decision.admission().reason_code(),
            SecurityAdmissionReasonCodeV0::PermissionNotGranted
        );
        assert!(decision.audit_event().policy_evidence_present());
    }

    #[test]
    fn test_application_surface_privilege_escalation_fails_before_principal_policy() {
        let permissions = [Permission::ManageSecurity];
        let request = PreTransactionAdmissionRequest::new(
            SecuritySurface::Application,
            SurfaceClass::Administration,
            Permission::ManageSecurity,
        )
        .with_principal(PrincipalAuthorization::new(&permissions))
        .with_policy_evidence(policy_evidence());

        let decision = IamAdmissionRuntime::evaluate(request);

        assert!(decision.is_denied());
        assert_eq!(
            decision.admission().reason_code(),
            SecurityAdmissionReasonCodeV0::BoundaryPlaneMismatch
        );
        assert_eq!(decision.receipt(), None);
    }
}
