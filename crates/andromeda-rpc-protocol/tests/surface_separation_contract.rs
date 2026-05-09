use andromeda_principal::{Permission, SurfaceScope};
use andromeda_types::ProcedureId;

const SECURITY_PERMISSION_SOURCE: &str =
    include_str!("../../../crates/andromeda-security-contract/src/permission.rs");
const SECURITY_ADMISSION_SOURCE: &str =
    include_str!("../../../crates/andromeda-security-contract/src/admission.rs");
const QUIC_ROUTE_SOURCE: &str =
    include_str!("../../../crates/andromeda-quic/src/procedure_gateway/route.rs");
const SURFACE_SPEC: &str = include_str!("../../../documentations/specs/SurfaceSeparation_v0.md");

#[test]
fn application_scope_denies_core_admin_and_hadr_permissions() {
    assert!(
        SurfaceScope::Application
            .permits_permission(&Permission::ExecuteProcedure(ProcedureId::new(42)))
    );
    assert!(SurfaceScope::Application.permits_permission(&Permission::ReadContractMetadata));

    let privileged_permissions = [
        ("admin role management", Permission::AdminRoleManagement),
        ("admin catalog publish", Permission::AdminCatalogPublish),
        ("admin shutdown", Permission::AdminShutdown),
        ("admin recovery", Permission::AdminRecovery),
        ("audit read", Permission::AuditRead),
        (
            "admin certificate rotation",
            Permission::AdminCertificateRotate,
        ),
        ("cluster promote", Permission::ClusterPromote),
        ("cluster fence node", Permission::ClusterFenceNode),
        ("cluster manifest update", Permission::ClusterManifestUpdate),
    ];

    for (label, permission) in privileged_permissions {
        assert!(
            !SurfaceScope::Application.permits_permission(&permission),
            "Application surface must not permit privileged {label} permission"
        );
    }
}

#[test]
fn security_contract_keeps_forensic_and_hadr_out_of_application_work() {
    let permission_source = compact(SECURITY_PERMISSION_SOURCE);
    assert_contains(
        &permission_source,
        "Self::Backup|Self::Restore|Self::ForensicStart=>PermissionFamily::Recovery",
    );
    assert_contains(
        &permission_source,
        "Self::ClusterPromote|Self::ClusterFenceNode|Self::ClusterUpdateManifest=>{PermissionFamily::Cluster}",
    );

    let admission_source = compact(SECURITY_ADMISSION_SOURCE);
    assert_contains(
        &admission_source,
        "Self::Application=>matches!(permission.family(),PermissionFamily::Application)",
    );
    assert_contains(
        &admission_source,
        "Self::Hadr=>matches!(permission.family(),PermissionFamily::Cluster)",
    );
    assert_contains(
        &admission_source,
        "Self::Forensic=>matches!(permission,Permission::ForensicStart)",
    );
}

#[test]
fn quic_application_route_has_explicit_surface_and_hadr_stream_gates() {
    let route_source = compact(QUIC_ROUTE_SOURCE);

    assert_contains(
        &route_source,
        "ifadmission.plane!=SurfacePlane::Application",
    );
    assert_contains(&route_source, "validate_application_stream_id(stream_id)?;");
    assert_contains(
        &route_source,
        "(HADR_STREAM_MIN..=HADR_STREAM_MAX).contains(&stream_id)",
    );
    assert_contains(
        &route_source,
        "applicationProcedureroutecannotuseHA/DRreservedstreamid",
    );
}

#[test]
fn surface_separation_spec_excludes_privileged_operations_from_application() {
    let compact_spec = compact(SURFACE_SPEC);

    for required in [
        "Application surface may invoke typed, cataloged Procedures",
        "Administration, HA/DR, backup, restore, forensic startup, security management",
        "Application routes carry typed, cataloged Procedures only",
        "Forensic startup is inspection-only",
        "V0 reserves stream ids `128..=255` for HA/DR control",
        "Failure at gates 1 through 8 is a route, protocol, contract, or surface rejection before IAM authorization evidence is produced",
    ] {
        let required_compact = compact(required);
        assert!(
            compact_spec.contains(&required_compact),
            "SurfaceSeparation_v0.md must preserve invariant text: {required_compact}"
        );
    }
}

fn compact(source: &str) -> String {
    source.split_whitespace().collect::<String>()
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected invariant fragment not found: {needle}"
    );
}
