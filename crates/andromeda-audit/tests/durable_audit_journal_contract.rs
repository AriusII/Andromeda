use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use andromeda_audit::{
    AdminOperation, DurableAuditAppendRecord, DurableAuditDecisionGate, DurableAuditEventFamily,
    DurableAuditFailureKind, DurableAuditPrincipalBinding, DurableAuditPruneBlockReason,
    DurableAuditRecordIdentity, DurableAuditReplayBehavior, DurableAuditReplayLsnRange,
    DurableAuditReplayQuery, DurableAuditReplayRecord, DurableAuditReplayWindow,
    DurableAuditRetentionBoundary, DurableAuditRetentionManager, DurableAuditRetentionPolicy,
    DurableAuditSinkFailure, DurableAuditSinkReport, DurableAuditTraceFamily,
    DurableAuditTraceQueryFilter, DurableAuditTraceQueryLsnRange, DurableAuditTraceQueryRow,
    DurableAuditTraceQuerySource, DurableAuditTraceQuerySpec, DurableAuditWalEvidence,
    DurableAuditWalSegmentArchiveProof, DurableAuditWalSink, EventId, FileDurableAuditWalSink,
    Permission, SecurityPolicyVersionEvidence, SurfaceScope, TraceId,
};
use andromeda_digest::sha256;
use andromeda_types::{RequestId, SessionId};

#[test]
fn audit_owned_replay_filter_rejects_unsafe_inputs() {
    assert!(
        DurableAuditReplayQuery {
            trace_id: Some(TraceId::new(0)),
            ..DurableAuditReplayQuery::all()
        }
        .validate()
        .unwrap_err()
        .message()
        .contains("trace_id")
    );

    assert!(
        DurableAuditReplayQuery {
            principal_id: Some(" \t".to_string()),
            ..DurableAuditReplayQuery::all()
        }
        .validate()
        .unwrap_err()
        .message()
        .contains("principal filter")
    );

    assert!(
        DurableAuditReplayQuery {
            principal_id: Some("private_key=must-not-enter-replay-filter".to_string()),
            ..DurableAuditReplayQuery::all()
        }
        .validate()
        .unwrap_err()
        .message()
        .contains("secret evidence")
    );

    assert!(
        DurableAuditReplayQuery {
            lsn_range: Some(DurableAuditReplayLsnRange::new(20, 10)),
            ..DurableAuditReplayQuery::all()
        }
        .validate()
        .unwrap_err()
        .message()
        .contains("LSN range")
    );
}

#[test]
fn audit_owned_durable_report_and_failure_truth_is_fail_closed() {
    let report = DurableAuditSinkReport {
        identity: identity(11, 90, DurableAuditEventFamily::SecurityDecision, 1),
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
}

#[test]
fn file_backed_audit_owned_sink_survives_restart_and_replays_targeted_index() {
    let journal = TempJournal::new("restart-replay");
    let path = journal.path();
    let mut sink = FileDurableAuditWalSink::open(path).expect("journal opens");
    let report = sink
        .append_durable_audit_record(security_record(
            11,
            90,
            3,
            "user:durable-audit",
            DurableAuditRetentionBoundary::SecurityPolicy,
            DurableAuditReplayBehavior::RebuildDecisionIndex,
        ))
        .expect("append is flushed before success");
    report.validate().expect("report proves durable audit WAL");

    drop(sink);
    let reopened = FileDurableAuditWalSink::open(path).expect("journal reopens after restart");
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
        Some(RequestId::new(11))
    );
    assert_eq!(
        replayed[0].principal_binding.session_id,
        Some(SessionId::new(111))
    );
    assert_eq!(replayed[0].event_kind, "SecurityAudit");

    let journal_text = fs::read_to_string(path).expect("journal is readable");
    assert!(!journal_text.contains("transaction_id="));
    assert!(!journal_text.contains("token="));
}

