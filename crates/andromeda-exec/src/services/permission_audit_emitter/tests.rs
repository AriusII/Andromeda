use super::*;
use andromeda_core::{AndromedaErrorKind, AndromedaResult, Permission, PrincipalId};
use andromeda_observe::{CriticalDecisionKind, TraceId};

#[test]
fn test_audit_event_allowed() {
    let event = PermissionAuditEvent::allowed(
        TraceId::new(1),
        PrincipalId::new(100),
        Permission::ExecuteProcedure(andromeda_core::ProcedureId::new(1)),
    );

    assert!(event.is_allowed());
    assert!(!event.is_denied());
}

#[test]
fn test_audit_event_denied() {
    let event = PermissionAuditEvent::denied(
        TraceId::new(1),
        PrincipalId::new(100),
        Permission::AdminShutdown,
        DenialAuditReason::PermissionNotGranted,
    );

    assert!(!event.is_allowed());
    assert!(event.is_denied());
}

#[test]
fn test_audit_event_denied_unknown_principal() {
    let event = PermissionAuditEvent::denied_unknown_principal(
        TraceId::new(1),
        Permission::AdminCatalogPublish,
    );

    assert!(!event.is_allowed());
    assert!(event.is_denied());
}

#[test]
fn test_denial_reason_explanation() {
    assert!(
        !DenialAuditReason::NoPermissionsGranted
            .explanation()
            .is_empty()
    );
    assert!(
        !DenialAuditReason::PermissionNotGranted
            .explanation()
            .is_empty()
    );
    assert!(
        !DenialAuditReason::ProcedureIdMismatch
            .explanation()
            .is_empty()
    );
    assert!(
        !DenialAuditReason::SuperAdminOperationNotAudited
            .explanation()
            .is_empty()
    );
    assert!(
        !DenialAuditReason::WildcardDeniedByPolicy
            .explanation()
            .is_empty()
    );
}

#[test]
fn test_decision_trace_conversion() {
    let event = PermissionAuditEvent::allowed(
        TraceId::new(100),
        PrincipalId::new(42),
        Permission::ReadContractMetadata,
    );

    let trace = event.to_decision_trace();
    assert_eq!(trace.trace_id, TraceId::new(100));
    assert_eq!(trace.decision, CriticalDecisionKind::SecurityAuthorization);
    assert!(trace.reason.contains("permission allowed"));
}

#[test]
fn test_only_noop_emitter_rejects_direct_emission() {
    let emitter = NoOpPermissionAuditEmitter::new_for_tests();
    let event = PermissionAuditEvent::denied(
        TraceId::new(1),
        PrincipalId::new(100),
        Permission::AdminShutdown,
        DenialAuditReason::PermissionNotGranted,
    );

    let error = emitter
        .emit_permission_decision(event)
        .expect_err("test-only no-op emitter must reject direct emission");
    assert_eq!(error.kind(), AndromedaErrorKind::Security);
    assert!(
        error
            .message()
            .contains("cannot emit audit records directly")
    );
}

#[test]
fn audit_evidence_redacts_sensitive_reason_markers() -> AndromedaResult<()> {
    let evidence = AuditEmissionEvidence::contract_rejected(
        AuditEmissionPolicy::fail_closed(),
        TraceId::new(77),
        "contract mismatch carried token=abc123 in a payload diagnostic",
        AuditSinkAvailability::available(),
    )?;

    assert_eq!(evidence.reason, redact_audit_reason("token=abc123"));
    assert!(!audit_text_contains_sensitive_marker(&evidence.reason));
    Ok(())
}

#[test]
fn audit_evidence_fails_closed_when_durable_sink_is_unavailable() {
    let error = AuditEmissionEvidence::contract_rejected(
        AuditEmissionPolicy::fail_closed(),
        TraceId::new(78),
        "contract rejected before transaction",
        AuditSinkAvailability::unavailable("failed to open audit sink: secret=raw"),
    )
    .expect_err("strict audit policy must fail closed when durable sink is unavailable");

    assert_eq!(error.kind(), AndromedaErrorKind::Security);
    assert!(error.message().contains("durable audit sink unavailable"));
    assert!(!audit_text_contains_sensitive_marker(error.message()));
}

#[test]
fn permission_emitter_consumes_policy_and_returns_evidence() -> AndromedaResult<()> {
    let emitter = NoOpPermissionAuditEmitter::new_for_tests();
    let event = PermissionAuditEvent::denied(
        TraceId::new(79),
        PrincipalId::new(100),
        Permission::AdminShutdown,
        DenialAuditReason::PermissionNotGranted,
    );

    let evidence = emitter.emit_permission_decision_with_policy(
        event,
        AuditEmissionPolicy::fail_closed(),
        AuditSinkAvailability::available(),
    )?;

    assert_eq!(evidence.kind, AuditEmissionKind::PermissionDecision);
    assert_eq!(evidence.outcome, AuditEmissionOutcome::Denied);
    assert!(evidence.sink.is_available());
    Ok(())
}
