use super::*;
use andromeda_types::ProcedureId;

#[test]
fn test_surface_scope_denies_application_crossing_admin_permissions() {
    assert!(
        SurfaceScope::Application
            .permits_permission(&Permission::ExecuteProcedure(ProcedureId::new(42)))
    );
    assert!(SurfaceScope::Application.permits_permission(&Permission::ReadContractMetadata));
    assert!(!SurfaceScope::Application.permits_permission(&Permission::AdminShutdown));
    assert!(SurfaceScope::Administration.permits_permission(&Permission::AdminShutdown));
    assert!(!SurfaceScope::Administration.permits_permission(&Permission::ClusterPromote));
    assert!(!SurfaceScope::Cluster.permits_permission(&Permission::AdminShutdown));
    assert!(SurfaceScope::Cluster.permits_permission(&Permission::ClusterPromote));
}
