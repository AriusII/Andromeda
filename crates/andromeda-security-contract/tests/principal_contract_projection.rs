use andromeda_security_contract::{
    PrincipalPermission, SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION, SecurityPermission,
    SecurityPolicyEvidence, SecurityPolicyVersion, SecuritySurface, SurfaceScope,
};
use andromeda_types::ProcedureId;

#[test]
fn principal_surface_scope_projects_to_security_contract_surface() {
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
fn principal_permission_projection_maps_only_exact_security_contract_permissions() {
    assert_eq!(
        PrincipalPermission::ReadContractMetadata.to_security_permission(),
        Some(SecurityPermission::ReadContractMetadata)
    );
    assert_eq!(
        PrincipalPermission::AuditRead.to_security_permission(),
        Some(SecurityPermission::ReadAudit)
    );
    assert_eq!(
        PrincipalPermission::AdminCertificateRotate.to_security_permission(),
        Some(SecurityPermission::RotateCertificate)
    );
    assert_eq!(
        PrincipalPermission::ClusterPromote.to_security_permission(),
        Some(SecurityPermission::ClusterPromote)
    );
    assert_eq!(
        PrincipalPermission::ClusterFenceNode.to_security_permission(),
        Some(SecurityPermission::ClusterFenceNode)
    );
    assert_eq!(
        PrincipalPermission::ClusterManifestUpdate.to_security_permission(),
        Some(SecurityPermission::ClusterUpdateManifest)
    );
}

#[test]
fn execute_procedure_projection_keeps_procedure_id_separate() {
    let procedure_id = ProcedureId::new(42);
    let permission = PrincipalPermission::ExecuteProcedure(procedure_id);

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
        PrincipalPermission::AdminRoleManagement,
        PrincipalPermission::AdminCatalogPublish,
        PrincipalPermission::AdminShutdown,
        PrincipalPermission::AdminRecovery,
    ] {
        assert_eq!(permission.to_security_permission(), None);
        assert_eq!(permission.security_contract_procedure_id(), None);
    }
}

#[test]
fn security_policy_evidence_validates_contract_policy_version_shape() {
    let policy_version = SecurityPolicyVersion::test_vector(0x5a);
    let evidence = SecurityPolicyEvidence::for_policy_version(policy_version)
        .expect("non-zero policy version projects to security evidence");

    assert_eq!(
        evidence.schema_version(),
        SECURITY_POLICY_EVIDENCE_SCHEMA_VERSION
    );
    assert_eq!(evidence.policy_version(), policy_version);
    assert!(evidence.has_version_evidence());
}