#[test]
fn file_backed_audit_owned_append_after_reopen_continues_chain() {
    let journal = TempJournal::new("append-after-replay");
    let path = journal.path();
    let mut sink = FileDurableAuditWalSink::open(path).expect("journal opens");
    let first_report = sink
        .append_durable_audit_record(security_record(
            12,
            91,
            71,
            "user:durable-audit",
            DurableAuditRetentionBoundary::SecurityPolicy,
            DurableAuditReplayBehavior::ForensicOnly,
        ))
        .expect("first append succeeds before restart");
    drop(sink);

    let mut reopened = FileDurableAuditWalSink::open(path).expect("journal reopens");
    let replayed = reopened
        .replay(&DurableAuditReplayQuery::all())
        .expect("reopened journal replays before next append");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].report, first_report);

    let second_report = reopened
        .append_durable_audit_record(admin_record(
            72,
            172,
            72,
            AdminOperation::InspectPlans,
            Permission::InspectPlans,
            DurableAuditEventFamily::AdminDecision,
            DurableAuditRetentionBoundary::SecurityPolicy,
        ))
        .expect("append after replay succeeds with continued chain evidence");
    assert_eq!(
        second_report.evidence.record_lsn,
        first_report.evidence.record_lsn + 1
    );

    let replayed_after_append = reopened
        .replay(&DurableAuditReplayQuery::all())
        .expect("journal remains replayable after append continuation");
    assert_eq!(replayed_after_append.len(), 2);
    assert_eq!(replayed_after_append[0].report, first_report);
    assert_eq!(replayed_after_append[1].report, second_report);

    let journal_text = fs::read_to_string(path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 2);
    assert!(lines[1].contains("previous_chain_checksum="));
}

