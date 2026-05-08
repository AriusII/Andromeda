use crate::support::{evaluator_for, new_resolver, resolver_and_evaluator};
use andromeda_core::{Permission, PrincipalRole, ProcedureId};
use andromeda_exec::services::{
    DenialReason, PermissionDecision, PermissionEvaluator, PrincipalResolver,
};
use std::{sync::Arc, thread};

#[test]
fn test_edge_case_anonymous_principal() {
    let resolver = new_resolver();
    let evaluator = evaluator_for(resolver);

    let decision = evaluator.evaluate_permission(
        "anonymous",
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );

    assert!(decision.is_denied());
    match decision {
        PermissionDecision::Denied { reason, .. } => {
            assert_eq!(reason, DenialReason::PrincipalNotFound);
        },
        _ => panic!("expected denied decision"),
    }
}

#[test]
fn test_edge_case_revoked_principal() {
    let fingerprint = "revoked_principal";
    let (resolver, evaluator) = resolver_and_evaluator(fingerprint, PrincipalRole::Admin);

    let decision_before =
        evaluator.evaluate_permission(fingerprint, &Permission::AdminCatalogPublish);
    assert!(decision_before.is_allowed());

    resolver
        .revoke_principal(fingerprint)
        .expect("revocation should succeed");

    let decision_after =
        evaluator.evaluate_permission(fingerprint, &Permission::AdminCatalogPublish);
    assert!(decision_after.is_denied());
}

#[test]
fn test_edge_case_super_admin_wildcard_permission() {
    let (_resolver, evaluator) =
        resolver_and_evaluator("test_superadmin", PrincipalRole::SuperAdmin);

    let decision1 = evaluator.evaluate_permission(
        "test_superadmin",
        &Permission::ExecuteProcedure(ProcedureId::new(1)),
    );
    let decision2 = evaluator.evaluate_permission(
        "test_superadmin",
        &Permission::ExecuteProcedure(ProcedureId::new(999)),
    );

    assert!(decision1.is_allowed());
    assert!(decision2.is_allowed());
}

#[test]
fn test_edge_case_guest_procedure_restriction() {
    let (_resolver, evaluator) = resolver_and_evaluator("test_guest", PrincipalRole::Guest);

    let public = evaluator.evaluate_permission(
        "test_guest",
        &Permission::ExecuteProcedure(ProcedureId::new(0)),
    );
    let private = evaluator.evaluate_permission(
        "test_guest",
        &Permission::ExecuteProcedure(ProcedureId::new(1)),
    );

    assert!(public.is_allowed());
    assert!(private.is_denied());
}

#[test]
fn test_concurrent_principal_resolution() {
    let resolver = new_resolver();

    for i in 0..5 {
        let fingerprint = format!("concurrent_{}", i);
        resolver
            .register_principal(fingerprint, PrincipalRole::User)
            .expect("registration should succeed");
    }

    let mut handles = vec![];

    for i in 0..10 {
        let resolver_clone = Arc::clone(&resolver);
        let handle = thread::spawn(move || {
            let fingerprint = format!("concurrent_{}", i % 5);
            let principal = resolver_clone
                .resolve(&fingerprint)
                .expect("registered concurrent principal should resolve");
            assert_eq!(principal.role, PrincipalRole::User);
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("thread should complete");
    }
}
