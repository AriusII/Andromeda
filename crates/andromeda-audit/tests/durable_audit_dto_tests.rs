use andromeda_audit::{
    DurableAuditAppendRecord, DurableAuditEventFamily, DurableAuditFailureKind,
    DurableAuditPrincipalBinding, DurableAuditRecordIdentity, DurableAuditReplayBehavior,
    DurableAuditReplayLsnRange, DurableAuditReplayQuery, DurableAuditReplayWindow,
    DurableAuditRetentionBoundary, DurableAuditWalEvidence, DurableAuditWalSink,
    FileDurableAuditWalSink, Permission, SecurityPolicyVersionEvidence, SurfaceScope,
};
use andromeda_observability::{EventId, TraceId};
use andromeda_types::{RequestId, SessionId};

#[test]
fn durable_audit_owner_dtos_keep_core_invariants() {
    assert!(DurableAuditEventFamily::SecurityDecision.requires_wal_before_visible_decision());
    assert!(DurableAuditFailureKind::CorruptionDetected.requires_fail_closed());
    assert!(
        DurableAuditWalEvidence {
            record_lsn: 7,
            durable_lsn: 9,
            checksum: 11,
        }
        .proves_durable()
    );
}

#[test]
fn replay_query_validation_rejects_invalid_runtime_free_filters() {
    assert!(
        DurableAuditReplayQuery {
            trace_id: Some(TraceId::new(0)),
            ..DurableAuditReplayQuery::all()
        }
        .validate()
        .is_err()
    );

    assert!(!DurableAuditReplayLsnRange::new(10, 2).is_valid());
    assert!(DurableAuditReplayWindow::new(0, 0).validate().is_err());
}

#[test]
fn append_record_validation_rejects_secret_event_kind() {
    let mut record = sample_append_record(1);
    record.event_kind = "payload: raw authorization token".to_string();

    assert!(record.validate().is_err());
}

#[test]
fn append_record_validation_requires_audit_owned_critical_binding_evidence() {
    assert!(DurableAuditEventFamily::SecurityDecision.requires_complete_permission_binding());
    assert!(!DurableAuditEventFamily::AdmissionDecision.requires_complete_permission_binding());

    for (index, family) in [
        DurableAuditEventFamily::SecurityDecision,
        DurableAuditEventFamily::AdminDecision,
        DurableAuditEventFamily::HadrDecision,
        DurableAuditEventFamily::BackupDecision,
        DurableAuditEventFamily::RestoreDecision,
        DurableAuditEventFamily::ForensicDecision,
    ]
    .into_iter()
    .enumerate()
    {
        let mut record = sample_append_record(10 + index as u64);
        record.identity.family = family;
        record.event_kind = format!("{family:?}");
        record.principal_binding.certificate_fingerprint = None;

        let err = record
            .validate()
            .expect_err("critical durable audit DTOs require certificate evidence");
        assert!(
            err.message()
                .contains("certificate, surface, and permission evidence")
        );
    }
}

#[test]
fn append_record_validation_requires_policy_evidence_for_permissioned_critical_records() {
    let mut record = sample_append_record(30);
    record.principal_binding.policy_version = None;

    let err = record
        .validate()
        .expect_err("permissioned critical durable audit DTOs require policy evidence");
    assert!(err.message().contains("policy version evidence"));
}

#[test]
fn replay_record_validation_uses_audit_owned_binding_invariants() {
    let append_record = sample_append_record(40);
    let replay_record = andromeda_audit::DurableAuditReplayRecord {
        report: andromeda_audit::DurableAuditSinkReport {
            identity: append_record.identity,
            evidence: DurableAuditWalEvidence {
                record_lsn: 40,
                durable_lsn: 40,
                checksum: 0xA11D_1700,
            },
            replay_behavior: append_record.replay_behavior,
            retention: append_record.retention,
        },
        principal_binding: append_record.principal_binding,
        event_kind: append_record.event_kind,
    };

    replay_record
        .validate()
        .expect("complete permission and policy evidence is audit-owned replay DTO evidence");
}

#[test]
fn file_sink_appends_and_replays_audit_owned_record() {
    let path = temp_journal_path("file-sink-owned-record");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let report = sink
        .append_durable_audit_record(sample_append_record(1))
        .expect("audit-owned record appends");

    assert_eq!(report.evidence.record_lsn, 1);
    assert!(report.evidence.proves_durable());

    let replayed = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect("journal replays");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].event_kind, "SecurityAudit");
    assert_eq!(
        replayed[0].report.identity.family,
        DurableAuditEventFamily::SecurityDecision
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}.chain", path.display()));
}

fn sample_append_record(sequence_number: u64) -> DurableAuditAppendRecord {
    DurableAuditAppendRecord::new(
        DurableAuditRecordIdentity {
            event_id: EventId::new(10 + u128::from(sequence_number)),
            trace_id: TraceId::new(90),
            family: DurableAuditEventFamily::SecurityDecision,
            sequence_number,
        },
        DurableAuditPrincipalBinding {
            principal_id: "user:durable-audit".to_string(),
            certificate_fingerprint: Some("sha256:durable-audit-contract".to_string()),
            surface: Some(SurfaceScope::Application),
            permission: Some(Permission::ExecuteProcedure),
            policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
            request_id: Some(RequestId::new(7)),
            session_id: Some(SessionId::new(8)),
        },
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        "SecurityAudit",
    )
    .expect("sample append record is valid")
}

fn temp_journal_path(test_name: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time is after UNIX epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "andromeda-audit-{test_name}-{}-{nonce}.audit",
        std::process::id()
    ))
}
