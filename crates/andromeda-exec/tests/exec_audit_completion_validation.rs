use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, InvocationId, Permission, PrincipalId,
    TransactionId,
};
use andromeda_exec::dispatch::permission_validation::{
    validate_dispatch_permissions_with_audit, validate_dispatch_permissions_with_durable_audit,
};
use andromeda_exec::services::permission_audit_emitter::{
    AuditEmissionEvidence, AuditEmissionKind, AuditEmissionOutcome, AuditEmissionPolicy,
    AuditSinkAvailability, DenialAuditReason, NoOpPermissionAuditEmitter, PermissionAuditEmitter,
    PermissionAuditEvent, audit_text_contains_sensitive_marker,
};
use andromeda_exec::{
    CompletionEmission, CompletionMappingService, CompletionStatus, InvocationCompletionEmitter,
};
use andromeda_observe::{
    DurableAuditEventFamily, DurableAuditRecordIdentity, DurableAuditReplayBehavior,
    DurableAuditRetentionBoundary, DurableAuditSinkReport, DurableAuditWalEvidence, EventId,
    TraceId,
};
use andromeda_storage::Lsn;
use andromeda_tx::TransactionState;

#[test]
fn permission_denied_audit_evidence_is_required_and_fail_closed() {
    let emitter = NoOpPermissionAuditEmitter;
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
fn permission_denied_and_contract_rejected_completions_require_completion_audit_evidence() {
    for (offset, status) in [
        (0_u128, CompletionStatus::PermissionDenied),
        (1_u128, CompletionStatus::ContractRejected),
    ] {
        let completion = CompletionMappingService::rejected(
            InvocationId::new(5100 + offset as u64),
            status,
            TraceId::new(5100 + offset),
        )
        .expect("rejected completion has no transaction evidence");
        let emission = CompletionEmission::pre_transaction(completion);
        let audit = emission
            .audit_evidence(
                AuditEmissionPolicy::fail_closed(),
                AuditSinkAvailability::available(),
            )
            .expect("completion must project audit evidence");
        let mut emitter = InvocationCompletionEmitter::new();

        let record = emitter
            .emit_with_audit(emission, audit)
            .expect("completion should emit only after audit evidence is available");

        assert_eq!(record.status, status);
        assert_eq!(record.transaction_id, None);
        assert_eq!(record.terminal_lsn, None);
        assert_eq!(record.durable_lsn, None);
    }
}

#[test]
fn completion_emission_rejects_wrong_audit_kind_and_unavailable_sink() {
    let completion = CompletionMappingService::rejected(
        InvocationId::new(5200),
        CompletionStatus::ContractRejected,
        TraceId::new(5200),
    )
    .expect("contract rejection completion has no transaction evidence");
    let emission = CompletionEmission::pre_transaction(completion);
    let wrong_audit = AuditEmissionEvidence::contract_rejected(
        AuditEmissionPolicy::fail_closed(),
        TraceId::new(5200),
        "contract rejected before completion",
        AuditSinkAvailability::available(),
    )
    .expect("available audit sink should produce contract evidence");
    let mut emitter = InvocationCompletionEmitter::new();

    let wrong_kind = emitter
        .emit_with_audit(emission, wrong_audit)
        .expect_err("completion emission must require completion audit evidence");
    assert!(wrong_kind.message().contains("completion audit evidence"));
    assert!(emitter.journal().is_empty());

    let unavailable = emission
        .audit_evidence(
            AuditEmissionPolicy::fail_closed(),
            AuditSinkAvailability::unavailable("durable audit flush failed: secret=raw"),
        )
        .expect_err("completion audit evidence must fail closed without durable audit");
    assert_eq!(unavailable.kind(), AndromedaErrorKind::Security);
    assert!(!audit_text_contains_sensitive_marker(unavailable.message()));
}

#[test]
fn completion_audit_projection_requires_durable_terminal_lsn_ordering() {
    let completion = CompletionMappingService::committed(
        InvocationId::new(5201),
        1,
        TransactionState::Committed,
        Lsn::new(8),
        TraceId::new(5201),
    )
    .expect("committed completion may be built with durable WAL evidence");
    let emission =
        CompletionEmission::committed(completion, TransactionId::new(6201), Lsn::new(9), Some(1));

    let error = emission
        .audit_evidence(
            AuditEmissionPolicy::fail_closed(),
            AuditSinkAvailability::available(),
        )
        .expect_err("completion audit evidence must not precede terminal WAL coverage");

    assert_eq!(error.kind(), AndromedaErrorKind::Execution);
    assert!(
        error
            .message()
            .contains("durable LSN must cover terminal WAL LSN")
    );
    assert!(!audit_text_contains_sensitive_marker(error.message()));
}

#[test]
fn permission_allowed_admin_and_security_visible_decisions_require_durable_audit_wal_evidence() {
    let emitter = NoOpPermissionAuditEmitter;
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
            }
            DurableAuditEventFamily::AdminDecision => {
                AuditSinkAvailability::durable_for_admin_decision(durable_audit_report(
                    trace_id,
                    family,
                    91 + offset as u64,
                ))
            }
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

#[test]
fn completion_visible_outcome_carries_durable_audit_evidence_under_strict_policy() {
    let completion = CompletionMappingService::rejected(
        InvocationId::new(5400),
        CompletionStatus::ContractRejected,
        TraceId::new(5400),
    )
    .expect("contract rejection completion has no transaction evidence");
    let emission = CompletionEmission::pre_transaction(completion);
    let policy = AuditEmissionPolicy::fail_closed_for_visible_decision(
        DurableAuditEventFamily::CatalogDecision,
    );

    let missing_wal = emission
        .audit_evidence(policy, AuditSinkAvailability::available())
        .expect_err("strict visible completion requires durable WAL audit evidence");
    assert_eq!(missing_wal.kind(), AndromedaErrorKind::Security);
    assert!(missing_wal.message().contains("durable audit WAL evidence"));

    let audit = emission
        .audit_evidence_from_durable_report(
            policy,
            durable_audit_report(
                TraceId::new(5400),
                DurableAuditEventFamily::CatalogDecision,
                96,
            ),
        )
        .expect("strict visible completion should carry durable audit proof");
    assert_eq!(
        audit.sink.durability().expect("durability evidence").family,
        DurableAuditEventFamily::CatalogDecision
    );

    let mut emitter = InvocationCompletionEmitter::new();
    let record = emitter
        .emit_with_durable_audit_report(
            emission,
            policy,
            durable_audit_report(
                TraceId::new(5400),
                DurableAuditEventFamily::CatalogDecision,
                97,
            ),
        )
        .expect("completion emission should accept durable audit report under strict policy");

    assert_eq!(record.status, CompletionStatus::ContractRejected);
    assert_eq!(record.transaction_id, None);
}

#[test]
fn committed_and_rolled_back_completions_require_durable_completion_audit_evidence() {
    let policy = AuditEmissionPolicy::fail_closed_with_durable_wal();

    for (offset, status) in [
        (0_u128, CompletionStatus::Committed),
        (1_u128, CompletionStatus::RolledBack),
    ] {
        let trace_id = TraceId::new(5450 + offset);
        let invocation_id = InvocationId::new(5450 + offset as u64);
        let transaction_id = TransactionId::new(6450 + offset as u64);
        let terminal_lsn = Lsn::new(70 + offset as u64);
        let durable_lsn = Lsn::new(80 + offset as u64);
        let completion = match status {
            CompletionStatus::Committed => CompletionMappingService::committed(
                invocation_id,
                4,
                TransactionState::Committed,
                durable_lsn,
                trace_id,
            )
            .expect("committed completion has durable WAL evidence"),
            CompletionStatus::RolledBack => CompletionMappingService::rolled_back(
                invocation_id,
                TransactionState::RolledBack,
                durable_lsn,
                trace_id,
            )
            .expect("rolled-back completion has durable WAL evidence"),
            _ => unreachable!("test covers transactional terminal completions"),
        };
        let emission = match status {
            CompletionStatus::Committed => {
                CompletionEmission::committed(completion, transaction_id, terminal_lsn, Some(4))
            }
            CompletionStatus::RolledBack => {
                CompletionEmission::rolled_back(completion, transaction_id, terminal_lsn)
            }
            _ => unreachable!("test covers transactional terminal completions"),
        };

        let missing_wal = emission
            .audit_evidence(policy, AuditSinkAvailability::available())
            .expect_err("transactional terminal completion requires durable audit WAL evidence");
        assert_eq!(missing_wal.kind(), AndromedaErrorKind::Security);
        assert!(missing_wal.message().contains("durable audit WAL evidence"));
        assert!(!audit_text_contains_sensitive_marker(missing_wal.message()));

        let audit = emission
            .audit_evidence_from_durable_report(
                policy,
                durable_audit_report(
                    trace_id,
                    DurableAuditEventFamily::GenericAudit,
                    110 + offset as u64,
                ),
            )
            .expect("durable audit report should prove completion audit emission");
        assert_eq!(audit.kind, AuditEmissionKind::Completion);
        assert_eq!(audit.outcome, AuditEmissionOutcome::Emitted);
        assert_eq!(audit.trace_id, trace_id);
        assert!(!audit_text_contains_sensitive_marker(&audit.reason));
        assert!(
            audit
                .sink
                .durability()
                .expect("durability proof")
                .proves_durable()
        );

        let mut emitter = InvocationCompletionEmitter::new();
        let record = emitter
            .emit_with_audit(emission, audit)
            .expect("completion emits only after durable audit evidence is available");

        assert_eq!(record.status, status);
        assert_eq!(record.transaction_id, Some(transaction_id));
        assert_eq!(record.terminal_lsn, Some(terminal_lsn));
        assert_eq!(record.durable_lsn, Some(durable_lsn));
    }
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

fn durable_audit_report(
    trace_id: TraceId,
    family: DurableAuditEventFamily,
    record_lsn: u64,
) -> DurableAuditSinkReport {
    DurableAuditSinkReport {
        identity: DurableAuditRecordIdentity {
            event_id: EventId::new(100_000 + record_lsn as u128),
            trace_id,
            family,
            sequence_number: record_lsn,
        },
        evidence: DurableAuditWalEvidence {
            record_lsn,
            durable_lsn: record_lsn,
            checksum: 0xABCD_0000 + record_lsn,
        },
        replay_behavior: DurableAuditReplayBehavior::RebuildDecisionIndex,
        retention: retention_boundary_for_family(family),
    }
}

fn retention_boundary_for_family(family: DurableAuditEventFamily) -> DurableAuditRetentionBoundary {
    match family {
        DurableAuditEventFamily::CatalogDecision => DurableAuditRetentionBoundary::CatalogVersion,
        DurableAuditEventFamily::BackupDecision | DurableAuditEventFamily::RestoreDecision => {
            DurableAuditRetentionBoundary::WalSegment
        }
        DurableAuditEventFamily::ForensicDecision => DurableAuditRetentionBoundary::ForensicHold,
        _ => DurableAuditRetentionBoundary::SecurityPolicy,
    }
}
