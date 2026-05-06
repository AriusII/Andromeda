use andromeda_core::{RequestId, SessionId};
use andromeda_observe::{
    CertificateIdentity, DurableAuditEventFamily, DurableAuditPrincipalBinding,
    DurableAuditPruneBlockReason, DurableAuditReplayBehavior, DurableAuditReplayLsnRange,
    DurableAuditReplayQuery, DurableAuditReplayRecord, DurableAuditReplayWindow,
    DurableAuditRetentionBoundary, DurableAuditRetentionManager, DurableAuditRetentionPolicy,
    DurableAuditWalSegmentArchiveProof, DurableAuditWalSink, EventCorrelation, EventEnvelope,
    EventId, FileDurableAuditWalSink, PendingDurableAuditRecord, Permission, SecurityAuditOutcome,
    SecurityAuditTrace, SurfaceScope, TraceEvent, TraceId, UserPrincipal, UserPrincipalKind,
};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn request_correlation(event_id: u128) -> EventCorrelation {
    EventCorrelation {
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
        contract_hash: None,
        catalog_version: None,
        catalog_object_id: None,
        transaction_id: None,
        durable_lsn: None,
        protocol: None,
    }
}

fn security_envelope(event_id: u128, trace_id: u128, principal_id: &str) -> EventEnvelope {
    let certificate = CertificateIdentity::new(
        format!("sha256:durable-audit-retention-{event_id}"),
        "CN=durable-audit-retention",
        SurfaceScope::Administration,
    )
    .expect("test certificate has explicit non-secret evidence");
    let principal = UserPrincipal::new(principal_id, UserPrincipalKind::Human)
        .expect("test principal has explicit evidence");
    let trace = SecurityAuditTrace::new(
        TraceId::new(trace_id),
        SurfaceScope::Administration,
        certificate,
        principal,
        Permission::InspectPlans,
        SecurityAuditOutcome::Allowed,
        format!("durable audit retention event {event_id}"),
    )
    .expect("security audit trace has explicit reason");

    EventEnvelope::new(
        EventId::new(event_id),
        request_correlation(event_id),
        TraceEvent::SecurityAudit(trace),
    )
    .expect("security audit envelope is valid")
}

