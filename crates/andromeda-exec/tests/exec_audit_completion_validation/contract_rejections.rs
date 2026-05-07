use crate::support::*;

#[test]
fn contract_rejection_validation_produces_redacted_audit_evidence_before_error() {
    let request_permissions = vec!["admin.global token=raw-secret".to_string()];
    let handler_permissions = vec!["inventory.query".to_string()];

    let (validation, evidence) = validate_dispatch_permissions_with_audit(
        &request_permissions,
        &handler_permissions,
        TraceId::new(5002),
        AuditEmissionPolicy::fail_closed(),
        AuditSinkAvailability::available(),
    )
    .expect("available audit sink should let validation project denial evidence");

    assert!(validation.is_denied());
    let evidence = evidence.expect("denied validation must produce contract audit evidence");
    assert_eq!(evidence.kind, AuditEmissionKind::ContractRejection);
    assert_eq!(evidence.outcome, AuditEmissionOutcome::Rejected);
    assert_eq!(evidence.trace_id, TraceId::new(5002));
    assert!(!audit_text_contains_sensitive_marker(&evidence.reason));

    let unavailable = validate_dispatch_permissions_with_audit(
        &request_permissions,
        &handler_permissions,
        TraceId::new(5003),
        AuditEmissionPolicy::fail_closed(),
        AuditSinkAvailability::unavailable("audit sink password=raw-secret"),
    )
    .expect_err("contract rejection must fail closed when durable audit is unavailable");

    assert_eq!(unavailable.kind(), AndromedaErrorKind::Security);
    assert!(!audit_text_contains_sensitive_marker(unavailable.message()));
}
#[test]
fn contract_rejected_visible_decision_rejects_wrong_durable_audit_family() {
    let request_permissions = vec!["catalog.publish.admin".to_string()];
    let handler_permissions = vec!["catalog.read".to_string()];
    let policy = AuditEmissionPolicy::fail_closed_for_visible_decision(
        DurableAuditEventFamily::CatalogDecision,
    );

    let error = validate_dispatch_permissions_with_durable_audit(
        &request_permissions,
        &handler_permissions,
        TraceId::new(5303),
        policy,
        durable_audit_report(
            TraceId::new(5303),
            DurableAuditEventFamily::SecurityDecision,
            94,
        ),
    )
    .expect_err("contract rejection must reject wrong-family durable audit proof");

    assert_eq!(error.kind(), AndromedaErrorKind::Security);
    assert!(error.message().contains("family mismatch"));

    let (validation, evidence) = validate_dispatch_permissions_with_durable_audit(
        &request_permissions,
        &handler_permissions,
        TraceId::new(5304),
        policy,
        durable_audit_report(
            TraceId::new(5304),
            DurableAuditEventFamily::CatalogDecision,
            95,
        ),
    )
    .expect("catalog durable proof should allow visible contract rejection");

    assert!(validation.is_denied());
    let evidence = evidence.expect("denied validation must emit contract rejection evidence");
    assert_eq!(evidence.kind, AuditEmissionKind::ContractRejection);
    assert_eq!(evidence.outcome, AuditEmissionOutcome::Rejected);
    assert_eq!(
        evidence
            .sink
            .durability()
            .expect("durability evidence")
            .family,
        DurableAuditEventFamily::CatalogDecision
    );
}
