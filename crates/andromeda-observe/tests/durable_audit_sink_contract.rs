use andromeda_core::{CatalogObjectId, CatalogVersion, RequestId, SessionId, digest::sha256};
use andromeda_observe::{
    AdminOperation, AdminOperationTrace, CatalogMutationTrace, CertificateIdentity,
    DurableAuditDecisionGate, DurableAuditEventFamily, DurableAuditFailureKind,
    DurableAuditPrincipalBinding, DurableAuditReplayBehavior, DurableAuditReplayLsnRange,
    DurableAuditReplayQuery, DurableAuditReplayRecord, DurableAuditRetentionBoundary,
    DurableAuditSinkFailure, DurableAuditSinkReport, DurableAuditWalEvidence, DurableAuditWalSink,
    EventCorrelation, EventEnvelope, EventId, FileDurableAuditWalSink, PendingDurableAuditRecord,
    Permission, SecurityAuditOutcome, SecurityAuditTrace, SecurityPolicyVersionEvidence,
    SurfaceScope, TraceEvent, TraceId, UserPrincipal, UserPrincipalKind,
};
use std::{
    fs,
    path::{Path, PathBuf},
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
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
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
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(7)),
        session_id: Some(SessionId::new(8)),
    }
}

fn hadr_envelope(
    event_id: u128,
    trace_id: u128,
    operation: AdminOperation,
    permission: Permission,
) -> EventEnvelope {
    let certificate = CertificateIdentity::new(
        format!("sha256:hadr-audit-{event_id}"),
        "CN=hadr-audit-contract",
        SurfaceScope::Cluster,
    )
    .expect("test certificate has explicit cluster evidence");
    let principal = UserPrincipal::new("svc:hadr-audit", UserPrincipalKind::Service)
        .expect("test service principal has explicit evidence");
    let trace = AdminOperationTrace::new(
        TraceId::new(trace_id),
        SurfaceScope::Cluster,
        certificate,
        principal,
        operation,
        permission,
        true,
        "cluster decision recorded before primary visibility",
    )
    .expect("HADR operation trace has explicit reason");

    EventEnvelope::new(
        EventId::new(event_id),
        request_correlation(),
        TraceEvent::AdminOperation(trace),
    )
    .expect("HADR operation envelope is valid")
}

fn hadr_principal_binding(event_id: u128, permission: Permission) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: "svc:hadr-audit".to_string(),
        certificate_fingerprint: Some(format!("sha256:hadr-audit-{event_id}")),
        surface: Some(SurfaceScope::Cluster),
        permission: Some(permission),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
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
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
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

fn journal_line_with_chain(payload: &str, previous_chain_checksum: u64) -> String {
    let checksum = durable_audit_test_checksum64(payload.as_bytes());
    let chain_checksum = durable_audit_test_checksum64(
        format!("{previous_chain_checksum:016x}|{checksum:016x}|{payload}").as_bytes(),
    );
    format!(
        "{payload}|previous_chain_checksum={previous_chain_checksum:016x}|chain_checksum={chain_checksum:016x}|checksum={checksum:016x}\n"
    )
}

fn journal_payload_from_line(line: &str) -> &str {
    line.rsplit_once("|previous_chain_checksum=")
        .expect("journal line carries chain predecessor evidence")
        .0
}

fn chain_checksum_from_line(line: &str) -> u64 {
    let (_, chain_and_checksum) = line
        .rsplit_once("|chain_checksum=")
        .expect("journal line carries chain checksum evidence");
    let (chain_checksum, _) = chain_and_checksum
        .split_once("|checksum=")
        .expect("journal line carries trailing record checksum");
    u64::from_str_radix(chain_checksum, 16).expect("chain checksum is fixed-width hex")
}

