use super::*;

const HADR_STREAM_MIN: u64 = 128;
const HADR_STREAM_MAX: u64 = 255;
use andromeda_security_contract::{
    AdmissionDecision, Permission as SecurityPermission, SecurityAdmissionOutcomeV0,
    SecurityAdmissionReasonCodeV0, SecuritySurface, SurfaceClass,
};

#[test]
fn security_contract_denies_privileged_work_on_application_surface() {
    let cases = [
        (
            SurfaceClass::Administration,
            SecurityPermission::ManageSecurity,
            "administration security management",
        ),
        (
            SurfaceClass::Hadr,
            SecurityPermission::ClusterPromote,
            "HA/DR promotion",
        ),
        (
            SurfaceClass::Recovery,
            SecurityPermission::Restore,
            "restore",
        ),
        (
            SurfaceClass::Forensic,
            SecurityPermission::ForensicStart,
            "forensic startup",
        ),
    ];

    for (class, permission, label) in cases {
        let decision = AdmissionDecision::evaluate(SecuritySurface::Application, class, permission);

        assert!(
            decision.is_denied(),
            "Application surface must deny privileged {label} work"
        );
        assert_eq!(decision.outcome(), SecurityAdmissionOutcomeV0::Denied);
        assert_eq!(
            decision.reason_code(),
            SecurityAdmissionReasonCodeV0::BoundaryPlaneMismatch,
            "privileged {label} work must fail at the surface boundary"
        );
        assert!(
            decision.request().is_none(),
            "denied privileged {label} work must not yield an admitted request"
        );
        assert!(
            !decision.security_admission_v0().is_allowed(),
            "security admission evidence for {label} must remain denied"
        );
    }
}

#[test]
fn application_route_rejects_privileged_manifest_permissions_before_iam_or_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let (registry, _) = registry_for_application_user();

    for (family, id, label) in [
        (
            "security",
            "andromeda.security.manage_security",
            "administration security management",
        ),
        ("cluster", "andromeda.cluster.promote", "HA/DR promotion"),
        (
            "recovery",
            "andromeda.recovery.forensic_start",
            "forensic startup",
        ),
        ("recovery", "andromeda.recovery.restore", "restore"),
    ] {
        let mut manifest = route_manifest();
        manifest
            .required_permissions
            .push(CatalogRequiredPermission {
                id: id.to_string(),
                family: family.to_string(),
            });
        let frame = valid_execute_frame(&manifest);

        let err = gateway
            .bind_authorized_application_procedure_route(91, &frame, &manifest, &registry)
            .unwrap_err();

        assert_route_rejection_before_authorization(
            err,
            AndromedaErrorKind::Contract,
            "non-Application permission",
        );

        assert!(
            !SecuritySurface::Application
                .permits_permission(SecurityPermission::from_canonical_id(id).unwrap()),
            "Application security surface must not permit privileged {label} permission {id}"
        );
    }
}

#[test]
fn application_route_rejects_privileged_request_surface_scopes_before_iam_or_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let (registry, _) = registry_for_application_user();

    for (surface_scope, label) in [
        ("administration", "Administration"),
        ("cluster", "HA/DR"),
        ("backup_agent", "BackupAgent"),
        ("forensic", "forensic"),
    ] {
        let frame = execute_request_frame(
            &manifest.procedure_name,
            manifest.contract_hash,
            manifest.catalog_version,
            Some(manifest.stats_version),
            surface_scope,
            manifest.contract_hash,
            manifest.catalog_version,
        );

        let err = gateway
            .bind_authorized_application_procedure_route(92, &frame, &manifest, &registry)
            .unwrap_err();

        assert_route_rejection_before_authorization(
            err,
            AndromedaErrorKind::Security,
            "surface_scope",
        );
        assert_ne!(
            surface_scope, "application",
            "{label} surface scope must stay outside Application route admission"
        );
    }
}

#[test]
fn application_route_rejects_hadr_stream_namespace_before_iam_or_dispatch() {
    let conn = setup_active_application_connection();
    let gateway = ProcedureGateway::new(&conn).expect("gateway construction failed");
    let manifest = route_manifest();
    let frame = valid_execute_frame(&manifest);
    let (registry, _) = registry_for_application_user();

    for stream_id in [HADR_STREAM_MIN, HADR_STREAM_MAX] {
        let err = gateway
            .bind_authorized_application_procedure_route(stream_id, &frame, &manifest, &registry)
            .unwrap_err();

        assert_route_rejection_before_authorization(
            err,
            AndromedaErrorKind::Security,
            "HA/DR reserved stream id",
        );
    }
}
