use super::*;
use andromeda_types::ProcedureId;

#[test]
fn test_principal_role_str_conversion() {
    assert_eq!(PrincipalRole::SuperAdmin.as_str(), "superadmin");
    assert_eq!(PrincipalRole::Admin.as_str(), "admin");
    assert_eq!(PrincipalRole::Operator.as_str(), "operator");
    assert_eq!(PrincipalRole::User.as_str(), "user");
    assert_eq!(PrincipalRole::Guest.as_str(), "guest");
}

#[test]
fn test_principal_role_from_str() {
    assert_eq!(
        "superadmin".parse::<PrincipalRole>().ok(),
        Some(PrincipalRole::SuperAdmin)
    );
    assert_eq!(
        "admin".parse::<PrincipalRole>().ok(),
        Some(PrincipalRole::Admin)
    );
    assert_eq!(
        "user".parse::<PrincipalRole>().ok(),
        Some(PrincipalRole::User)
    );
    assert_eq!("invalid".parse::<PrincipalRole>().ok(), None);
}

#[test]
fn test_principal_role_display() {
    assert_eq!(PrincipalRole::SuperAdmin.to_string(), "superadmin");
    assert_eq!(PrincipalRole::Operator.to_string(), "operator");
}

#[test]
fn test_permission_execute_procedure() {
    let perm = Permission::ExecuteProcedure(ProcedureId::new(42));
    assert_eq!(perm.as_str(), "execute_procedure");
}

#[test]
fn test_permission_matches_exact() {
    let perm = Permission::ExecuteProcedure(ProcedureId::new(42));
    let required = Permission::ExecuteProcedure(ProcedureId::new(42));
    assert!(perm.matches(&required));
}

#[test]
fn test_permission_matches_wildcard() {
    let perm = Permission::ExecuteProcedure(ProcedureId::new(u64::MAX));
    let required = Permission::ExecuteProcedure(ProcedureId::new(42));
    assert!(perm.matches(&required));
}

#[test]
fn test_permission_does_not_match() {
    let perm = Permission::ExecuteProcedure(ProcedureId::new(10));
    let required = Permission::ExecuteProcedure(ProcedureId::new(42));
    assert!(!perm.matches(&required));
}

#[test]
fn test_permission_display() {
    let perm = Permission::ExecuteProcedure(ProcedureId::new(42));
    assert_eq!(perm.to_string(), "execute_procedure(42)");

    let perm2 = Permission::AdminCatalogPublish;
    assert_eq!(perm2.to_string(), "admin_catalog_publish");
}

#[test]
fn test_permission_set_has_permission() {
    let set = PermissionSet::new()
        .with_permission(Permission::AdminCatalogPublish)
        .with_permission(Permission::AuditRead);

    assert!(set.has_permission(&Permission::AdminCatalogPublish));
    assert!(set.has_permission(&Permission::AuditRead));
    assert!(!set.has_permission(&Permission::AdminShutdown));
}

#[test]
fn test_permission_set_empty() {
    let set = PermissionSet::new();
    assert!(set.is_empty());
    assert_eq!(set.len(), 0);
}

#[test]
fn test_permission_set_from_vec() {
    let perms = vec![
        Permission::AdminCatalogPublish,
        Permission::AuditRead,
        Permission::AuditRead,
    ];
    let set = PermissionSet::from_vec(perms);
    assert_eq!(set.len(), 2);
}

#[test]
fn test_super_admin_has_all_permissions() {
    let perms = PrincipalRole::SuperAdmin.permissions();
    assert!(perms.has_permission(&Permission::AdminShutdown));
    assert!(perms.has_permission(&Permission::AdminRecovery));
    assert!(perms.has_permission(&Permission::AuditRead));
    assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
}

#[test]
fn test_guest_limited_permissions() {
    let perms = PrincipalRole::Guest.permissions();
    assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(0))));
    assert!(!perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
}

#[test]
fn test_operator_permissions() {
    let perms = PrincipalRole::Operator.permissions();
    assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
    assert!(perms.has_permission(&Permission::AuditRead));
    assert!(!perms.has_permission(&Permission::AdminShutdown));
}

#[test]
fn test_admin_permissions() {
    let perms = PrincipalRole::Admin.permissions();
    assert!(perms.has_permission(&Permission::AdminRoleManagement));
    assert!(!perms.has_permission(&Permission::AdminShutdown));
}

#[test]
fn test_user_permissions() {
    let perms = PrincipalRole::User.permissions();
    assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
    assert!(perms.has_permission(&Permission::ReadContractMetadata));
    assert!(!perms.has_permission(&Permission::AdminRoleManagement));
}