fn principal_binding(event_id: u128, principal_id: &str) -> DurableAuditPrincipalBinding {
    DurableAuditPrincipalBinding {
        principal_id: principal_id.to_string(),
        certificate_fingerprint: Some(format!("sha256:durable-audit-retention-{event_id}")),
        surface: Some(SurfaceScope::Administration),
        permission: Some(Permission::InspectPlans),
        request_id: Some(RequestId::new(event_id as u64)),
        session_id: Some(SessionId::new(event_id as u64 + 100)),
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

fn append_security_record(
    sink: &mut FileDurableAuditWalSink,
    event_id: u128,
    principal_id: &str,
    retention: DurableAuditRetentionBoundary,
) -> andromeda_observe::DurableAuditSinkReport {
    let record = PendingDurableAuditRecord::new(
        event_id as u64,
        principal_binding(event_id, principal_id),
        retention,
        DurableAuditReplayBehavior::ForensicOnly,
        security_envelope(event_id, event_id + 1_000, principal_id),
    )
    .expect("durable audit record contract is satisfied");

    sink.append_durable_audit_record(record)
        .expect("append is flushed before success")
}

fn replay_record_for_report(
    sink: &FileDurableAuditWalSink,
    report: andromeda_observe::DurableAuditSinkReport,
) -> DurableAuditReplayRecord {
    sink.replay(&DurableAuditReplayQuery {
        lsn_range: Some(DurableAuditReplayLsnRange::new(
            report.evidence.record_lsn,
            report.evidence.record_lsn,
        )),
        ..DurableAuditReplayQuery::all()
    })
    .expect("record can be replayed by durable LSN")
    .into_iter()
    .find(|record| record.report == report)
    .expect("appended report has matching replay record")
}

#[test]
fn durable_audit_journal_query_with_evidence_reports_stable_scan_counts() {
    let path = temp_journal_path("query-evidence");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
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
        .query_with_evidence(
            &DurableAuditReplayQuery {
                family: Some(DurableAuditEventFamily::SecurityDecision),
                trace_id: None,
                principal_id: Some("user:scan-b".to_string()),
                lsn_range: None,
            },
            DurableAuditReplayWindow::new(1, 0),
        )
        .expect("durable audit journal query scans with evidence");

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

    let _ = fs::remove_file(path);
}

#[test]
fn forensic_hold_blocks_prune() {
    let path = temp_journal_path("forensic-hold-prune");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let report = append_security_record(
        &mut sink,
        20,
        "user:forensic-prune",
        DurableAuditRetentionBoundary::ForensicHold,
    );
    let record = replay_record_for_report(&sink, report);
    let manager = DurableAuditRetentionManager::new(
        DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(report.evidence.record_lsn + 1)
            .with_forensic_hold_preserved(false),
    )
    .expect("retention manager accepts explicit expiry policy");

    let evidence = manager
        .evaluate_prune(&record, None)
        .expect("forensic hold prune decision produces evidence");

    assert!(!evidence.prune_allowed);
    assert_eq!(
        evidence.blocked_by,
        Some(DurableAuditPruneBlockReason::ForensicHold)
    );
    assert_eq!(evidence.record_lsn, report.evidence.record_lsn);

    let _ = fs::remove_file(path);
}

#[test]
fn security_policy_retention_blocks_prune() {
    let path = temp_journal_path("security-policy-prune");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let report = append_security_record(
        &mut sink,
        21,
        "user:security-policy-prune",
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    let record = replay_record_for_report(&sink, report);
    let manager = DurableAuditRetentionManager::new(
        DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(report.evidence.record_lsn + 1)
            .with_forensic_hold_preserved(false),
    )
    .expect("retention manager accepts explicit expiry policy");

    let evidence = manager
        .evaluate_prune(&record, None)
        .expect("security policy prune decision produces evidence");

    assert!(!evidence.prune_allowed);
    assert_eq!(
        evidence.blocked_by,
        Some(DurableAuditPruneBlockReason::SecurityPolicy)
    );
    assert_eq!(evidence.record_checksum, report.evidence.checksum);

    let _ = fs::remove_file(path);
}

#[test]
fn catalog_version_retention_blocks_prune_without_catalog_retention_proof() {
    let path = temp_journal_path("catalog-version-prune");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let report = append_security_record(
        &mut sink,
        25,
        "user:catalog-version-prune",
        DurableAuditRetentionBoundary::CatalogVersion,
    );
    let record = replay_record_for_report(&sink, report);
    let manager = DurableAuditRetentionManager::new(
        DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(report.evidence.record_lsn + 1)
            .with_forensic_hold_preserved(false),
    )
    .expect("retention manager accepts explicit expiry policy");

    let evidence = manager
        .evaluate_prune(&record, None)
        .expect("catalog retention boundary produces prune evidence");

    assert!(!evidence.prune_allowed);
    assert_eq!(
        evidence.blocked_by,
        Some(DurableAuditPruneBlockReason::CatalogVersionRetentionBoundary)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn wal_segment_retention_allows_prune_after_archive() {
    let path = temp_journal_path("wal-segment-prune");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let report = append_security_record(
        &mut sink,
        22,
        "user:wal-segment-prune",
        DurableAuditRetentionBoundary::WalSegment,
    );
    let record = replay_record_for_report(&sink, report);
    let manager = DurableAuditRetentionManager::new(
        DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(report.evidence.record_lsn + 1)
            .with_forensic_hold_preserved(false),
    )
    .expect("retention manager accepts explicit expiry policy");
    let archive_proof = DurableAuditWalSegmentArchiveProof {
        archive_id: "backup-archive:segment-22".to_string(),
        first_lsn: report.evidence.record_lsn,
        last_lsn: report.evidence.durable_lsn,
        checksum: report.evidence.checksum,
    };

    let evidence = manager
        .evaluate_prune(&record, Some(archive_proof.clone()))
        .expect("archived WAL segment prune decision produces evidence");

    assert!(evidence.prune_allowed);
    assert_eq!(evidence.blocked_by, None);
    assert_eq!(evidence.archive_proof, Some(archive_proof));
    assert_eq!(
        evidence.retention,
        DurableAuditRetentionBoundary::WalSegment
    );

    let _ = fs::remove_file(path);
}

#[test]
fn wal_segment_retention_blocks_prune_without_archive_proof() {
    let path = temp_journal_path("wal-segment-prune-missing-proof");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let report = append_security_record(
        &mut sink,
        23,
        "user:wal-segment-missing-proof",
        DurableAuditRetentionBoundary::WalSegment,
    );
    let record = replay_record_for_report(&sink, report);
    let manager = DurableAuditRetentionManager::new(
        DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(report.evidence.record_lsn + 1)
            .with_forensic_hold_preserved(false),
    )
    .expect("retention manager accepts explicit expiry policy");

    let evidence = manager
        .evaluate_prune(&record, None)
        .expect("missing WAL archive proof produces prune evidence");

    assert!(!evidence.prune_allowed);
    assert_eq!(
        evidence.blocked_by,
        Some(DurableAuditPruneBlockReason::MissingWalSegmentArchiveProof)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn wal_segment_retention_blocks_prune_when_archive_checksum_does_not_match_record() {
    let path = temp_journal_path("wal-segment-prune-checksum-mismatch");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let report = append_security_record(
        &mut sink,
        24,
        "user:wal-segment-checksum-mismatch",
        DurableAuditRetentionBoundary::WalSegment,
    );
    let record = replay_record_for_report(&sink, report);
    let manager = DurableAuditRetentionManager::new(
        DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(report.evidence.record_lsn + 1)
            .with_forensic_hold_preserved(false),
    )
    .expect("retention manager accepts explicit expiry policy");
    let archive_proof = DurableAuditWalSegmentArchiveProof {
        archive_id: "backup-archive:segment-24".to_string(),
        first_lsn: report.evidence.record_lsn,
        last_lsn: report.evidence.durable_lsn,
        checksum: report.evidence.checksum.wrapping_add(1),
    };

    let evidence = manager
        .evaluate_prune(&record, Some(archive_proof))
        .expect("checksum mismatch produces prune evidence");

    assert!(!evidence.prune_allowed);
    assert_eq!(
        evidence.blocked_by,
        Some(DurableAuditPruneBlockReason::WalSegmentArchiveProofMismatch)
    );

    let _ = fs::remove_file(path);
}

#[test]
fn durable_audit_compaction_keeps_retained_records_queryable_and_checksummed() {
    let path = temp_journal_path("retention-compaction");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
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

    let expired_archive_proof = DurableAuditWalSegmentArchiveProof {
        archive_id: "backup-archive:expired-segment-10".to_string(),
        first_lsn: expired.evidence.record_lsn,
        last_lsn: expired.evidence.durable_lsn,
        checksum: expired.evidence.checksum,
    };
    let compaction = sink
        .compact_with_archive_proofs(
            &DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(
                retained.evidence.record_lsn,
            ),
            &[expired_archive_proof],
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

    drop(sink);
    let mut reopened = FileDurableAuditWalSink::open(&path).expect("compacted journal reopens");
    let replayed = reopened
        .replay(&DurableAuditReplayQuery::all())
        .expect("compacted records replay without checksum corruption");
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[0].report, retained);
    assert_eq!(replayed[1].report, forensic_hold);

    let expired_query = reopened
        .query_with_evidence(
            &DurableAuditReplayQuery {
                lsn_range: Some(DurableAuditReplayLsnRange::new(
                    expired.evidence.record_lsn,
                    expired.evidence.record_lsn,
                )),
                ..DurableAuditReplayQuery::all()
            },
            DurableAuditReplayWindow::ALL,
        )
        .expect("expired LSN query is valid after compaction");
    assert_eq!(expired_query.evidence.records_scanned, 2);
    assert_eq!(expired_query.evidence.records_returned, 0);
    assert!(expired_query.evidence.filter_applied);

    let appended_after_compaction = append_security_record(
        &mut reopened,
        13,
        "user:after-compaction",
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    assert!(
        appended_after_compaction.evidence.record_lsn > forensic_hold.evidence.record_lsn,
        "next durable audit LSN must not regress after compaction"
    );

    let replayed_after_append = reopened
        .replay(&DurableAuditReplayQuery::all())
        .expect("journal remains replayable after post-compaction append");
    assert_eq!(replayed_after_append.len(), 3);

    let _ = fs::remove_file(path);
}

#[test]
fn durable_audit_compaction_retains_wal_segment_without_archive_proof() {
    let path = temp_journal_path("retention-compaction-missing-proof");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let wal_segment = append_security_record(
        &mut sink,
        30,
        "user:wal-segment-retained-without-proof",
        DurableAuditRetentionBoundary::WalSegment,
    );

    let compaction = sink
        .compact(&DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(
            wal_segment.evidence.record_lsn + 1,
        ))
        .expect("retention compaction keeps WAL segment without archive proof");

    assert_eq!(compaction.records_scanned, 1);
    assert_eq!(compaction.records_retained, 1);
    assert_eq!(compaction.records_expired, 0);

    let replayed = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect("retained WAL segment remains queryable");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].report, wal_segment);

    let _ = fs::remove_file(path);
}

#[test]
fn durable_audit_compaction_preserves_high_water_lsn_when_all_records_are_prunable() {
    let path = temp_journal_path("retention-compaction-high-water");
    let mut sink = FileDurableAuditWalSink::open(&path).expect("journal opens");
    let first = append_security_record(
        &mut sink,
        40,
        "user:high-water-first",
        DurableAuditRetentionBoundary::WalSegment,
    );
    let second = append_security_record(
        &mut sink,
        41,
        "user:high-water-second",
        DurableAuditRetentionBoundary::WalSegment,
    );

    let first_archive_proof = DurableAuditWalSegmentArchiveProof {
        archive_id: "backup-archive:high-water-first".to_string(),
        first_lsn: first.evidence.record_lsn,
        last_lsn: first.evidence.durable_lsn,
        checksum: first.evidence.checksum,
    };
    let second_archive_proof = DurableAuditWalSegmentArchiveProof {
        archive_id: "backup-archive:high-water-second".to_string(),
        first_lsn: second.evidence.record_lsn,
        last_lsn: second.evidence.durable_lsn,
        checksum: second.evidence.checksum,
    };
    let compaction = sink
        .compact_with_archive_proofs(
            &DurableAuditRetentionPolicy::retain_record_lsn_at_or_after(
                second.evidence.record_lsn + 1,
            ),
            &[first_archive_proof, second_archive_proof],
        )
        .expect("retention compaction preserves durable audit high-water evidence");

    assert_eq!(compaction.records_scanned, 2);
    assert_eq!(compaction.records_retained, 1);
    assert_eq!(compaction.records_expired, 1);
    assert_eq!(
        compaction.first_retained_lsn,
        Some(second.evidence.record_lsn)
    );
    assert_eq!(
        compaction.last_retained_lsn,
        Some(second.evidence.record_lsn)
    );

    let replayed = sink
        .replay(&DurableAuditReplayQuery::all())
        .expect("high-water record remains replayable after compaction");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].report, second);

    let appended_after_compaction = append_security_record(
        &mut sink,
        42,
        "user:after-high-water-compaction",
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    assert_eq!(
        appended_after_compaction.evidence.record_lsn,
        second.evidence.record_lsn + 1,
        "next durable audit LSN must continue from retained high-water evidence"
    );

    let _ = fs::remove_file(path);
}
