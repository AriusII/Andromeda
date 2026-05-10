use crate::support::{evaluator_for, resolver_and_evaluator, resolver_with_principal};
use andromeda_admission::AdmissionService;
use andromeda_iam::{PermissionEvaluator, PrincipalResolver};
use andromeda_observability::TraceId;
use andromeda_principal::{Permission, PrincipalRole};
use andromeda_types::ProcedureId;
use std::sync::Arc;

#[test]
fn test_audit_event_principal_binding() {
    let resolver = resolver_with_principal("test_audit", PrincipalRole::Operator);

    let principal = resolver
        .resolve("test_audit")
        .expect("registered principal should resolve");

    assert_eq!(principal.cert_fingerprint.as_str(), "test_audit");
    assert_eq!(principal.role, PrincipalRole::Operator);
    assert!(!principal.session_token.as_str().is_empty());
}

#[test]
fn test_audit_event_traceability() {
    let (_resolver, evaluator) = resolver_and_evaluator("test_trace", PrincipalRole::User);

    let decision1 = evaluator.evaluate_permission(
        "test_trace",
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );
    let decision2 = evaluator.evaluate_permission(
        "test_trace",
        &Permission::ExecuteProcedure(ProcedureId::new(42)),
    );

    assert_eq!(decision1, decision2);
}

#[test]
fn test_admission_integration_permission_allowed() {
    let resolver = resolver_with_principal("test_admission", PrincipalRole::Admin);
    let evaluator = Arc::new(evaluator_for(resolver));
    let admission = AdmissionService::new(evaluator);

    let result = admission.evaluate_permission(
        "test_admission",
        &Permission::AdminCatalogPublish,
        ProcedureId::new(1),
        TraceId::new(1),
    );

    assert!(result.is_ok());
}

#[test]
fn test_admission_integration_permission_denied() {
    let resolver = resolver_with_principal("test_admission_user", PrincipalRole::User);
    let evaluator = Arc::new(evaluator_for(resolver));
    let admission = AdmissionService::new(evaluator);

    let err = admission
        .evaluate_permission(
            "test_admission_user",
            &Permission::AdminShutdown,
            ProcedureId::new(1),
            TraceId::new(1),
        )
        .expect_err("user should not satisfy admin shutdown permission");

    assert!(err.reason.contains("access denied"));
}
