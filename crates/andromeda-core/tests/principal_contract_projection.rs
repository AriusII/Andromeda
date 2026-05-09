use andromeda_core::PrincipalPolicyVersion;
use andromeda_security_contract::{
    PrincipalPermission as Permission, SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION, SecurityPermission,
    SecurityPolicyVersion, SecuritySurface, SurfaceScope,
};
use andromeda_types::ProcedureId;

#[test]
fn surface_scope_projects_to_security_contract_surface() {
    assert_eq!(
        SurfaceScope::Application.to_security_surface(),
        SecuritySurface::Application
    );
    assert_eq!(
        SurfaceScope::Administration.to_security_surface(),
        SecuritySurface::Administration
    );
    assert_eq!(
        SurfaceScope::Cluster.to_security_surface(),
        SecuritySurface::Cluster
    );
    assert_eq!(
        SurfaceScope::BackupAgent.to_security_surface(),
        SecuritySurface::BackupAgent
    );
    assert_eq!(
        SurfaceScope::MonitoringAgent.to_security_surface(),
        SecuritySurface::MonitoringAgent
    );
}

#[test]
fn permission_projection_maps_only_exact_security_contract_permissions() {
    assert_eq!(
        Permission::ReadContractMetadata.to_security_permission(),
        Some(SecurityPermission::ReadContractMetadata)
    );
    assert_eq!(
        Permission::AuditRead.to_security_permission(),
        Some(SecurityPermission::ReadAudit)
    );
    assert_eq!(
        Permission::AdminCertificateRotate.to_security_permission(),
        Some(SecurityPermission::RotateCertificate)
    );
    assert_eq!(
        Permission::ClusterPromote.to_security_permission(),
        Some(SecurityPermission::ClusterPromote)
    );
    assert_eq!(
        Permission::ClusterFenceNode.to_security_permission(),
        Some(SecurityPermission::ClusterFenceNode)
    );
    assert_eq!(
        Permission::ClusterManifestUpdate.to_security_permission(),
        Some(SecurityPermission::ClusterUpdateManifest)
    );
}

#[test]
fn execute_procedure_projection_keeps_procedure_id_separate() {
    let procedure_id = ProcedureId::new(42);
    let permission = Permission::ExecuteProcedure(procedure_id);

    assert_eq!(
        permission.to_security_permission(),
        Some(SecurityPermission::ExecuteProcedure)
    );
    assert_eq!(
        permission.security_contract_procedure_id(),
        Some(procedure_id)
    );
}

#[test]
fn ambiguous_admin_permissions_are_not_broadened_into_contract_permissions() {
    for permission in [
        Permission::AdminRoleManagement,
        Permission::AdminCatalogPublish,
        Permission::AdminShutdown,
        Permission::AdminRecovery,
    ] {
        assert_eq!(permission.to_security_permission(), None);
        assert_eq!(permission.security_contract_procedure_id(), None);
    }
}

#[test]
fn principal_policy_version_projects_to_security_contract_policy_version() {
    let policy_version = PrincipalPolicyVersion::test_vector(0x5a);
    let security_version = policy_version.to_security_policy_version();

    assert_eq!(
        security_version,
        SecurityPolicyVersion::new(policy_version.as_bytes())
    );

    let evidence = policy_version
        .to_security_policy_evidence()
        .expect("non-zero policy version projects to security evidence");

    assert_eq!(
        evidence.schema_version(),
        SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION
    );
    assert_eq!(evidence.policy_version(), security_version);
    assert!(evidence.has_version_evidence());
}