#[test]
fn audit_owned_decision_gate_fails_closed_and_rejects_non_visible_family() {
    let record = admin_record(
        20,
        120,
        20,
        AdminOperation::InspectPlans,
        Permission::InspectPlans,
        DurableAuditEventFamily::AdminDecision,
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    let mut sink = FailingDurableAuditWalSink;
    let failure = DurableAuditDecisionGate::new(DurableAuditEventFamily::AdminDecision)
        .append_and_prove(&mut sink, record)
        .expect_err("visible admin decision fails closed when audit WAL flush fails");

    assert_eq!(failure.kind, DurableAuditFailureKind::WalFlushRejected);
    assert!(failure.requires_fail_closed());

    let journal = TempJournal::new("non-visible-gate-no-append");
    let mut sink = FileDurableAuditWalSink::open(journal.path()).expect("journal opens");
    let failure = DurableAuditDecisionGate::new(DurableAuditEventFamily::AdmissionDecision)
        .append_and_prove(
            &mut sink,
            security_record(
                21,
                121,
                21,
                "user:durable-audit",
                DurableAuditRetentionBoundary::SecurityPolicy,
                DurableAuditReplayBehavior::RebuildDecisionIndex,
            ),
        )
        .expect_err("non-visible decision families must be rejected before append");

    assert_eq!(failure.kind, DurableAuditFailureKind::ValidationRejected);
    assert!(failure.reason.contains("visible decision family"));
    assert!(
        fs::read_to_string(journal.path())
            .expect("journal remains readable")
            .is_empty()
    );

    let missing_flush = DurableAuditSinkReport {
        identity: identity(22, 122, DurableAuditEventFamily::SecurityDecision, 22),
        evidence: DurableAuditWalEvidence {
            record_lsn: 0,
            durable_lsn: 0,
            checksum: 0,
        },
        replay_behavior: DurableAuditReplayBehavior::RebuildDecisionIndex,
        retention: DurableAuditRetentionBoundary::SecurityPolicy,
    };
    let failure = DurableAuditDecisionGate::new(DurableAuditEventFamily::SecurityDecision)
        .prove_visible_decision(missing_flush, principal_binding(22, "user:durable-audit"))
        .expect_err("security allow must not become visible without durable audit evidence");

    assert_eq!(failure.kind, DurableAuditFailureKind::ValidationRejected);
    assert!(failure.requires_fail_closed());
    assert!(failure.reason.contains("flushed WAL evidence"));
}

#[test]
fn audit_owned_permissioned_records_require_policy_evidence() {
    let mut missing_policy = principal_binding(24, "user:admin-audit");
    missing_policy.policy_version = None;
    let err = DurableAuditAppendRecord::new(
        identity(24, 124, DurableAuditEventFamily::AdminDecision, 24),
        missing_policy,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        "AdminOperation",
    )
    .expect_err("permissioned admin durable audit records require policy evidence");
    assert!(
        err.message()
            .contains("permissioned critical AdminDecision records")
    );

    let mut zero_policy = principal_binding(29, "user:admin-audit");
    zero_policy.policy_version = Some(SecurityPolicyVersionEvidence {
        policy_version: 0,
        policy_digest: "sha256:9999999999999999999999999999999999999999999999999999999999999999"
            .to_string(),
    });
    let err = DurableAuditAppendRecord::new(
        identity(29, 129, DurableAuditEventFamily::AdminDecision, 29),
        zero_policy,
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        "AdminOperation",
    )
    .expect_err("zero policy version evidence must fail closed");
    assert!(
        err.message()
            .contains("policy version evidence must be non-zero")
    );

    let observational_recovery = DurableAuditReplayRecord {
        report: DurableAuditSinkReport {
            identity: identity(27, 127, DurableAuditEventFamily::RecoveryDecision, 27),
            evidence: DurableAuditWalEvidence {
                record_lsn: 27,
                durable_lsn: 27,
                checksum: 0xA11D_1701,
            },
            replay_behavior: DurableAuditReplayBehavior::ForensicOnly,
            retention: DurableAuditRetentionBoundary::ForensicHold,
        },
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
            identity: identity(28, 128, DurableAuditEventFamily::RecoveryDecision, 28),
            ..observational_recovery.report
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
fn audit_owned_catalog_publication_requires_audit_wal() {
    let journal = TempJournal::new("catalog-publication-gate");
    let mut sink = FileDurableAuditWalSink::open(journal.path()).expect("journal opens");
    let proof = DurableAuditDecisionGate::new(DurableAuditEventFamily::CatalogDecision)
        .append_and_prove(
            &mut sink,
            catalog_record(142, 242, 22, DurableAuditRetentionBoundary::CatalogVersion),
        )
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
        .replay(&DurableAuditReplayQuery {
            family: Some(DurableAuditEventFamily::CatalogDecision),
            ..DurableAuditReplayQuery::all()
        })
        .expect("catalog publication audit WAL remains replayable");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].report, proof.report);
}

#[test]
fn audit_owned_query_filters_replay_records_without_observe_envelopes() {
    let records = vec![
        replay_record(
            1,
            90,
            DurableAuditEventFamily::SecurityDecision,
            "SecurityAudit",
            "user:durable-audit",
            10,
        ),
        replay_record(
            2,
            91,
            DurableAuditEventFamily::RecoveryDecision,
            "WalFlush",
            "user:storage",
            20,
        ),
        replay_record(
            3,
            92,
            DurableAuditEventFamily::RecoveryDecision,
            "RecoveryStartup",
            "user:storage",
            30,
        ),
    ];
    let source = DurableAuditTraceQuerySource::new(&records);

    let mut spec = DurableAuditTraceQuerySpec::new(DurableAuditTraceQueryFilter {
        family: Some(DurableAuditTraceFamily::Wal),
        principal: Some("user:storage".to_string()),
        lsn_range: Some(DurableAuditTraceQueryLsnRange::new(15, 25)),
        ..DurableAuditTraceQueryFilter::default()
    });
    spec.include_total_count = true;

    let result = source
        .inspect(&spec)
        .expect("durable replay records are inspectable through audit-owned filters");

    assert_eq!(result.metadata.returned_rows, 1);
    assert_eq!(result.metadata.total_matching_rows, Some(1));
    assert_eq!(result.rows[0].event_id, EventId::new(2));
    assert_eq!(result.rows[0].family, DurableAuditTraceFamily::Wal);
    assert_eq!(
        result.rows[0].durable_audit_family,
        DurableAuditEventFamily::RecoveryDecision
    );
}

#[test]
fn audit_owned_query_rejects_unsupported_filters_and_secret_fields() {
    let records = vec![replay_record(
        1,
        90,
        DurableAuditEventFamily::SecurityDecision,
        "SecurityAudit",
        "user:durable-audit",
        10,
    )];
    let source = DurableAuditTraceQuerySource::new(&records);

    let spec = DurableAuditTraceQuerySpec::new(DurableAuditTraceQueryFilter {
        principal: Some("x-api-key must-not-enter-query-path".to_string()),
        ..DurableAuditTraceQueryFilter::default()
    });
    let error = source
        .inspect(&spec)
        .expect_err("durable audit inspection rejects secret-bearing principal filters");
    assert!(
        error
            .message()
            .contains("principal filter must not contain secret evidence")
    );

    let record = replay_record(
        5,
        95,
        DurableAuditEventFamily::SecurityDecision,
        "token=must-not-appear",
        "user:durable-audit",
        50,
    );
    let error = DurableAuditTraceQueryRow::from_replay_record(&record)
        .expect_err("secret-bearing replay evidence must not become an inspection row");
    assert!(error.message().contains("secret evidence"));
    assert!(!error.message().contains("must-not-appear"));
}

#[test]
fn file_backed_audit_owned_replay_detects_corruption_and_missing_anchor() {
    let journal = TempJournal::new("checksum-corruption");
    let path = journal.path();
    let mut sink = FileDurableAuditWalSink::open(path).expect("journal opens");
    sink.append_durable_audit_record(security_record(
        31,
        131,
        31,
        "user:durable-audit",
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
    ))
    .expect("append succeeds before corruption");

    let corrupted = fs::read_to_string(path)
        .expect("journal can be read")
        .replace("SecurityDecision", "GenericAudit");
    fs::write(path, corrupted).expect("test can corrupt journal in place");
    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("checksum corruption is rejected");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);

    let anchor_journal = TempJournal::new("checksum-chain-missing-anchor");
    let anchor_path = anchor_journal.path();
    let mut anchor_sink = FileDurableAuditWalSink::open(anchor_path).expect("journal opens");
    anchor_sink
        .append_durable_audit_record(security_record(
            32,
            132,
            32,
            "user:durable-audit",
            DurableAuditRetentionBoundary::SecurityPolicy,
            DurableAuditReplayBehavior::ForensicOnly,
        ))
        .expect("append writes journal and chain anchor");
    fs::remove_file(journal_chain_anchor_path(anchor_path)).expect("test can remove chain anchor");

    let failure = anchor_sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("non-empty journal without a chain anchor must fail closed");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(failure.reason.contains("chain anchor is required"));
}

