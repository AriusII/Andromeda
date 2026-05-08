use crate::support::*;

#[test]
fn permission_denied_audit_evidence_is_required_and_fail_closed() {
    let emitter = NoOpPermissionAuditEmitter::new_for_tests();
    let event = PermissionAuditEvent::denied(
        TraceId::new(5001),
        PrincipalId::new(41),
        Permission::AuditRead,
        DenialAuditReason::PermissionNotGranted,
    );

    let unavailable = emitter
        .emit_permission_decision_with_policy(
            event.clone(),
            AuditEmissionPolicy::fail_closed(),
            AuditSinkAvailability::unavailable("audit wal open failed: token=raw-secret"),
        )
        .expect_err("permission denial must fail closed when durable audit is unavailable");

    assert_eq!(unavailable.kind(), AndromedaErrorKind::Security);
    assert!(
        unavailable
            .message()
            .contains("durable audit sink unavailable")
    );
    assert!(!audit_text_contains_sensitive_marker(unavailable.message()));

    let evidence = emitter
        .emit_permission_decision_with_policy(
            event,
            AuditEmissionPolicy::fail_closed(),
            AuditSinkAvailability::available(),
        )
        .expect("permission denial should produce audit evidence before visible denial");

    assert_eq!(evidence.trace_id, TraceId::new(5001));
    assert_eq!(evidence.kind, AuditEmissionKind::PermissionDecision);
    assert_eq!(evidence.outcome, AuditEmissionOutcome::Denied);
    assert!(evidence.sink.is_available());
    assert!(!audit_text_contains_sensitive_marker(&evidence.reason));
}
#[test]
fn permission_allowed_admin_and_security_visible_decisions_require_durable_audit_wal_evidence() {
    let emitter = RecordingPermissionAuditEmitter::default();
    for (offset, family, permission) in [
        (
            0_u128,
            DurableAuditEventFamily::SecurityDecision,
            Permission::AuditRead,
        ),
        (
            1_u128,
            DurableAuditEventFamily::AdminDecision,
            Permission::AdminShutdown,
        ),
    ] {
        let trace_id = TraceId::new(5300 + offset);
        let event = PermissionAuditEvent::allowed(trace_id, PrincipalId::new(70), permission);
        let policy = AuditEmissionPolicy::fail_closed_for_visible_decision(family);

        let missing_wal = emitter
            .emit_permission_decision_with_policy(
                event.clone(),
                policy,
                AuditSinkAvailability::available(),
            )
            .expect_err("visible permission allow requires concrete durable WAL evidence");
        assert_eq!(missing_wal.kind(), AndromedaErrorKind::Security);
        assert!(missing_wal.message().contains("durable audit WAL evidence"));

        let sink = match family {
            DurableAuditEventFamily::SecurityDecision => {
                AuditSinkAvailability::durable_for_security_decision(durable_audit_report(
                    trace_id,
                    family,
                    91 + offset as u64,
                ))
            },
            DurableAuditEventFamily::AdminDecision => {
                AuditSinkAvailability::durable_for_admin_decision(durable_audit_report(
                    trace_id,
                    family,
                    91 + offset as u64,
                ))
            },
            _ => unreachable!("test covers security/admin visible decisions"),
        }
        .expect("typed durable audit report should become sink availability");

        let evidence = emitter
            .emit_permission_decision_with_policy(event, policy, sink)
            .expect("durable audit report should allow visible permission decision");

        assert_eq!(evidence.kind, AuditEmissionKind::PermissionDecision);
        assert_eq!(evidence.outcome, AuditEmissionOutcome::Allowed);
        assert_eq!(
            evidence
                .sink
                .durability()
                .expect("durability evidence")
                .family,
            family
        );
    }
}

#[test]
fn test_only_noop_permission_audit_emitter_rejects_visible_decision_policy() {
    let emitter = NoOpPermissionAuditEmitter::new_for_tests();
    let trace_id = TraceId::new(5306);
    let event =
        PermissionAuditEvent::allowed(trace_id, PrincipalId::new(74), Permission::AdminShutdown);
    let policy = AuditEmissionPolicy::fail_closed_for_visible_decision(
        DurableAuditEventFamily::AdminDecision,
    );
    let sink = AuditSinkAvailability::durable_for_admin_decision(durable_audit_report(
        trace_id,
        DurableAuditEventFamily::AdminDecision,
        99,
    ))
    .expect("valid admin report should become sink availability");

    let error = emitter
        .emit_permission_decision_with_policy(event, policy, sink)
        .expect_err("test-only no-op emitter must not satisfy strict visible-decision audit");

    assert_eq!(error.kind(), AndromedaErrorKind::Security);
    assert!(error.message().contains("test-only no-op"));
}

