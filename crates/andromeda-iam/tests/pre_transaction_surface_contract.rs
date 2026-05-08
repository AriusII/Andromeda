use andromeda_iam::{IamAdmissionRuntime, PreTransactionAdmissionRequest, PrincipalAuthorization};
use andromeda_security_contract::{
    Permission, SecurityAdmissionOutcomeV0, SecurityAdmissionReasonCodeV0, SecurityPolicyEvidence,
    SecurityPolicyVersion, SecuritySurface, SurfaceClass,
};

fn policy_evidence() -> SecurityPolicyEvidence {
    match SecurityPolicyEvidence::for_policy_version(SecurityPolicyVersion::test_vector(0x31)) {
        Ok(evidence) => evidence,
        Err(error) => panic!("test policy evidence should be valid: {error}"),
    }
}

#[test]
fn application_surface_rejects_privileged_permissions_before_transaction_even_with_valid_evidence()
{
    let denied_matrix = [
        (SurfaceClass::Administration, Permission::ManageSecurity),
        (SurfaceClass::Monitoring, Permission::InspectPlans),
        (SurfaceClass::Recovery, Permission::Backup),
        (SurfaceClass::Recovery, Permission::Restore),
        (SurfaceClass::Forensic, Permission::ForensicStart),
        (SurfaceClass::Hadr, Permission::ClusterPromote),
    ];

    for (class, permission) in denied_matrix {
        let principal_permissions = [permission];
        let decision = IamAdmissionRuntime::evaluate(
            PreTransactionAdmissionRequest::new(SecuritySurface::Application, class, permission)
                .with_principal(PrincipalAuthorization::new(&principal_permissions))
                .with_policy_evidence(policy_evidence()),
        );

        assert!(decision.is_denied());
        assert_eq!(decision.receipt(), None);
        assert_eq!(
            decision.admission().outcome(),
            SecurityAdmissionOutcomeV0::Denied
        );
        assert_eq!(
            decision.admission().reason_code(),
            SecurityAdmissionReasonCodeV0::BoundaryPlaneMismatch
        );
        assert!(decision.audit_event().is_denied());
        assert_eq!(
            decision.audit_event().reason_code(),
            SecurityAdmissionReasonCodeV0::BoundaryPlaneMismatch
        );
        assert!(decision.audit_event().policy_evidence_present());
    }
}