#[test]
fn file_backed_audit_owned_replay_rejects_chain_gaps_reorders_and_duplicates() {
    let journal = TempJournal::new("checksum-chain-reorder");
    let path = journal.path();
    let mut sink = FileDurableAuditWalSink::open(path).expect("journal opens");
    sink.append_durable_audit_record(security_record(
        61,
        161,
        61,
        "user:durable-audit",
        DurableAuditRetentionBoundary::SecurityPolicy,
        DurableAuditReplayBehavior::ForensicOnly,
    ))
    .expect("first append succeeds before corruption");
    sink.append_durable_audit_record(admin_record(
        62,
        162,
        62,
        AdminOperation::InspectPlans,
        Permission::InspectPlans,
        DurableAuditEventFamily::AdminDecision,
        DurableAuditRetentionBoundary::SecurityPolicy,
    ))
    .expect("second append succeeds before corruption");

    let journal_text = fs::read_to_string(path).expect("journal can be read");
    let lines = journal_text.lines().collect::<Vec<_>>();
    fs::write(path, format!("{}\n{}\n", lines[1], lines[0]))
        .expect("test can reorder journal records");

    let failure = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("reordered durable audit records break the checksum chain");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("checksum chain previous value mismatch")
    );

    let duplicate_journal = TempJournal::new("checksum-chain-duplicate-lsn");
    let duplicate_path = duplicate_journal.path();
    let mut duplicate_sink = FileDurableAuditWalSink::open(duplicate_path).expect("journal opens");
    duplicate_sink
        .append_durable_audit_record(security_record(
            69,
            169,
            69,
            "user:durable-audit",
            DurableAuditRetentionBoundary::SecurityPolicy,
            DurableAuditReplayBehavior::ForensicOnly,
        ))
        .expect("append succeeds before duplicate LSN corruption");
    let journal_text = fs::read_to_string(duplicate_path).expect("journal can be read");
    let line = journal_text
        .lines()
        .next()
        .expect("journal contains one line");
    let duplicate_payload = journal_payload_from_line(line);
    let duplicate_line = journal_line_with_chain(duplicate_payload, chain_checksum_from_line(line));
    fs::write(duplicate_path, format!("{line}\n{duplicate_line}"))
        .expect("test can append a rechained duplicate LSN");

    let failure = duplicate_sink
        .replay(&DurableAuditReplayQuery::all())
        .expect_err("duplicate LSN must fail even when the checksum chain is syntactically valid");
    assert_eq!(failure.kind, DurableAuditFailureKind::CorruptionDetected);
    assert!(
        failure
            .reason
            .contains("record LSNs must increase strictly")
    );
}

