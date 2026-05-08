use andromeda_security_contract::{
    ALL_SURFACE_CLASSES, AdmissionDecision, Permission, PermissionRequest,
    SecurityAdmissionEvidenceCodeV0, SecurityAdmissionOutcomeV0, SecurityAdmissionReasonCodeV0,
    SecurityAdmissionStepV0, SecurityContractError, SecuritySurface, SurfaceClass,
};

#[test]
fn surface_class_codes_roundtrip() {
    for class in ALL_SURFACE_CLASSES {
        assert_eq!(SurfaceClass::from_code(class.as_str()), Some(class));
        assert_eq!(class.to_string(), class.as_str());
    }

    assert_eq!(SurfaceClass::from_code("application/admin"), None);
}

#[test]
fn permission_request_admits_only_matching_surface_class_and_permission() {
    assert!(
        PermissionRequest::new(
            SecuritySurface::Application,
            SurfaceClass::Application,
            Permission::ExecuteProcedure,
        )
        .is_ok_and(PermissionRequest::is_application_work)
    );

    assert!(
        PermissionRequest::new(
            SecuritySurface::Administration,
            SurfaceClass::Administration,
            Permission::ManageSecurity,
        )
        .is_ok()
    );
    assert!(
        PermissionRequest::new(
            SecuritySurface::Cluster,
            SurfaceClass::Hadr,
            Permission::ClusterPromote,
        )
        .is_ok()
    );
    assert!(
        PermissionRequest::new(
            SecuritySurface::BackupAgent,
            SurfaceClass::Recovery,
            Permission::Restore,
        )
        .is_ok()
    );
    assert!(
        PermissionRequest::new(
            SecuritySurface::Administration,
            SurfaceClass::Forensic,
            Permission::ForensicStart,
        )
        .is_ok()
    );
    assert!(
        PermissionRequest::new(
            SecuritySurface::MonitoringAgent,
            SurfaceClass::Monitoring,
            Permission::InspectPlans,
        )
        .is_ok()
    );
}

#[test]
fn permission_request_rejects_application_surface_privilege_escalation() {
    assert_eq!(
        PermissionRequest::new(
            SecuritySurface::Application,
            SurfaceClass::Administration,
            Permission::ManageSecurity,
        ),
        Err(SecurityContractError::SurfaceClassBoundaryMismatch)
    );
    assert_eq!(
        PermissionRequest::new(
            SecuritySurface::Application,
            SurfaceClass::Hadr,
            Permission::ClusterPromote,
        ),
        Err(SecurityContractError::SurfaceClassBoundaryMismatch)
    );
    assert_eq!(
        PermissionRequest::new(
            SecuritySurface::Application,
            SurfaceClass::Forensic,
            Permission::ForensicStart,
        ),
        Err(SecurityContractError::SurfaceClassBoundaryMismatch)
    );
    assert_eq!(
        PermissionRequest::new(
            SecuritySurface::Application,
            SurfaceClass::Recovery,
            Permission::Restore,
        ),
        Err(SecurityContractError::SurfaceClassBoundaryMismatch)
    );
    assert_eq!(
        PermissionRequest::new(
            SecuritySurface::Application,
            SurfaceClass::Application,
            Permission::ManageSecurity,
        ),
        Err(SecurityContractError::SurfacePermissionBoundaryMismatch)
    );
}

#[test]
fn admission_decision_fails_closed_for_privileged_application_routes() {
    let application = AdmissionDecision::evaluate(
        SecuritySurface::Application,
        SurfaceClass::Application,
        Permission::ExecuteProcedure,
    );
    assert!(application.is_admitted());
    assert_eq!(application.outcome(), SecurityAdmissionOutcomeV0::Allowed);
    assert_eq!(
        application.security_admission_v0().outcome(),
        SecurityAdmissionOutcomeV0::Allowed
    );
    assert!(
        application
            .request()
            .is_some_and(PermissionRequest::is_application_work)
    );

    for (class, permission) in [
        (SurfaceClass::Administration, Permission::ManageSecurity),
        (SurfaceClass::Hadr, Permission::ClusterPromote),
        (SurfaceClass::Recovery, Permission::Restore),
        (SurfaceClass::Forensic, Permission::ForensicStart),
    ] {
        let decision = AdmissionDecision::evaluate(SecuritySurface::Application, class, permission);

        assert!(decision.is_denied());
        assert_eq!(
            decision.reason_code(),
            SecurityAdmissionReasonCodeV0::BoundaryPlaneMismatch
        );
        assert_eq!(decision.request(), None);
        assert_eq!(
            decision.security_admission_v0().step(),
            SecurityAdmissionStepV0::SurfaceBoundary
        );
        assert_eq!(
            decision.security_admission_v0().evidence(),
            SecurityAdmissionEvidenceCodeV0::SurfaceBoundary
        );
        assert_eq!(
            decision.security_admission_v0().outcome(),
            SecurityAdmissionOutcomeV0::Denied
        );
    }

    let mislabeled_admin_permission = AdmissionDecision::evaluate(
        SecuritySurface::Application,
        SurfaceClass::Application,
        Permission::ManageSecurity,
    );
    assert!(mislabeled_admin_permission.is_denied());
    assert_eq!(
        mislabeled_admin_permission.reason_code(),
        SecurityAdmissionReasonCodeV0::SurfacePermissionBoundaryMismatch
    );
    assert_eq!(
        mislabeled_admin_permission.security_admission_v0().step(),
        SecurityAdmissionStepV0::PermissionBoundary
    );
}
