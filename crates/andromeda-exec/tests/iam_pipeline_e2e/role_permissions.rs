use andromeda_core::{Permission, PrincipalRole, ProcedureId};

#[test]
fn test_super_admin_permission_set() {
    let perms = PrincipalRole::SuperAdmin.permissions();

    assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
    assert!(perms.has_permission(&Permission::AdminCatalogPublish));
    assert!(perms.has_permission(&Permission::AdminShutdown));
    assert!(perms.has_permission(&Permission::AdminRecovery));
    assert!(perms.has_permission(&Permission::AuditRead));
    assert!(perms.has_permission(&Permission::AdminCertificateRotate));
    assert!(perms.has_permission(&Permission::AdminRoleManagement));
}

#[test]
fn test_admin_permission_set() {
    let perms = PrincipalRole::Admin.permissions();

    assert!(perms.has_permission(&Permission::AdminCatalogPublish));
    assert!(perms.has_permission(&Permission::AdminRecovery));
    assert!(perms.has_permission(&Permission::AuditRead));
    assert!(!perms.has_permission(&Permission::AdminShutdown));
}

#[test]
fn test_operator_permission_set() {
    let perms = PrincipalRole::Operator.permissions();

    assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
    assert!(perms.has_permission(&Permission::AuditRead));
    assert!(!perms.has_permission(&Permission::AdminCatalogPublish));
}

#[test]
fn test_user_permission_set() {
    let perms = PrincipalRole::User.permissions();

    assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
    assert!(!perms.has_permission(&Permission::AdminCatalogPublish));
    assert!(!perms.has_permission(&Permission::AuditRead));
}

#[test]
fn test_guest_permission_set_restricted() {
    let perms = PrincipalRole::Guest.permissions();

    assert!(perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(0))));
    assert!(!perms.has_permission(&Permission::ExecuteProcedure(ProcedureId::new(42))));
}

#[test]
fn test_no_permission_inheritance_between_roles() {
    let admin_perms = PrincipalRole::Admin.permissions();
    let user_perms = PrincipalRole::User.permissions();

    assert!(admin_perms.has_permission(&Permission::AdminCatalogPublish));
    assert!(!user_perms.has_permission(&Permission::AdminCatalogPublish));
}
