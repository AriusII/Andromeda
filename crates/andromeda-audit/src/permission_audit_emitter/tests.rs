use super::*;
use andromeda_core::{AndromedaErrorKind, AndromedaResult, Permission, PrincipalId};
use andromeda_observability::TraceId;

fn durable_report(
    trace_id: TraceId,
    family: AuditEmissionEventFamily,
    record_lsn: u64,
) -> AuditSinkDurabilityReport {
    AuditSinkDurabilityReport {
        trace_id,
        family,
        sequence_number: record_lsn,
        record_lsn,
        durable_lsn: record_lsn,
        checksum: 0xABCD_0000 + record_lsn,
        replay_behavior: AuditEmissionReplayBehavior::RebuildDecisionIndex,
        retention: match family {
            AuditEmissionEventFamily::CatalogDecision => {
                AuditEmissionRetentionBoundary::CatalogVersion
            }
            AuditEmissionEventFamily::BackupDecision
            | AuditEmissionEventFamily::RestoreDecision => {
                AuditEmissionRetentionBoundary::WalSegment
            }
            AuditEmissionEventFamily::ForensicDecision => {
                AuditEmissionRetentionBoundary::ForensicHold
            }
            _ => AuditEmissionRetentionBoundary::SecurityPolicy,
        },
    }
}

#[test]
fn audit_event_allowed() {
    let event = PermissionAuditEvent::allowed(
        TraceId::new(1),
        PrincipalId::new(100),
        Permission::ExecuteProcedure(andromeda_core::ProcedureId::new(1)),
    );

    assert!(event.is_allowed());
    assert!(!event.is_denied());
}

#[test]
fn audit_event_denied() {
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
fn audit_event_denied_unknown_principal() {
    let event = PermissionAuditEvent::denied_unknown_principal(
        TraceId::new(1),
        Permission::AdminCatalogPublish,
    );

    assert!(!event.is_allowed());
    assert!(event.is_denied());
}

#[test]
fn denial_reason_uses_permission_denial_vocabulary() {
    assert_eq!(
        DenialAuditReason::PermissionNotGranted.as_str(),
        "missing_permission"
    );
    assert_eq!(
        DenialAuditReason::NoPermissionsGranted.as_str(),
        "no_permissions_granted"
    );
    assert!(
        !DenialAuditReason::ProcedureIdMismatch
            .explanation()
            .is_empty()
    );
}

#[test]
fn decision_trace_conversion_preserves_reason_evidence() {
    let event = PermissionAuditEvent::allowed(
        TraceId::new(100),
        PrincipalId::new(42),
        Permission::ReadContractMetadata,
    );

    let trace = event.to_decision_trace();
    assert_eq!(trace.trace_id, TraceId::new(100));
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
fn visible_permission_decision_requires_durable_audit_evidence() {
    let event = PermissionAuditEvent::denied(
        TraceId::new(5302),
        PrincipalId::new(72),
        Permission::AdminCatalogPublish,
        DenialAuditReason::PermissionNotGranted,
    );
    let policy = AuditEmissionPolicy::fail_closed_for_visible_decision(
        AuditEmissionEventFamily::SecurityDecision,
    );

    let missing_wal = event
        .audit_evidence(policy, AuditSinkAvailability::available())
        .expect_err("visible denied decision requires concrete WAL evidence");
    assert_eq!(missing_wal.kind(), AndromedaErrorKind::Security);
    assert!(missing_wal.message().contains("durable audit WAL evidence"));

    let evidence = event
        .audit_evidence(
            policy,
            AuditSinkAvailability::durable_for_security_decision(durable_report(
                TraceId::new(5302),
                AuditEmissionEventFamily::SecurityDecision,
                93,
            ))
            .expect("valid security report should become sink availability"),
        )
        .expect("durable WAL evidence should allow visible permission denial");

    assert_eq!(evidence.kind, AuditEmissionKind::PermissionDecision);
    assert_eq!(evidence.outcome, AuditEmissionOutcome::Denied);
    assert_eq!(
        evidence
            .sink
            .durability()
            .expect("durability evidence")
            .family,
        AuditEmissionEventFamily::SecurityDecision
    );
}
