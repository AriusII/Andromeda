use andromeda_audit::{
    SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_ID, SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_VERSION,
    SecurityAdmissionAuditEventV0,
};
use andromeda_security_contract::{
    Permission, SecurityAdmissionEvidenceCodeV0, SecurityAdmissionReasonCodeV0,
    SecurityAdmissionStepV0, SecurityAdmissionV0, SecuritySurface, SurfaceClass,
};

#[test]
fn security_admission_audit_event_preserves_runtime_free_codes() {
    let admission = SecurityAdmissionV0::denied(
        SecurityAdmissionStepV0::PrincipalBinding,
        SecurityAdmissionEvidenceCodeV0::PrincipalBinding,
        SecurityAdmissionReasonCodeV0::PrincipalBindingMissing,
    );
    let event = SecurityAdmissionAuditEventV0::new(
        admission,
        SecuritySurface::Application,
        SurfaceClass::Application,
        Permission::ExecuteProcedure,
        false,
    );

    assert_eq!(
        event.schema_id(),
        SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_ID
    );
    assert_eq!(
        event.schema_version(),
        SECURITY_ADMISSION_AUDIT_EVENT_V0_SCHEMA_VERSION
    );
    assert_eq!(event.family(), "security_admission");
    assert_eq!(event.phase(), "pre_transaction_admission");
    assert_eq!(event.step(), SecurityAdmissionStepV0::PrincipalBinding);
    assert_eq!(
        event.evidence(),
        SecurityAdmissionEvidenceCodeV0::PrincipalBinding
    );
    assert_eq!(
        event.reason_code(),
        SecurityAdmissionReasonCodeV0::PrincipalBindingMissing
    );
    assert_eq!(event.surface(), SecuritySurface::Application);
    assert_eq!(event.class(), SurfaceClass::Application);
    assert_eq!(event.permission(), Permission::ExecuteProcedure);
    assert!(event.is_denied());
    assert!(!event.policy_evidence_present());
}
