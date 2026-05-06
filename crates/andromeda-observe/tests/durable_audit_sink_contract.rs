use andromeda_core::{RequestId, SessionId};
use andromeda_observe::{
    CertificateIdentity, DurableAuditEventFamily, DurableAuditFailureKind,
    DurableAuditPrincipalBinding, DurableAuditReplayBehavior, DurableAuditReplayLsnRange,
    DurableAuditReplayQuery, DurableAuditRetentionBoundary, DurableAuditSinkFailure,
    DurableAuditSinkReport, DurableAuditWalEvidence, DurableAuditWalSink, EventCorrelation,
    EventEnvelope, EventId, FileDurableAuditWalSink, PendingDurableAuditRecord, Permission,
    SecurityAuditOutcome, SecurityAuditTrace, SurfaceScope, TraceEvent, TraceId, UserPrincipal,
    UserPrincipalKind,
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn request_correlation() -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(7)),
        session_id: Some(SessionId::new(8)),
        contract_hash: None,
        catalog_version: None,
        catalog_object_id: None,
        transaction_id: None,
        durable_lsn: None,
        protocol: None,
    }
}

fn security_envelope() -> EventEnvelope {
    let certificate = CertificateIdentity::new(
        "sha256:durable-audit-contract",
        "CN=durable-audit-contract",
        SurfaceScope::Application,
    )
    .expect("test certificate has explicit non-secret evidence");
    let principal = UserPrincipal::new("user:durable-audit", UserPrincipalKind::Human)
        .expect("test principal has explicit evidence");
    let trace = SecurityAuditTrace::new(
        TraceId::new(90),
        SurfaceScope::Application,
        certificate,
        principal,
        Permission::ExecuteProcedure,
        SecurityAuditOutcome::Allowed,
        "permission grant recorded before dispatch",
    )
    .expect("security audit trace has explicit reason");

    EventEnvelope::new(
        EventId::new(11),
        request_correlation(),
        TraceEvent::SecurityAudit(trace),
    )
    .expect("security audit envelope is valid")
}

fn principal_binding() -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: "user:durable-audit".to_string(),
        certificate_fingerprint: Some("sha256:durable-audit-contract".to_string()),
        surface: Some(SurfaceScope::Application),
        permission: Some(Permission::ExecuteProcedure),
        request_id: Some(RequestId::new(7)),
        session_id: Some(SessionId::new(8)),
    }
}

fn temp_journal_path(test_name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after UNIX epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "andromeda-observe-{test_name}-{}-{nonce}.audit",
        std::process::id()
    ))
}

#[test]
fn pending_durable_audit_record_binds_identity_family_principal_and_session() {
    let record = PendingDurableAuditRecord::new(
        1,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(),
    )
    .expect("durable audit record contract is satisfied");

    assert_eq!(record.identity.event_id, EventId::new(11));
    assert_eq!(record.identity.trace_id, TraceId::new(90));
    assert_eq!(
        record.identity.family,
        DurableAuditEventFamily::SecurityDecision
    );
    assert_eq!(record.principal_binding.request_id, Some(RequestId::new(7)));
    assert_eq!(record.principal_binding.session_id, Some(SessionId::new(8)));
    record.validate().expect("record remains valid");
}

#[test]
fn durable_audit_sink_report_requires_durable_lsn_and_checksum_evidence() {
    let record = PendingDurableAuditRecord::new(
        2,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(),
    )
    .expect("durable audit record contract is satisfied");

    let report = DurableAuditSinkReport {
        identity: record.identity,
        evidence: DurableAuditWalEvidence {
            record_lsn: 10,
            durable_lsn: 10,
            checksum: 0xA11D17,
        },
        replay_behavior: DurableAuditReplayBehavior::RebuildDecisionIndex,
        retention: DurableAuditRetentionBoundary::SecurityPolicy,
    };

    report
        .validate()
        .expect("non-zero LSN/checksum evidence proves durability");

    let missing_flush = DurableAuditSinkReport {
        evidence: DurableAuditWalEvidence {
            record_lsn: 10,
            durable_lsn: 9,
            checksum: 0xA11D17,
        },
        ..report
    };
    assert!(
        missing_flush
            .validate()
            .unwrap_err()
            .message()
            .contains("WAL LSN/checksum evidence")
    );
}

#[test]
fn durable_audit_contract_rejects_secret_principal_evidence_and_zero_sequence() {
    let secret_binding = DurableAuditPrincipalBinding {
        principal_id: "token=must-not-enter-audit".to_string(),
        ..principal_binding()
    };

    assert!(
        PendingDurableAuditRecord::new(
            1,
            secret_binding,
            DurableAuditRetentionBoundary::SecurityPolicy,
            DurableAuditReplayBehavior::ForensicOnly,
            security_envelope(),
        )
        .unwrap_err()
        .message()
        .contains("secret evidence")
    );

    assert!(
        PendingDurableAuditRecord::new(
            0,
            principal_binding(),
            DurableAuditRetentionBoundary::SecurityPolicy,
            DurableAuditReplayBehavior::ForensicOnly,
            security_envelope(),
        )
        .unwrap_err()
        .message()
        .contains("sequence_number")
    );
}