#[test]
fn audit_owned_retention_manager_evaluates_prune_boundaries() {
    let wal_segment = replay_record(
        22,
        122,
        DurableAuditEventFamily::SecurityDecision,
        "SecurityAudit",
        "user:wal-segment-prune",
        22,
    );
    let manager = DurableAuditRetentionManager::new(
        DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(23)
            .with_forensic_hold_preserved(false),
    )
    .expect("retention manager accepts explicit expiry policy");

    let evidence = manager
        .evaluate_prune(&wal_segment, None)
        .expect("missing WAL archive proof produces prune evidence");
    assert!(!evidence.prune_allowed);
    assert_eq!(
        evidence.blocked_by,
        Some(DurableAuditPruneBlockReason::MissingWalSegmentArchiveProof)
    );

    let archive_proof = archive_proof(wal_segment.report, "backup-archive:segment-22");
    let evidence = manager
        .evaluate_prune(&wal_segment, Some(archive_proof.clone()))
        .expect("archived WAL segment prune decision produces evidence");
    assert!(evidence.prune_allowed);
    assert_eq!(evidence.blocked_by, None);
    assert_eq!(evidence.archive_proof, Some(archive_proof));

    let forensic_hold = DurableAuditReplayRecord {
        report: DurableAuditSinkReport {
            retention: DurableAuditRetentionBoundary::ForensicHold,
            ..wal_segment.report
        },
        ..wal_segment
    };
    let evidence = manager
        .evaluate_prune(&forensic_hold, None)
        .expect("forensic hold prune decision produces evidence");
    assert!(!evidence.prune_allowed);
    assert_eq!(
        evidence.blocked_by,
        Some(DurableAuditPruneBlockReason::ForensicHold)
    );
}

