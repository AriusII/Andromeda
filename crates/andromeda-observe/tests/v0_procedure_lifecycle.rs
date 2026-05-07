use andromeda_core::{
    CatalogObjectId, CatalogVersion, ContractHash, RequestId, ResourceBudget, SessionId,
    TransactionId,
};
use andromeda_observe::{
    AuthorizationDeniedTrace, CertificateIdentity, CompletionEmittedTrace, CriticalDecisionKind,
    DecisionTrace, EventCorrelation, EventEnvelope, EventId, InMemoryEventSequence,
    IoBudgetDecisionTrace, IoPipelineStage, Permission, ProtocolCorrelation, RecoveryTrace,
    SecurityAuditOutcome, SecurityAuditTrace, SecurityPolicyVersionEvidence, SurfaceScope,
    TraceEvent, TraceId, UserPrincipal, UserPrincipalKind, WalEventTrace, WalOperation,
};

fn base_correlation() -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(11)),
        session_id: Some(SessionId::new(12)),
        contract_hash: Some(ContractHash::test_vector(13)),
        catalog_version: Some(CatalogVersion::new(14)),
        catalog_object_id: Some(CatalogObjectId::new(15)),
        transaction_id: None,
        durable_lsn: None,
        protocol: Some(ProtocolCorrelation {
            protocol_version: Some(1),
            stream_id: Some(16),
            stream_role: Some(2),
            frame_type: Some(3),
            payload_kind: Some(4),
            sequence: Some(17),
        }),
    }
}

fn tx_correlation(transaction_id: TransactionId, durable_lsn: u64) -> EventCorrelation {
    EventCorrelation {
        transaction_id: Some(transaction_id),
        durable_lsn: Some(durable_lsn),
        ..base_correlation()
    }
}

fn certificate(surface: SurfaceScope) -> CertificateIdentity {
    CertificateIdentity::new("sha256:v0-lifecycle-cert", "CN=v0-worker", surface)
        .expect("test certificate identity is explicit")
}

fn principal() -> UserPrincipal {
    UserPrincipal::new("svc:v0-worker", UserPrincipalKind::Service)
        .expect("test principal identity is explicit")
}

fn admission(event_id: u128, trace_id: u128) -> EventEnvelope {
    EventEnvelope::new(
        EventId::new(event_id),
        base_correlation(),
        TraceEvent::Decision(DecisionTrace {
            trace_id: TraceId::new(trace_id),
            decision: CriticalDecisionKind::ContractValidation,
            reason: "contract hash matched the published V0 procedure contract".to_string(),
        }),
    )
    .expect("admission evidence is pre-transaction and correlated")
}

fn security_audit(
    event_id: u128,
    trace_id: u128,
    outcome: SecurityAuditOutcome,
    reason: &'static str,
) -> EventEnvelope {
    EventEnvelope::new(
        EventId::new(event_id),
        base_correlation(),
        TraceEvent::SecurityAudit(
            SecurityAuditTrace::new(
                TraceId::new(trace_id),
                SurfaceScope::Application,
                certificate(SurfaceScope::Application),
                principal(),
                Permission::ExecuteProcedure,
                outcome,
                reason,
            )
            .expect("security audit requires typed identity, permission, and reason evidence"),
        ),
    )
    .expect("security audit event is valid V0 IAM evidence")
}

fn completion(
    event_id: u128,
    trace_id: u128,
    committed: bool,
    durable_lsn: Option<u64>,
    correlation: EventCorrelation,
) -> EventEnvelope {
    EventEnvelope::new(
        EventId::new(event_id),
        correlation,
        TraceEvent::CompletionEmitted(CompletionEmittedTrace {
            trace_id: TraceId::new(trace_id),
            protocol: ProtocolCorrelation::empty(),
            completion_code: Some(if committed { 0 } else { 1 }),
            committed,
            durable_lsn,
            reason: "terminal completion emitted after V0 lifecycle evidence".to_string(),
        }),
    )
    .expect("completion evidence is request/session correlated")
}

#[test]
fn v0_procedure_lifecycle_records_audit_wal_commit_completion_and_recovery() {
    let transaction_id = TransactionId::new(21);
    let durable_lsn = 900;
    let mut sequence = InMemoryEventSequence::new();

    sequence.append(admission(1, 101)).unwrap();
    sequence
        .append(security_audit(
            2,
            102,
            SecurityAuditOutcome::Allowed,
            "execute procedure permission was audited before transaction begin",
        ))
        .unwrap();
    assert!(sequence.has_mandatory_security_audit());

    sequence
        .append(
            EventEnvelope::new(
                EventId::new(3),
                base_correlation(),
                TraceEvent::IoBudgetDecision(
                    IoBudgetDecisionTrace::from_budget_request(
                        TraceId::new(103),
                        andromeda_core::PipelineClass::ForegroundExecution,
                        IoPipelineStage::Hot,
                        ResourceBudget::new(16 * 1024, 4 * 1024, 1),
                        4 * 1024,
                        0,
                        1,
                        "foreground procedure IO budget was accepted before transaction begin",
                    )
                    .expect("IO budget helper requires explicit reason evidence"),
                ),
            )
            .expect("IO budget decision carries accepted budget evidence"),
        )
        .unwrap();

    sequence
        .append(
            EventEnvelope::new(
                EventId::new(4),
                tx_correlation(transaction_id, durable_lsn),
                TraceEvent::WalEvent(WalEventTrace {
                    trace_id: TraceId::new(104),
                    transaction_id: Some(transaction_id),
                    operation: WalOperation::Flush,
                    appended_lsn: durable_lsn,
                    durable_lsn: Some(durable_lsn),
                }),
            )
            .expect("WAL flush introduces durable transaction evidence"),
        )
        .unwrap();

    sequence
        .append(
            EventEnvelope::new(
                EventId::new(5),
                tx_correlation(transaction_id, durable_lsn),
                TraceEvent::CommitVisible(andromeda_observe::CommitVisibleTrace {
                    trace_id: TraceId::new(105),
                    transaction_id,
                    durable_commit_lsn: durable_lsn,
                }),
            )
            .expect("commit visibility points at the durable commit LSN"),
        )
        .unwrap();

    sequence
        .append(completion(
            6,
            106,
            true,
            Some(durable_lsn),
            tx_correlation(transaction_id, durable_lsn),
        ))
        .unwrap();
    assert!(sequence.has_terminal_completion());

    sequence
        .append(
            EventEnvelope::new(
                EventId::new(7),
                EventCorrelation {
                    durable_lsn: Some(durable_lsn),
                    ..base_correlation()
                },
                TraceEvent::RecoveryStartup(RecoveryTrace {
                    trace_id: TraceId::new(107),
                    last_durable_lsn: durable_lsn,
                    corruption_boundary_lsn: None,
                }),
            )
            .expect("recovery startup is correlated to the durable replay boundary"),
        )
        .unwrap();

    assert!(sequence.is_recovery_started());
    assert_eq!(sequence.events().len(), 7);
}

