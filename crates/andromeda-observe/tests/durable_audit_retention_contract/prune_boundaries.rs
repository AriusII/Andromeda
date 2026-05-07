use crate::support::*;

#[test]
fn forensic_hold_blocks_prune() {
    let (_journal, mut sink) = open_journal("forensic-hold-prune");
    let report = append_security_record(
        &mut sink,
        20,
        "user:forensic-prune",
        DurableAuditRetentionBoundary::ForensicHold,
    );
    let record = replay_record_for_report(&sink, report);
    let manager = retention_manager_after_record_lsn(report.evidence.record_lsn);

    let evidence = manager
        .evaluate_prune(&record, None)
        .expect("forensic hold prune decision produces evidence");

    assert!(!evidence.prune_allowed);
    assert_eq!(
        evidence.blocked_by,
        Some(DurableAuditPruneBlockReason::ForensicHold)
    );
    assert_eq!(evidence.record_lsn, report.evidence.record_lsn);
}

#[test]
fn security_policy_retention_blocks_prune() {
    let (_journal, mut sink) = open_journal("security-policy-prune");
    let report = append_security_record(
        &mut sink,
        21,
        "user:security-policy-prune",
        DurableAuditRetentionBoundary::SecurityPolicy,
    );
    let record = replay_record_for_report(&sink, report);
    let manager = retention_manager_after_record_lsn(report.evidence.record_lsn);

    let evidence = manager
        .evaluate_prune(&record, None)
        .expect("security policy prune decision produces evidence");

    assert!(!evidence.prune_allowed);
    assert_eq!(
        evidence.blocked_by,
        Some(DurableAuditPruneBlockReason::SecurityPolicy)
    );
    assert_eq!(evidence.record_checksum, report.evidence.checksum);
}

#[test]
fn catalog_version_retention_blocks_prune_without_catalog_retention_proof() {
    let (_journal, mut sink) = open_journal("catalog-version-prune");
    let report = append_security_record(
        &mut sink,
        25,
        "user:catalog-version-prune",
        DurableAuditRetentionBoundary::CatalogVersion,
    );
    let record = replay_record_for_report(&sink, report);
    let manager = retention_manager_after_record_lsn(report.evidence.record_lsn);

    let evidence = manager
        .evaluate_prune(&record, None)
        .expect("catalog retention boundary produces prune evidence");

    assert!(!evidence.prune_allowed);
    assert_eq!(
        evidence.blocked_by,
        Some(DurableAuditPruneBlockReason::CatalogVersionRetentionBoundary)
    );
}

#[test]
fn wal_segment_retention_allows_prune_after_archive() {
    let (_journal, mut sink) = open_journal("wal-segment-prune");
    let report = append_security_record(
        &mut sink,
        22,
        "user:wal-segment-prune",
        DurableAuditRetentionBoundary::WalSegment,
    );
    let record = replay_record_for_report(&sink, report);
    let manager = retention_manager_after_record_lsn(report.evidence.record_lsn);
    let archive_proof = archive_proof(report, "backup-archive:segment-22");

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
}

#[test]
fn wal_segment_retention_blocks_prune_without_archive_proof() {
    let (_journal, mut sink) = open_journal("wal-segment-prune-missing-proof");
    let report = append_security_record(
        &mut sink,
        23,
        "user:wal-segment-missing-proof",
        DurableAuditRetentionBoundary::WalSegment,
    );
    let record = replay_record_for_report(&sink, report);
    let manager = retention_manager_after_record_lsn(report.evidence.record_lsn);

    let evidence = manager
        .evaluate_prune(&record, None)
        .expect("missing WAL archive proof produces prune evidence");

    assert!(!evidence.prune_allowed);
    assert_eq!(
        evidence.blocked_by,
        Some(DurableAuditPruneBlockReason::MissingWalSegmentArchiveProof)
    );
}

#[test]
fn wal_segment_retention_blocks_prune_when_archive_checksum_does_not_match_record() {
    let (_journal, mut sink) = open_journal("wal-segment-prune-checksum-mismatch");
    let report = append_security_record(
        &mut sink,
        24,
        "user:wal-segment-checksum-mismatch",
        DurableAuditRetentionBoundary::WalSegment,
    );
    let record = replay_record_for_report(&sink, report);
    let manager = retention_manager_after_record_lsn(report.evidence.record_lsn);
    let archive_proof = archive_proof_with_checksum(
        report,
        "backup-archive:segment-24",
        report.evidence.checksum.wrapping_add(1),
    );

    let evidence = manager
        .evaluate_prune(&record, Some(archive_proof))
        .expect("checksum mismatch produces prune evidence");

    assert!(!evidence.prune_allowed);
    assert_eq!(
        evidence.blocked_by,
        Some(DurableAuditPruneBlockReason::WalSegmentArchiveProofMismatch)
    );
}