#[test]
fn audit_owned_compaction_rechains_retained_records_and_preserves_replay_evidence() {
    let journal = TempJournal::new("retention-compaction");
    let path = journal.path();
    let mut sink = FileDurableAuditWalSink::open(path).expect("journal opens");
    let expired = append_security_record(
        &mut sink,
        10,
        "user:expired",
        DurableAuditRetentionBoundary::WalSegment,
    );
    let retained = append_security_record(
        &mut sink,
        11,
        "user:retained",
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    let forensic_hold = append_security_record(
        &mut sink,
        12,
        "user:forensic",
        DurableAuditRetentionBoundary::ForensicHold,
    );

    let compaction = sink
        .compact_with_archive_proofs(
            &DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(
                retained.evidence.record_lsn,
            ),
            &[archive_proof(expired, "backup-archive:expired-segment-10")],
        )
        .expect("retention compaction rewrites a valid journal");
    assert_eq!(compaction.records_scanned, 3);
    assert_eq!(compaction.records_retained, 2);
    assert_eq!(compaction.records_expired, 1);
    assert_eq!(
        compaction.first_retained_lsn,
        Some(retained.evidence.record_lsn)
    );
    assert_eq!(
        compaction.last_retained_lsn,
        Some(forensic_hold.evidence.record_lsn)
    );
    assert_ne!(compaction.retained_checksum_evidence, 0);

    let replayed = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect("compacted records replay without checksum corruption");
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[0].report, retained);
    assert_eq!(replayed[1].report, forensic_hold);

    let compacted_text = fs::read_to_string(path).expect("compacted journal can be read");
    let compacted_lines = compacted_text.lines().collect::<Vec<_>>();
    assert_eq!(compacted_lines.len(), 2);
    assert_eq!(previous_chain_checksum(compacted_lines[0]), 0);
    let retained_chain_checksum = chain_checksum(compacted_lines[0]);
    assert_eq!(
        previous_chain_checksum(compacted_lines[1]),
        retained_chain_checksum
    );

    let compacted_query = sink
        .replay_with_evidence(
            &DurableAuditReplayQuery::all(),
            DurableAuditReplayWindow::ALL,
        )
        .expect("compacted journal exposes replay evidence");
    assert!(compacted_query.evidence.chain_anchor_present);
    assert_eq!(
        compacted_query.evidence.tail_chain_checksum,
        chain_checksum(compacted_lines[1])
    );

    let appended_after_compaction = append_security_record(
        &mut sink,
        13,
        "user:after-compaction",
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    assert!(appended_after_compaction.evidence.record_lsn > forensic_hold.evidence.record_lsn);
}

#[test]
fn audit_owned_journal_replay_with_evidence_reports_stable_scan_counts() {
    let journal = TempJournal::new("replay-evidence");
    let mut sink = FileDurableAuditWalSink::open(journal.path()).expect("journal opens");
    append_security_record(
        &mut sink,
        1,
        "user:scan-a",
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    let first_match = append_security_record(
        &mut sink,
        2,
        "user:scan-b",
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    append_security_record(
        &mut sink,
        3,
        "user:scan-b",
        DurableAuditRetentionBoundary::ForensicHold,
    );

    let result = sink
        .replay_with_evidence(
            &DurableAuditReplayQuery {
                family: Some(DurableAuditEventFamily::SecurityDecision),
                trace_id: None,
                principal_id: Some("user:scan-b".to_string()),
                lsn_range: None,
            },
            DurableAuditReplayWindow::new(1, 0),
        )
        .expect("durable audit journal replay scans with evidence");

    assert_eq!(result.records.len(), 1);
    assert_eq!(result.evidence.records_scanned, 3);
    assert_eq!(result.evidence.records_matched, 2);
    assert_eq!(result.evidence.records_returned, 1);
    assert!(result.evidence.filter_applied);
    assert_eq!(result.evidence.limit, 1);
    assert_eq!(result.evidence.offset, 0);
    assert!(result.evidence.truncated);
    assert_eq!(
        result.evidence.first_returned_lsn,
        Some(first_match.evidence.record_lsn)
    );
    assert_eq!(
        result.evidence.last_returned_lsn,
        Some(first_match.evidence.record_lsn)
    );
    assert!(result.evidence.chain_anchor_present);
    assert_eq!(result.evidence.first_scanned_lsn, Some(1));
    assert_eq!(result.evidence.last_scanned_lsn, Some(3));
    assert_ne!(result.evidence.tail_chain_checksum, 0);
}

fn identity(
    event_id: u128,
    trace_id: u128,
    family: DurableAuditEventFamily,
    sequence_number: u64,
) -> DurableAuditRecordIdentity {
    DurableAuditRecordIdentity {
        event_id: EventId::new(event_id),
        trace_id: TraceId::new(trace_id),
        family,
        sequence_number,
    }
}

fn principal_binding(event_id: u128, principal_id: &str) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: principal_id.to_string(),
        certificate_fingerprint: Some(format!("sha256:durable-audit-{event_id}")),
        surface: Some(SurfaceScope::Administration),
        permission: Some(Permission::InspectPlans),
        policy_version: Some(SecurityPolicyVersionEvidence::bootstrap_v0()),
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
    }
}

fn security_record(
    event_id: u128,
    trace_id: u128,
    sequence_number: u64,
    principal_id: &str,
    retention: DurableAuditRetentionBoundary,
    replay_behavior: DurableAuditReplayBehavior,
) -> DurableAuditAppendRecord {
    DurableAuditAppendRecord::new(
        identity(
            event_id,
            trace_id,
            DurableAuditEventFamily::SecurityDecision,
            sequence_number,
        ),
        principal_binding(event_id, principal_id),
        retention,
        replay_behavior,
        "SecurityAudit",
    )
    .expect("sample security audit append record is valid")
}

fn admin_record(
    event_id: u128,
    trace_id: u128,
    sequence_number: u64,
    operation: AdminOperation,
    permission: Permission,
    family: DurableAuditEventFamily,
    retention: DurableAuditRetentionBoundary,
) -> DurableAuditAppendRecord {
    let mut binding = principal_binding(event_id, "user:admin-audit");
    binding.permission = Some(permission);
    binding.surface = Some(match operation {
        AdminOperation::ClusterPromote
        | AdminOperation::FenceNode
        | AdminOperation::UpdateClusterManifest => SurfaceScope::Cluster,
        _ => SurfaceScope::Administration,
    });
    DurableAuditAppendRecord::new(
        identity(event_id, trace_id, family, sequence_number),
        binding,
        retention,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        "AdminOperation",
    )
    .expect("sample admin audit append record is valid")
}

fn catalog_record(
    event_id: u128,
    trace_id: u128,
    sequence_number: u64,
    retention: DurableAuditRetentionBoundary,
) -> DurableAuditAppendRecord {
    let mut binding = principal_binding(event_id, "svc:catalog-publisher");
    binding.permission = Some(Permission::ImportDefinitionBatch);
    DurableAuditAppendRecord::new(
        identity(
            event_id,
            trace_id,
            DurableAuditEventFamily::CatalogDecision,
            sequence_number,
        ),
        binding,
        retention,
        DurableAuditReplayBehavior::RebuildDecisionIndex,
        "CatalogMutation",
    )
    .expect("sample catalog audit append record is valid")
}

fn replay_record(
    event_id: u128,
    trace_id: u128,
    family: DurableAuditEventFamily,
    event_kind: &str,
    principal_id: &str,
    record_lsn: u64,
) -> DurableAuditReplayRecord {
    DurableAuditReplayRecord {
        report: DurableAuditSinkReport {
            identity: identity(event_id, trace_id, family, event_id as u64),
            evidence: DurableAuditWalEvidence {
                record_lsn,
                durable_lsn: record_lsn,
                checksum: 0xA11D1700 + record_lsn,
            },
            replay_behavior: DurableAuditReplayBehavior::ForensicOnly,
            retention: DurableAuditRetentionBoundary::WalSegment,
        },
        principal_binding: DurableAuditPrincipalBinding {
            principal_id: principal_id.to_string(),
            certificate_fingerprint: Some(format!("sha256:fingerprint-{event_id}")),
            surface: Some(SurfaceScope::Administration),
            permission: Some(Permission::InspectPlans),
            policy_version: matches!(
                family,
                DurableAuditEventFamily::SecurityDecision
                    | DurableAuditEventFamily::AdminDecision
                    | DurableAuditEventFamily::CatalogDecision
                    | DurableAuditEventFamily::HadrDecision
                    | DurableAuditEventFamily::BackupDecision
                    | DurableAuditEventFamily::RestoreDecision
                    | DurableAuditEventFamily::ForensicDecision
                    | DurableAuditEventFamily::RecoveryDecision
            )
            .then(SecurityPolicyVersionEvidence::bootstrap_v0),
            request_id: Some(RequestId::new(event_id as u64)),
            session_id: Some(SessionId::new((event_id + 100) as u64)),
        },
        event_kind: event_kind.to_string(),
    }
}

fn append_security_record(
    sink: &mut FileDurableAuditWalSink,
    event_id: u128,
    principal_id: &str,
    retention: DurableAuditRetentionBoundary,
) -> DurableAuditSinkReport {
    sink.append_durable_audit_record(security_record(
        event_id,
        event_id + 1_000,
        event_id as u64,
        principal_id,
        retention,
        DurableAuditReplayBehavior::ForensicOnly,
    ))
    .expect("append is flushed before success")
}

fn archive_proof(
    report: DurableAuditSinkReport,
    archive_id: impl Into<String>,
) -> DurableAuditWalSegmentArchiveProof {
    DurableAuditWalSegmentArchiveProof {
        archive_id: archive_id.into(),
        first_lsn: report.evidence.record_lsn,
        last_lsn: report.evidence.durable_lsn,
        checksum: report.evidence.checksum,
    }
}

fn durable_audit_test_checksum64(bytes: &[u8]) -> u64 {
    let digest = sha256(bytes);
    u64::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ])
    .max(1)
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

