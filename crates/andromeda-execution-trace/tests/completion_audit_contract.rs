use andromeda_audit::{
    AuditEmissionEvidence, AuditEmissionKind, AuditEmissionOutcome, AuditEmissionPolicy,
    AuditSinkAvailability, DurableAuditEventFamily, DurableAuditRecordIdentity,
    DurableAuditReplayBehavior, DurableAuditRetentionBoundary, DurableAuditSinkReport,
    DurableAuditWalEvidence, audit_text_contains_sensitive_marker,
};
use andromeda_error::AndromedaErrorKind;
use andromeda_execution_trace::{
    CompletionEmission, CompletionMappingService, InvocationCompletionEmitter,
};
use andromeda_observability::{EventId, TraceId};
use andromeda_result_stream::CompletionStatus;
use andromeda_transaction::TransactionState;
use andromeda_types::{InvocationId, TransactionId};
use andromeda_wal::Lsn;

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

const fn retention_boundary_for_family(
    family: DurableAuditEventFamily,
) -> DurableAuditRetentionBoundary {
    match family {
        DurableAuditEventFamily::CatalogDecision => DurableAuditRetentionBoundary::CatalogVersion,
        DurableAuditEventFamily::BackupDecision | DurableAuditEventFamily::RestoreDecision => {
            DurableAuditRetentionBoundary::WalSegment
        },
        DurableAuditEventFamily::ForensicDecision => DurableAuditRetentionBoundary::ForensicHold,
        _ => DurableAuditRetentionBoundary::SecurityPolicy,
    }
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
            },
            CompletionStatus::RolledBack => {
                CompletionEmission::rolled_back(completion, transaction_id, terminal_lsn)
            },
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