#[test]
fn durable_audit_failure_kinds_fail_closed_without_global_disable_mode() {
    assert!(DurableAuditEventFamily::SecurityDecision.requires_wal_before_visible_decision());
    assert!(DurableAuditEventFamily::AdminDecision.requires_wal_before_visible_decision());
    assert!(DurableAuditEventFamily::CatalogDecision.requires_wal_before_visible_decision());
    assert!(DurableAuditFailureKind::ValidationRejected.requires_fail_closed());
    assert!(DurableAuditFailureKind::WalAppendRejected.requires_fail_closed());
    assert!(DurableAuditFailureKind::WalFlushRejected.requires_fail_closed());
    assert!(DurableAuditFailureKind::CorruptionDetected.requires_fail_closed());
    assert!(DurableAuditFailureKind::PermissionDenied.requires_fail_closed());
    assert!(DurableAuditFailureKind::RetentionRejected.requires_fail_closed());

    let failure = DurableAuditSinkFailure::new(
        DurableAuditFailureKind::WalFlushRejected,
        None,
        "audit WAL flush failed before visible decision",
    )
    .expect("typed failure carries reason evidence");
    assert!(failure.requires_fail_closed());
}

#[test]
fn durable_audit_replay_query_rejects_unsafe_filters() {
    let zero_trace = DurableAuditReplayQuery {
        trace_id: Some(TraceId::new(0)),
        ..DurableAuditReplayQuery::all()
    };
    assert!(
        zero_trace
            .validate()
            .unwrap_err()
            .message()
            .contains("trace_id")
    );

    let empty_principal = DurableAuditReplayQuery {
        principal_id: Some(" \t".to_string()),
        ..DurableAuditReplayQuery::all()
    };
    assert!(
        empty_principal
            .validate()
            .unwrap_err()
            .message()
            .contains("principal filter")
    );

    let secret_principal = DurableAuditReplayQuery {
        principal_id: Some("token=must-not-be-queryable".to_string()),
        ..DurableAuditReplayQuery::all()
    };
    assert!(
        secret_principal
            .validate()
            .unwrap_err()
            .message()
            .contains("secret evidence")
    );

    let invalid_lsn = DurableAuditReplayQuery {
        lsn_range: Some(DurableAuditReplayLsnRange::new(20, 10)),
        ..DurableAuditReplayQuery::all()
    };
    assert!(
        invalid_lsn
            .validate()
            .unwrap_err()
            .message()
            .contains("LSN range")
    );
}

#[test]
fn file_backed_durable_audit_sink_survives_restart_and_replays_targeted_index() {
    let path = temp_journal_path("restart-replay");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        3,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(),
    )
    .expect("durable audit record contract is satisfied");

    let report = sink
        .append_durable_audit_record(record)
        .expect("append is flushed before success");
    report.validate().expect("report proves durable audit WAL");
    assert_ne!(report.evidence.record_lsn, 0);
    assert_eq!(report.evidence.record_lsn, report.evidence.durable_lsn);
    assert_ne!(report.evidence.checksum, 0);

    drop(sink);
    let reopened = FileDurableAuditWalSink::open(&path).expect("journal reopens after restart");
    let replayed = reopened
        .replay(&DurableAuditReplayQuery {
            family: Some(DurableAuditEventFamily::SecurityDecision),
            trace_id: Some(TraceId::new(90)),
            principal_id: Some("user:durable-audit".to_string()),
            lsn_range: None,
        })
        .expect("targeted replay scans the durable journal");

    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].report, report);
    assert_eq!(
        replayed[0].principal_binding.request_id,
        Some(RequestId::new(7))
    );
    assert_eq!(
        replayed[0].principal_binding.session_id,
        Some(SessionId::new(8))
    );
    assert_eq!(replayed[0].event_kind, "SecurityAudit");

    let journal_text = fs::read_to_string(&path).expect("journal is readable");
    assert!(!journal_text.contains("transaction_id="));
    assert!(!journal_text.contains("token="));

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_detects_checksum_corruption() {
    let path = temp_journal_path("checksum-corruption");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        4,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record contract is satisfied");

    sink.append_durable_audit_record(record)
        .expect("append succeeds before corruption");
    let corrupted = fs::read_to_string(&path)
        .expect("journal can be read")
        .replace("SecurityDecision", "GenericAudit");
    fs::write(&path, corrupted).expect("test can corrupt journal in place");

    let failure =
        FileDurableAuditWalSink::open(&path).expect_err("checksum mismatch blocks replay/open");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);

    let _ = fs::remove_file(path);
}
