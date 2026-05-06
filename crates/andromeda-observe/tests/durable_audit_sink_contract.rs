use andromeda_core::{CatalogObjectId, CatalogVersion, RequestId, SessionId, digest::sha256};
use andromeda_observe::{
    AdminOperation, AdminOperationTrace, CatalogMutationTrace, CertificateIdentity,
    DurableAuditDecisionGate, DurableAuditEventFamily, DurableAuditFailureKind,
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

fn admin_envelope(
    event_id: u128,
    trace_id: u128,
    operation: AdminOperation,
    permission: Permission,
) -> EventEnvelope {
    let certificate = CertificateIdentity::new(
        format!("sha256:admin-audit-{event_id}"),
        "CN=admin-audit-contract",
        SurfaceScope::Administration,
    )
    .expect("test certificate has explicit non-secret evidence");
    let principal = UserPrincipal::new("user:admin-audit", UserPrincipalKind::Human)
        .expect("test principal has explicit evidence");
    let trace = AdminOperationTrace::new(
        TraceId::new(trace_id),
        SurfaceScope::Administration,
        certificate,
        principal,
        operation,
        permission,
        true,
        "admin decision recorded before visible side effect",
    )
    .expect("admin operation trace has explicit reason");

    EventEnvelope::new(
        EventId::new(event_id),
        request_correlation(),
        TraceEvent::AdminOperation(trace),
    )
    .expect("admin operation envelope is valid")
}

fn admin_principal_binding(event_id: u128, permission: Permission) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: "user:admin-audit".to_string(),
        certificate_fingerprint: Some(format!("sha256:admin-audit-{event_id}")),
        surface: Some(SurfaceScope::Administration),
        permission: Some(permission),
        request_id: Some(RequestId::new(7)),
        session_id: Some(SessionId::new(8)),
    }
}

fn catalog_publication_envelope() -> EventEnvelope {
    let catalog_version = CatalogVersion::new(42);
    let object_id = CatalogObjectId::new(700);
    let trace = CatalogMutationTrace {
        trace_id: TraceId::new(142),
        catalog_version,
        object_id: Some(object_id),
        action: "publication_manifest_committed".to_string(),
    };
    let correlation = EventCorrelation {
        request_id: Some(RequestId::new(70)),
        session_id: Some(SessionId::new(80)),
        contract_hash: None,
        catalog_version: Some(catalog_version),
        catalog_object_id: Some(object_id),
        transaction_id: None,
        durable_lsn: None,
        protocol: None,
    };

    EventEnvelope::new(
        EventId::new(142),
        correlation,
        TraceEvent::CatalogMutation(trace),
    )
    .expect("catalog publication envelope is valid")
}

fn catalog_principal_binding() -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: "svc:catalog-publisher".to_string(),
        certificate_fingerprint: Some("sha256:catalog-publication".to_string()),
        surface: Some(SurfaceScope::Administration),
        permission: Some(Permission::ImportDefinitionBatch),
        request_id: Some(RequestId::new(70)),
        session_id: Some(SessionId::new(80)),
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

fn hex_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        encoded.push_str(&format!("{byte:02x}"));
    }
    encoded
}

fn durable_audit_test_checksum64(bytes: &[u8]) -> u64 {
    let digest = sha256(bytes);
    let checksum = u64::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ]);
    checksum.max(1)
}

fn journal_line_with_checksum(payload: &str) -> String {
    let checksum = durable_audit_test_checksum64(payload.as_bytes());
    format!("{payload}|checksum={checksum:016x}\n")
}

struct FailingDurableAuditWalSink;