#[test]
fn pretransaction_security_rejection_requires_audit_and_never_tx_evidence() {
    let mut sequence = InMemoryEventSequence::new();
    sequence.append(admission(1, 201)).unwrap();
    sequence
        .append(security_audit(
            2,
            202,
            SecurityAuditOutcome::Denied,
            "execute procedure permission was denied before transaction begin",
        ))
        .unwrap();
    sequence
        .append(completion(3, 203, false, None, base_correlation()))
        .unwrap();

    assert!(sequence.has_mandatory_security_audit());
    assert!(sequence.has_terminal_pre_transaction_rejection());
    assert!(sequence.has_terminal_completion());

    let mut tx_leak = base_correlation();
    tx_leak.transaction_id = Some(TransactionId::new(99));
    let denied_with_tx = EventEnvelope::new(
        EventId::new(4),
        tx_leak,
        TraceEvent::SecurityAudit(
            SecurityAuditTrace::new(
                TraceId::new(204),
                SurfaceScope::Application,
                certificate(SurfaceScope::Application),
                principal(),
                Permission::ExecuteProcedure,
                SecurityAuditOutcome::Denied,
                "denied path must not allocate transaction evidence",
            )
            .expect("denial trace construction succeeds before envelope correlation validation"),
        ),
    )
    .unwrap_err();
    assert!(
        denied_with_tx
            .message()
            .contains("must not include transaction")
    );

    let mut unaudited_sequence = InMemoryEventSequence::new();
    unaudited_sequence.append(admission(10, 210)).unwrap();
    let unaudited_denial = EventEnvelope::new(
        EventId::new(11),
        base_correlation(),
        TraceEvent::AuthorizationDenied(AuthorizationDeniedTrace {
            trace_id: TraceId::new(211),
            denied_permission: "Inventory.ReserveStock.Execute".to_string(),
            reason: "legacy denial event lacks mandatory V0 security audit evidence".to_string(),
        }),
    )
    .expect("standalone authorization denial is valid observability evidence");
    let err = unaudited_sequence.append(unaudited_denial).unwrap_err();
    assert!(err.message().contains("V0 security audit event"));
}

#[test]
fn security_audit_rejects_surface_permission_drift_and_secret_evidence() {
    let permission_drift = EventEnvelope::new(
        EventId::new(1),
        base_correlation(),
        TraceEvent::SecurityAudit(
            SecurityAuditTrace::new(
                TraceId::new(301),
                SurfaceScope::Application,
                certificate(SurfaceScope::Application),
                principal(),
                Permission::ManageSecurity,
                SecurityAuditOutcome::Denied,
                "application surface cannot borrow administration permission evidence",
            )
            .expect("trace construction only validates shape"),
        ),
    )
    .unwrap_err();
    assert!(permission_drift.message().contains("surface scope"));

    let certificate_surface_drift = EventEnvelope::new(
        EventId::new(2),
        base_correlation(),
        TraceEvent::SecurityAudit(
            SecurityAuditTrace::new(
                TraceId::new(302),
                SurfaceScope::Application,
                certificate(SurfaceScope::Administration),
                principal(),
                Permission::ExecuteProcedure,
                SecurityAuditOutcome::Denied,
                "certificate scope must match the audited surface",
            )
            .expect("trace construction only validates shape"),
        ),
    )
    .unwrap_err();
    assert!(
        certificate_surface_drift
            .message()
            .contains("certificate surface")
    );

    let secret_certificate = EventEnvelope::new(
        EventId::new(3),
        base_correlation(),
        TraceEvent::SecurityAudit(SecurityAuditTrace {
            trace_id: TraceId::new(303),
            schema_version: andromeda_observe::V0_EVENT_SCHEMA_VERSION,
            surface: SurfaceScope::Application,
            certificate: CertificateIdentity {
                fingerprint: "token=raw-certificate-material".to_string(),
                subject: "CN=v0-worker".to_string(),
                surface: SurfaceScope::Application,
            },
            principal: principal(),
            permission: Permission::ExecuteProcedure,
            outcome: SecurityAuditOutcome::Denied,
            policy_version: SecurityPolicyVersionEvidence::bootstrap_v0(),
            reason: "certificate evidence must be secret-safe".to_string(),
        }),
    )
    .unwrap_err();
    assert!(
        secret_certificate
            .message()
            .contains("must not include secrets")
    );
}
