use andromeda_principal::{Permission, PermissionSet};
use andromeda_types::ProcedureId;

#[test]
fn test_permission_matching_exact_procedure() {
    let perm = Permission::ExecuteProcedure(ProcedureId::new(42));
    let required = Permission::ExecuteProcedure(ProcedureId::new(42));

    assert!(perm.matches(&required));
}

#[test]
fn test_permission_matching_wildcard_procedure() {
    let perm = Permission::ExecuteProcedure(ProcedureId::new(u64::MAX));
    let required = Permission::ExecuteProcedure(ProcedureId::new(42));

    assert!(perm.matches(&required));
}

#[test]
fn test_permission_matching_specific_does_not_satisfy_required_wildcard() {
    let granted = Permission::ExecuteProcedure(ProcedureId::new(42));
    let required_wildcard = Permission::ExecuteProcedure(ProcedureId::new(u64::MAX));

    assert!(!granted.matches(&required_wildcard));
}

#[test]
fn test_permission_set_matching_is_directional_for_wildcard() {
    let wildcard_grant = PermissionSet::from_vec(vec![Permission::ExecuteProcedure(
        ProcedureId::new(u64::MAX),
    )]);
    let specific_grant =
        PermissionSet::from_vec(vec![Permission::ExecuteProcedure(ProcedureId::new(42))]);

    assert!(wildcard_grant.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
    assert!(
        !specific_grant.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(u64::MAX)))
    );
}

#[test]
fn test_permission_matching_non_procedure() {
    let perm = Permission::AdminCatalogPublish;
    let required = Permission::AdminCatalogPublish;

    assert!(perm.matches(&required));
}