fn chain_hex_field(line: &str, label: &str) -> u64 {
    let marker = format!("|{label}=");
    let value = line
        .split(&marker)
        .nth(1)
        .and_then(|rest| rest.split('|').next())
        .expect("journal line carries requested chain field");
    u64::from_str_radix(value, 16).expect("chain field is fixed-width hex")
}

fn previous_chain_checksum(line: &str) -> u64 {
    chain_hex_field(line, "previous_chain_checksum")
}

fn chain_checksum(line: &str) -> u64 {
    chain_hex_field(line, "chain_checksum")
}

struct FailingDurableAuditWalSink;

impl DurableAuditWalSink for FailingDurableAuditWalSink {
    fn append_durable_audit_record<R>(
        &mut self,
        record: R,
    ) -> Result<DurableAuditSinkReport, DurableAuditSinkFailure>
    where
        R: Into<DurableAuditAppendRecord>,
    {
        let record = record.into();
        Err(DurableAuditSinkFailure::new(
            DurableAuditFailureKind::WalFlushRejected,
            Some(record.identity),
            "simulated durable audit WAL flush failure",
        )
        .expect("test failure reason is valid"))
    }
}

struct TempJournal {
    path: PathBuf,
}

impl TempJournal {
    fn new(test_name: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after UNIX epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "andromeda-audit-{test_name}-{}-{nonce}.audit",
            std::process::id()
        ));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempJournal {
    fn drop(&mut self) {
        for suffix in ["", ".chain", ".compact.tmp", ".lock"] {
            let mut path = self.path.as_os_str().to_os_string();
            path.push(suffix);
            let _ = fs::remove_file(PathBuf::from(path));
        }
    }
}
