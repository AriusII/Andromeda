use crate::support::*;

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
        .replay(&DurableAuditReplayQuery {
            family: Some(DurableAuditEventFamily::CatalogDecision),
            ..DurableAuditReplayQuery::all()
        })
        .expect("catalog publication audit WAL remains replayable");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].report, proof.report);

    let _ = fs::remove_file(path);
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