#[test]
fn permission_allowed_visible_decision_rejects_audit_append_failure() {
    struct FailingAppendPermissionAuditEmitter;

    impl PermissionAuditEmitter for FailingAppendPermissionAuditEmitter {
        fn emit_permission_decision(&self, _event: PermissionAuditEvent) -> AndromedaResult<()> {
            Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "durable audit append failed before visible permission decision",
            ))
        }
    }

    let emitter = FailingAppendPermissionAuditEmitter;
    let event = PermissionAuditEvent::allowed(
        TraceId::new(5305),
        PrincipalId::new(73),
        Permission::AdminShutdown,
    );
    let policy = AuditEmissionPolicy::fail_closed_for_visible_decision(
        DurableAuditEventFamily::AdminDecision,
    );
    let sink = AuditSinkAvailability::durable_for_admin_decision(durable_audit_report(
        TraceId::new(5305),
        DurableAuditEventFamily::AdminDecision,
        98,
    ))
    .expect("valid admin report should become sink availability");

    let error = emitter
        .emit_permission_decision_with_policy(event, policy, sink)
        .expect_err("audit append failure must reject the visible allow decision");

    assert_eq!(error.kind(), AndromedaErrorKind::Security);
    assert!(error.message().contains("durable audit append failed"));
}
#[test]
fn permission_denied_visible_decision_requires_durable_audit_wal_evidence() {
    let event = PermissionAuditEvent::denied(
        TraceId::new(5302),
        PrincipalId::new(72),
        Permission::AdminCatalogPublish,
        DenialAuditReason::PermissionNotGranted,
    );
    let policy = AuditEmissionPolicy::fail_closed_for_visible_decision(
        DurableAuditEventFamily::SecurityDecision,
    );

    let missing_wal = event
        .audit_evidence(policy, AuditSinkAvailability::available())
        .expect_err("visible denied decision requires concrete WAL evidence");
    assert_eq!(missing_wal.kind(), AndromedaErrorKind::Security);
    assert!(missing_wal.message().contains("durable audit WAL evidence"));

    let evidence = event
        .audit_evidence(
            policy,
            AuditSinkAvailability::durable_for_security_decision(durable_audit_report(
                TraceId::new(5302),
                DurableAuditEventFamily::SecurityDecision,
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
        DurableAuditEventFamily::SecurityDecision
    );
}
#[test]
fn admin_decision_fails_closed_when_audit_wal_family_is_wrong() {
    let event = PermissionAuditEvent::allowed(
        TraceId::new(5301),
        PrincipalId::new(71),
        Permission::AdminShutdown,
    );
    let policy = AuditEmissionPolicy::fail_closed_for_visible_decision(
        DurableAuditEventFamily::AdminDecision,
    );

    let error = event
        .audit_evidence(
            policy,
            AuditSinkAvailability::durable(durable_audit_report(
                TraceId::new(5301),
                DurableAuditEventFamily::SecurityDecision,
                92,
            ))
            .expect("valid but wrong-family report"),
        )
        .expect_err("admin-visible decision must reject non-admin audit WAL evidence");

    assert_eq!(error.kind(), AndromedaErrorKind::Security);
    assert!(error.message().contains("family mismatch"));
}
#[test]
fn visible_decision_rejects_durable_audit_report_with_wrong_trace_id() {
    let event = PermissionAuditEvent::allowed(
        TraceId::new(5501),
        PrincipalId::new(81),
        Permission::AuditRead,
    );
    let policy = AuditEmissionPolicy::fail_closed_for_visible_decision(
        DurableAuditEventFamily::SecurityDecision,
    );
    let sink = AuditSinkAvailability::durable_for_security_decision(durable_audit_report(
        TraceId::new(5502),
        DurableAuditEventFamily::SecurityDecision,
        120,
    ))
    .expect("wrong-trace report is otherwise durable security evidence");

    let error = event
        .audit_evidence(policy, sink)
        .expect_err("visible decision audit WAL evidence must match the decision trace");

    assert_eq!(error.kind(), AndromedaErrorKind::Security);
    assert!(error.message().contains("trace id mismatch"));
    assert!(!audit_text_contains_sensitive_marker(error.message()));
}
#[test]
fn visible_decision_rejects_non_durable_audit_report() {
    let mut report = durable_audit_report(
        TraceId::new(5302),
        DurableAuditEventFamily::SecurityDecision,
        93,
    );
    report.evidence.durable_lsn = report.evidence.record_lsn - 1;

    let error = AuditSinkAvailability::durable(report)
        .expect_err("sink availability must reject report before durable LSN catches up");

    assert_eq!(error.kind(), AndromedaErrorKind::Internal);
}

#[test]
fn default_audit_policy_is_fail_closed_not_test_support() {
    let default_policy = AuditEmissionPolicy::default();
    let test_support_policy =
        AuditEmissionPolicy::allow_unavailable_sink_for_explicit_test_support();

    assert_eq!(default_policy, AuditEmissionPolicy::fail_closed());
    assert_ne!(default_policy, test_support_policy);
    assert!(default_policy.requires_available_sink());
    assert!(!default_policy.requires_durable_wal_evidence());
    assert!(!test_support_policy.requires_available_sink());
    assert!(!test_support_policy.requires_durable_wal_evidence());
    assert_eq!(test_support_policy.expected_event_family(), None);
}
