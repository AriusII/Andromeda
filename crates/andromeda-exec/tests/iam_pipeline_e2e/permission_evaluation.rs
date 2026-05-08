use crate::support::{evaluator_for, new_resolver, resolver_and_evaluator};
use andromeda_core::{Permission, PrincipalRole, ProcedureId};
use andromeda_exec::services::{DenialReason, PermissionDecision, PermissionEvaluator};

#[test]
fn test_permission_evaluation_allowed() {
    let (_resolver, evaluator) = resolver_and_evaluator("test_admin", PrincipalRole::Admin);

    let decision = evaluator.evaluate_permission("test_admin", &Permission::AdminCatalogPublish);

    assert!(decision.is_allowed());
    match decision {
        PermissionDecision::Allowed { principal_id, .. } => {
            assert!(!principal_id.is_zero());
        },
        _ => panic!("expected allowed decision"),
    }
}

#[test]
fn test_permission_evaluation_denied_missing() {
    let (_resolver, evaluator) = resolver_and_evaluator("test_user", PrincipalRole::User);

    let decision = evaluator.evaluate_permission("test_user", &Permission::AdminShutdown);

    assert!(decision.is_denied());
    match decision {
        PermissionDecision::Denied {
            reason,
            principal_id,
            ..
        } => {
            assert_eq!(reason, DenialReason::MissingPermission);
            assert!(principal_id.is_some());
        },
        _ => panic!("expected denied decision"),
    }
}

#[test]
fn test_permission_evaluation_denied_principal_not_found() {
    let resolver = new_resolver();
    let evaluator = evaluator_for(resolver);

    let decision =
        evaluator.evaluate_permission("unknown_fingerprint", &Permission::AdminCatalogPublish);

    assert!(decision.is_denied());
    match decision {
        PermissionDecision::Denied {
            reason,
            principal_id,
            ..
        } => {
            assert_eq!(reason, DenialReason::PrincipalNotFound);
            assert!(principal_id.is_none());
        },
        _ => panic!("expected denied decision"),
    }
}

#[test]
fn test_permission_evaluation_all_permissions_allowed() {
    let (_resolver, evaluator) =
        resolver_and_evaluator("test_superadmin", PrincipalRole::SuperAdmin);
    let required = vec![
        Permission::AdminCatalogPublish,
        Permission::AdminShutdown,
        Permission::AuditRead,
    ];

    let result = evaluator.evaluate_all_permissions("test_superadmin", &required);

    assert!(result.is_ok());
}

#[test]
fn test_permission_evaluation_all_permissions_denied() {
    let (_resolver, evaluator) = resolver_and_evaluator("test_user", PrincipalRole::User);
    let required = vec![
        Permission::AdminCatalogPublish,
        Permission::ExecuteProcedure(ProcedureId::new(42)),
    ];

    let err = evaluator
        .evaluate_all_permissions("test_user", &required)
        .expect_err("user should not satisfy admin catalog permission");

    match err {
        PermissionDecision::Denied { reason, .. } => {
            assert_eq!(reason, DenialReason::MissingPermission);
        },
        _ => panic!("expected denied decision"),
    }
}

#[test]
fn test_deterministic_authorization_decisions() {
    let (_resolver, evaluator) = resolver_and_evaluator("deterministic", PrincipalRole::Operator);
    let perm = Permission::ExecuteProcedure(ProcedureId::new(42));

    let decisions: Vec<_> = (0..10)
        .map(|_| evaluator.evaluate_permission("deterministic", &perm))
        .collect();

    for decision in &decisions[1..] {
        assert_eq!(decision, &decisions[0]);
    }
}