fn journal_chain_anchor_path(path: &Path) -> PathBuf {
    let mut anchor = path.as_os_str().to_os_string();
    anchor.push(".chain");
    PathBuf::from(anchor)
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
fn permissioned_admin_records_fail_closed_without_policy_evidence() {
    let mut missing_policy = admin_principal_binding(24, Permission::InspectPlans);
    missing_policy.policy_version = None;
    let err = PendingDurableAuditRecord::new(
        24,
        missing_policy,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            24,
            124,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect_err("permissioned admin durable audit records require policy evidence");

    assert!(
        err.message()
            .contains("permissioned critical AdminDecision records")
    );

    let mut zero_policy = admin_principal_binding(29, Permission::InspectPlans);
    zero_policy.policy_version = Some(SecurityPolicyVersionEvidence {
        policy_version: 0,
        policy_digest: "sha256:9999999999999999999999999999999999999999999999999999999999999999"
            .to_string(),
    });
    let err = PendingDurableAuditRecord::new(
        29,
        zero_policy,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            29,
            129,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect_err("zero policy version evidence must fail closed");
    assert!(
        err.message()
            .contains("policy version evidence must be non-zero")
    );
}

#[test]
fn permissioned_hadr_records_require_policy_evidence_before_visible_decision() {
    let path = temp_journal_path("hadr-policy-evidence-gate");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let mut missing_policy = hadr_principal_binding(25, Permission::ClusterPromote);
    missing_policy.policy_version = None;
    let rejected = PendingDurableAuditRecord::new(
        25,
        missing_policy,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        hadr_envelope(
            25,
            125,
            AdminOperation::ClusterPromote,
            Permission::ClusterPromote,
        ),
    )
    .expect_err("HADR durable audit records require policy evidence");
    assert!(
        rejected
            .message()
            .contains("permissioned critical HadrDecision records")
    );

    let record = PendingDurableAuditRecord::new(
        26,
        hadr_principal_binding(26, Permission::ClusterPromote),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        hadr_envelope(
            26,
            126,
            AdminOperation::ClusterPromote,
            Permission::ClusterPromote,
        ),
    )
    .expect("HADR record with policy evidence is durable-audit eligible");
    let proof = DurableAuditDecisionGate::new(DurableAuditEventFamily::HadrDecision)
        .append_and_prove(&mut sink, record)
        .expect("HADR visible decision is proved by durable audit policy evidence");

    assert_eq!(
        proof.report.identity.family,
        DurableAuditEventFamily::HadrDecision
    );
    assert!(
        proof
            .principal_binding
            .policy_version
            .as_ref()
            .is_some_and(SecurityPolicyVersionEvidence::has_version_evidence)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn recovery_replay_policy_gate_only_applies_to_permissioned_records() {
    let base_report = DurableAuditSinkReport {
        identity: andromeda_observe::DurableAuditRecordIdentity {
            event_id: EventId::new(27),
            trace_id: TraceId::new(127),
            family: DurableAuditEventFamily::RecoveryDecision,
            sequence_number: 27,
        },
        evidence: DurableAuditWalEvidence {
            record_lsn: 27,
            durable_lsn: 27,
            checksum: 0xA11D_1701,
        },
        replay_behavior: DurableAuditReplayBehavior::ForensicOnly,
        retention: DurableAuditRetentionBoundary::ForensicHold,
    };
    let observational_recovery = DurableAuditReplayRecord {
        report: base_report,
        principal_binding: DurableAuditPrincipalBinding {
            principal_id: "svc:recovery-observer".to_string(),
            certificate_fingerprint: None,
            surface: Some(SurfaceScope::MonitoringAgent),
            permission: None,
            policy_version: None,
            request_id: Some(RequestId::new(7)),
            session_id: Some(SessionId::new(8)),
        },
        event_kind: "RecoveryStartup".to_string(),
    };
    observational_recovery
        .validate()
        .expect("non-permissioned recovery replay records remain compatible");

    let permissioned_without_policy = DurableAuditReplayRecord {
        report: DurableAuditSinkReport {
            identity: andromeda_observe::DurableAuditRecordIdentity {
                event_id: EventId::new(28),
                trace_id: TraceId::new(128),
                family: DurableAuditEventFamily::RecoveryDecision,
                sequence_number: 28,
            },
            ..base_report
        },
        principal_binding: DurableAuditPrincipalBinding {
            permission: Some(Permission::Restore),
            surface: Some(SurfaceScope::BackupAgent),
            ..observational_recovery.principal_binding
        },
        event_kind: "RecoveryStartup".to_string(),
    };
    let err = permissioned_without_policy
        .validate()
        .expect_err("permissioned recovery replay records fail closed without policy evidence");
    assert!(
        err.message()
            .contains("permissioned critical RecoveryDecision records")
    );
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
        .expect("catalog publication audit WAL remains replayable");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].report, proof.report);

    let _ = fs::remove_file(path);
}

#[test]
fn durable_audit_contract_rejects_secret_principal_evidence_and_zero_sequence() {
    let secret_binding = DurableAuditPrincipalBinding {
        principal_id: "credential=must-not-enter-audit".to_string(),
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
fn durable_audit_principal_binding_rejects_impossible_permission_and_half_correlation() {
    let impossible_permission = DurableAuditPrincipalBinding {
        surface: Some(SurfaceScope::Application),
        permission: Some(Permission::ManageSecurity),
        ..principal_binding()
    };
    assert!(
        impossible_permission
            .validate()
            .unwrap_err()
            .message()
            .contains("surface must permit permission")
    );

    let half_correlated = DurableAuditPrincipalBinding {
        session_id: None,
        ..principal_binding()
    };
    assert!(
        half_correlated
            .validate()
            .unwrap_err()
            .message()
            .contains("present together")
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
fn durable_audit_replay_filter_rejects_unsafe_inputs() {
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
        principal_id: Some("private_key=must-not-enter-replay-filter".to_string()),
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
        .expect_err("targeted replay inspection detects checksum corruption");
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
fn file_backed_durable_audit_replay_detects_checksum_chain_gap() {
    let path = temp_journal_path("checksum-chain-gap");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        6,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    sink.append_durable_audit_record(first)
        .expect("first append succeeds before corruption");

    let second = PendingDurableAuditRecord::new(
        7,
        admin_principal_binding(70, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            70,
            170,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    sink.append_durable_audit_record(second)
        .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    assert!(
        journal_text.contains("previous_chain_checksum="),
        "journal records carry explicit chain predecessor evidence"
    );
    assert!(
        journal_text.contains("chain_checksum="),
        "journal records carry explicit chain checksum evidence"
    );
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    fs::write(&path, format!("{}\n", lines[1])).expect("test can remove first chain record");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("removing a prior record breaks the checksum chain");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("checksum chain previous value mismatch"),
        "replay should identify the broken predecessor link"
    );

    let open_failure =
        FileDurableAuditWalSink::open(&path).expect_err("broken checksum chain blocks open");
    assert_eq!(
        open_failure.kind,
        DurableAuditFailureKind::CorruptionDetected
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_missing_chain_anchor_for_non_empty_journal() {
    let path = temp_journal_path("checksum-chain-missing-anchor");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        60,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append writes journal and chain anchor");

    fs::remove_file(journal_chain_anchor_path(&path)).expect("test can remove chain anchor");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("non-empty journal without a chain anchor must fail closed");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("chain anchor is required"),
        "replay should not silently accept journal records after anchor deletion"
    );

    let open_failure =
        FileDurableAuditWalSink::open(&path).expect_err("missing chain anchor blocks open");
    assert_eq!(
        open_failure.kind,
        DurableAuditFailureKind::CorruptionDetected
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_reordered_chain_records() {
    let path = temp_journal_path("checksum-chain-reorder");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        61,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    sink.append_durable_audit_record(first)
        .expect("first append succeeds before corruption");

    let second = PendingDurableAuditRecord::new(
        62,
        admin_principal_binding(62, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            62,
            162,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    sink.append_durable_audit_record(second)
        .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    fs::write(&path, format!("{}\n{}\n", lines[1], lines[0]))
        .expect("test can reorder journal records");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("reordered durable audit records break the checksum chain");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("checksum chain previous value mismatch"),
        "replay should reject records whose predecessor evidence does not match scan order"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_duplicate_lsn_even_when_rechained() {
    let path = temp_journal_path("checksum-chain-duplicate-lsn");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        69,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before duplicate LSN corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1);
    let duplicate_payload = journal_payload_from_line(lines[0]);
    let duplicate_line =
        journal_line_with_chain(duplicate_payload, chain_checksum_from_line(lines[0]));
    fs::write(&path, format!("{}\n{}", lines[0], duplicate_line))
        .expect("test can append a rechained duplicate LSN");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("duplicate LSN must fail even when the checksum chain is syntactically valid");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("record LSNs must increase strictly"),
        "replay should reject duplicate LSNs after chain validation"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_inserted_chain_record() {
    let path = temp_journal_path("checksum-chain-insertion");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        69,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    sink.append_durable_audit_record(first)
        .expect("first append succeeds before corruption");

    let second = PendingDurableAuditRecord::new(
        70,
        admin_principal_binding(70, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            70,
            170,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    sink.append_durable_audit_record(second)
        .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    fs::write(&path, format!("{}\n{}\n{}\n", lines[0], lines[0], lines[1]))
        .expect("test can insert a stale replay record into the chain");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("inserted durable audit records must break predecessor evidence");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("checksum chain previous value mismatch"),
        "replay should reject inserted records whose predecessor evidence is stale"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_append_after_reopen_and_replay_continues_chain() {
    let path = temp_journal_path("append-after-replay");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        71,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    let first_report = sink
        .append_durable_audit_record(first)
        .expect("first append succeeds before restart");
    drop(sink);

    let mut reopened = FileDurableAuditWalSink::open(&path).expect("journal reopens");
    let replayed = reopened
        .replay(&DurableAuditReplayQuery::all())
        .expect("reopened journal replays before next append");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].report, first_report);

    let second = PendingDurableAuditRecord::new(
        72,
        admin_principal_binding(72, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            72,
            172,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    let second_report = reopened
        .append_durable_audit_record(second)
        .expect("append after replay succeeds with continued chain evidence");
    assert_eq!(
        second_report.evidence.record_lsn,
        first_report.evidence.record_lsn + 1,
        "append after replay must continue from the durable chain tail"
    );

    let replayed_after_append = reopened
        .replay(&DurableAuditReplayQuery::all())
        .expect("journal remains replayable after append continuation");
    assert_eq!(replayed_after_append.len(), 2);
    assert_eq!(replayed_after_append[0].report, first_report);
    assert_eq!(replayed_after_append[1].report, second_report);

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert!(
        lines[1].contains("previous_chain_checksum="),
        "continued append must persist predecessor chain evidence"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_legacy_journal_without_chain_fields() {
    let path = temp_journal_path("legacy-no-chain-fields");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        73,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before legacy rewrite");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let legacy_payload = journal_payload_from_line(
        journal_text
            .lines()
            .next()
            .expect("journal contains one current-format line"),
    );
    fs::write(&path, journal_line_with_checksum(legacy_payload))
        .expect("test can rewrite journal to legacy checksum-only format");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("legacy checksum-only journals are explicitly rejected");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("missing chain_checksum field"),
        "legacy rejection should name the missing chain evidence field"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_unsupported_journal_format_version() {
    let path = temp_journal_path("unsupported-format-version");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        74,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before format version corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let payload = journal_payload_from_line(
        journal_text
            .lines()
            .next()
            .expect("journal contains one current-format line"),
    );
    let unsupported_payload = payload.replacen(
        "andromeda-durable-audit-v2",
        "andromeda-durable-audit-v99",
        1,
    );
    assert_ne!(
        payload, unsupported_payload,
        "test corruption must change the durable audit journal format prefix"
    );
    fs::write(&path, journal_line_with_chain(&unsupported_payload, 0))
        .expect("test can rewrite journal with a valid checksum chain");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("unsupported durable audit journal versions fail closed");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("format version") && failure.reason.contains("supported versions"),
        "version rejection should expose explicit format-version evidence"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_truncated_record_tail() {
    let path = temp_journal_path("checksum-chain-tail-truncation");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        63,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before corruption");

    let mut journal_text = fs::read_to_string(&path).expect("journal can be read");
    assert!(
        journal_text.ends_with('\n'),
        "journal record is newline-delimited before corruption"
    );
    journal_text.pop();
    fs::write(&path, journal_text).expect("test can truncate final record delimiter");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("missing record delimiter is a truncation boundary");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("record-delimited"),
        "replay should surface truncation as an observable record delimiter failure"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_secret_journal_fields_without_leak() {
    let path = temp_journal_path("secret-journal-field");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        75,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before secret-bearing corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let payload = journal_payload_from_line(
        journal_text
            .lines()
            .next()
            .expect("journal contains one current-format line"),
    );
    let principal_field = format!("principal_id={}", hex_encode("user:durable-audit"));
    let secret_principal_field = format!(
        "principal_id={}",
        hex_encode("token=must-not-enter-journal")
    );
    let corrupted_payload = payload.replace(&principal_field, &secret_principal_field);
    assert_ne!(
        payload, corrupted_payload,
        "test corruption must inject secret-bearing principal evidence"
    );
    fs::write(&path, journal_line_with_chain(&corrupted_payload, 0))
        .expect("test can rewrite journal with a valid checksum chain");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("secret-bearing journal evidence must fail closed");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(failure.reason.contains("secret evidence"));
    assert!(
        !failure.reason.contains("must-not-enter-journal"),
        "durable audit failure evidence must not echo secret-bearing journal fields"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_chain_checksum_alteration() {
    let path = temp_journal_path("checksum-chain-alteration");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        64,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let corrupted = journal_text.replacen("|chain_checksum=", "|chain_checksum=ffffffff", 1);
    fs::write(&path, corrupted).expect("test can alter chain checksum field");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("altered chain checksum is rejected");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("chain_checksum field")
            || failure.reason.contains("chain checksum mismatch"),
        "replay should identify the altered chain checksum field"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_forged_chain_tail_against_anchor() {
    let path = temp_journal_path("checksum-chain-forged-tail");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        65,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    sink.append_durable_audit_record(first)
        .expect("first append succeeds before corruption");

    let second = PendingDurableAuditRecord::new(
        66,
        admin_principal_binding(66, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            66,
            166,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    sink.append_durable_audit_record(second)
        .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    fs::write(&path, format!("{}\n", lines[0])).expect("test can remove the anchored chain tail");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("removing the tail record must not satisfy the persisted chain anchor");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("chain anchor last record LSN mismatch")
            || failure
                .reason
                .contains("chain anchor record count mismatch")
            || failure
                .reason
                .contains("chain anchor tail checksum mismatch"),
        "replay should identify the tail deletion against persisted anchor evidence"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_chain_anchor_checksum_alteration() {
    let path = temp_journal_path("checksum-anchor-alteration");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let record = PendingDurableAuditRecord::new(
        76,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("durable audit record is valid");
    sink.append_durable_audit_record(record)
        .expect("append succeeds before anchor checksum corruption");

    let anchor_path = journal_chain_anchor_path(&path);
    let anchor_text = fs::read_to_string(&anchor_path).expect("chain anchor can be read");
    let (payload, _) = anchor_text
        .trim_end()
        .rsplit_once("|checksum=")
        .expect("chain anchor carries checksum evidence");
    fs::write(
        &anchor_path,
        format!("{payload}|checksum=ffffffffffffffff\n"),
    )
    .expect("test can alter chain anchor checksum evidence");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("altered chain anchor checksum is rejected");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("chain anchor checksum mismatch"),
        "replay should identify the altered chain anchor checksum field"
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(anchor_path);
}

#[test]
fn file_backed_durable_audit_replay_rejects_forged_chain_head_against_anchor() {
    let path = temp_journal_path("checksum-chain-forged-head");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        67,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    sink.append_durable_audit_record(first)
        .expect("first append succeeds before corruption");

    let second = PendingDurableAuditRecord::new(
        68,
        admin_principal_binding(68, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            68,
            168,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    sink.append_durable_audit_record(second)
        .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    let forged_head_payload = journal_payload_from_line(lines[1]);
    fs::write(&path, journal_line_with_chain(forged_head_payload, 0))
        .expect("test can forge a syntactically valid chain head from the second record");

    let failure = sink
        .query(&DurableAuditReplayQuery::all())
        .expect_err("forged chain head must not satisfy persisted anchor evidence");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("chain anchor first record LSN mismatch")
            || failure
                .reason
                .contains("chain anchor record count mismatch"),
        "replay should identify the forged head against persisted anchor evidence"
    );

    let _ = fs::remove_file(path);
}

#[test]
fn file_backed_durable_audit_targeted_replay_validates_unmatched_chain_prefix() {
    let path = temp_journal_path("targeted-chain-prefix");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = PendingDurableAuditRecord::new(
        8,
        principal_binding(),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(),
    )
    .expect("first durable audit record is valid");
    sink.append_durable_audit_record(first)
        .expect("first append succeeds before corruption");

    let second = PendingDurableAuditRecord::new(
        9,
        admin_principal_binding(80, Permission::InspectPlans),
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        admin_envelope(
            80,
            180,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
        ),
    )
    .expect("second durable audit record is valid");
    sink.append_durable_audit_record(second)
        .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(&path).expect("journal can be read");
    let corrupted = journal_text.replace("SecurityDecision", "GenericAudit");
    assert_ne!(journal_text, corrupted, "test must corrupt the first line");
    fs::write(&path, corrupted).expect("test can corrupt journal in place");

    let failure = sink
        .query(&DurableAuditReplayQuery {
            family: Some(DurableAuditEventFamily::AdminDecision),
            trace_id: Some(TraceId::new(180)),
            principal_id: Some("user:admin-audit".to_string()),
            ..DurableAuditReplayQuery::all()
        })
        .expect_err("targeted replay must validate unmatched prefix records");

    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure.reason.contains("checksum mismatch")
            || failure.reason.contains("chain checksum mismatch"),
        "replay should fail because an earlier chain record was corrupted"
    );

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