impl DurableAuditWalSink for FailingDurableAuditWalSink {
    fn append_durable_audit_record(
        &mut self,
        record: PendingDurableAuditRecord,
    ) -> Result<DurableAuditSinkReport, DurableAuditSinkFailure> {
        Err(DurableAuditSinkFailure::new(
            DurableAuditFailureKind::WalFlushRejected,
            Some(record.identity),
            "simulated durable audit WAL flush failure",
        )
        .expect("test failure reason is valid"))
    }
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
fn admin_decision_fails_closed_when_audit_sink_fails() {
    let record = PendingDurableAuditRecord::new(
        20,
        admin_principal_binding(20, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            20,
            120,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("admin durable audit record is valid before sink failure");
    assert_eq!(
        record.identity.family,
        DurableAuditEventFamily::AdminDecision
    );

    let mut sink = FailingDurableAuditWalSink;
    let failure = DurableAuditDecisionGate::new(DurableAuditEventFamily::AdminDecision)
        .append_and_prove(&mut sink, record)
        .expect_err("visible admin decision fails closed when audit WAL flush fails");

    assert_eq!(failure.kind, DurableAuditFailureKind::WalFlushRejected);
    assert!(failure.requires_fail_closed());
}

#[test]
fn visible_decision_gate_rejects_non_visible_family_before_journal_append() {
    let path = temp_journal_path("non-visible-gate-no-append");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        23,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(),
    )
    .expect("security durable audit record is valid");

    let failure = DurableAuditDecisionGate::new(DurableAuditEventFamily::AdmissionDecision)
        .append_and_prove(&mut sink, record)
        .expect_err("non-visible decision families must be rejected before append");

    assert_eq!(failure.kind, DurableAuditFailureKind::ValidationRejected);
    assert!(failure.reason.contains("visible decision family"));
    assert!(
        fs::read_to_string(&path)
            .expect("journal remains readable")
            .is_empty(),
        "visible decision gate must not append records for non-visible families"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn security_allow_requires_durable_audit_evidence() {
    let record = PendingDurableAuditRecord::new(
        21,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        security_envelope(),
    )
    .expect("security durable audit record is valid");
    let missing_flush = DurableAuditSinkReport {
        identity: record.identity,
        evidence: DurableAuditWalEvidence {
            record_lsn: 0,
            durable_lsn: 0,
            checksum: 0,
        },
        replay_behavior: DurableAuditReplayBehavior::RebuildDecisionIndex,
        retention: DurableAuditRetentionBoundary::SecurityPolicy,
    };

    let failure = DurableAuditDecisionGate::new(DurableAuditEventFamily::SecurityDecision)
        .prove_visible_decision(missing_flush, principal_binding())
        .expect_err("security allow must not become visible without durable audit evidence");

    assert_eq!(failure.kind, DurableAuditFailureKind::ValidationRejected);
    assert!(failure.requires_fail_closed());
    assert!(failure.reason.contains("flushed WAL evidence"));
}

#[test]
fn catalog_publication_requires_audit_wal() {
    let path = temp_journal_path("catalog-publication-gate");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        22,
        catalog_principal_binding(),
        DurableAuditRetentionBoundary::CatalogVersion,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        catalog_publication_envelope(),
    )
    .expect("catalog publication durable audit record is valid");
    assert_eq!(
        record.identity.family,
        DurableAuditEventFamily::CatalogDecision
    );

    let proof = DurableAuditDecisionGate::new(DurableAuditEventFamily::CatalogDecision)
        .append_and_prove(&mut sink, record)
        .expect("catalog publication is visible only after audit WAL flush");

    assert_eq!(
        proof.report.identity.family,
        DurableAuditEventFamily::CatalogDecision
    );
    assert_ne!(proof.report.evidence.record_lsn, 0);
    assert_eq!(
        proof.report.retention,
        DurableAuditRetentionBoundary::CatalogVersion
    );

    let replayed = sink
        .query(&DurableAuditReplayQuery {
            family: Some(DurableAuditEventFamily::CatalogDecision),
            ..DurableAuditReplayQuery::all()
        })
        .expect("catalog publication audit WAL remains queryable");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].report, proof.report);

    let _ = fs::remove_file(path);
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
    assert!(DurableAuditEventFamily::HadrDecision.requires_wal_before_visible_decision());
    assert!(DurableAuditEventFamily::BackupDecision.requires_wal_before_visible_decision());
    assert!(DurableAuditEventFamily::RestoreDecision.requires_wal_before_visible_decision());
    assert!(DurableAuditEventFamily::ForensicDecision.requires_wal_before_visible_decision());
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
            lsn_range: Some(DurableAuditReplayLsnRange::new(
                report.evidence.record_lsn,
                report.evidence.durable_lsn,
            )),
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

    let query_failure = sink
        .query(&DurableAuditReplayQuery {
            family: Some(DurableAuditEventFamily::SecurityDecision),
            trace_id: Some(TraceId::new(90)),
            principal_id: Some("user:durable-audit".to_string()),
            lsn_range: Some(DurableAuditReplayLsnRange::new(1, u64::MAX)),
        })
        .expect_err("targeted replay query detects checksum corruption");
    assert_eq!(
        query_failure.kind,
        DurableAuditFailureKind::CorruptionDetected
    );

    let failure =
        FileDurableAuditWalSink::open(&path).expect_err("checksum mismatch blocks replay/open");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_non_ascii_journal_payload_without_panic() {
    let path = temp_journal_path("non-ascii-journal-payload");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        5,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record contract is satisfied");

    sink.append_durable_audit_record(record)
        .expect("append succeeds before corruption");
    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let (payload, _) = journal_text
        .trim_end()
        .rsplit_once("|checksum=")
        .expect("journal line has checksum field");
    let principal_field = format!("principal_id={}", hex_encode("user:durable-audit"));
    let corrupted_payload = payload.replace(&principal_field, "principal_id=€a");
    assert_ne!(
        payload, corrupted_payload,
        "test corruption must alter the principal field"
    );
    fs::write(&path, journal_line_with_checksum(&corrupted_payload))
        .expect("test can rewrite malformed journal");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("malformed non-ASCII journal payload is rejected as corruption");

    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("ASCII"),
        "journal parser should reject non-ASCII payloads before field decoding"
    );

    let _ = fs::remove_file(path);
}
