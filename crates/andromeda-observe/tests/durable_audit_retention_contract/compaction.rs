use crate::support::*;

#[test]
fn durable_audit_compaction_keeps_retained_records_replayable_and_checksummed() {
    let (journal, mut sink) = open_journal("retention-compaction");
    let path = journal.path();
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

    let expired_archive_proof = archive_proof(expired, "backup-archive:expired-segment-10");
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
    let mut reopened = FileDurableAuditWalSink::open(path).expect("compacted journal reopens");
    let replayed = reopened
        .replay(&DurableAuditReplayQuery::all())
        .expect("compacted records replay without checksum corruption");
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[0].report, retained);
    assert_eq!(replayed[1].report, forensic_hold);

    let compacted_text = fs::read_to_string(path).expect("compacted journal can be read");
    let compacted_lines = compacted_text.lines().collect::<Vec<_>>();
    assert_eq!(compacted_lines.len(), 2);
    assert_eq!(
        previous_chain_checksum(compacted_lines[0]),
        0,
        "compaction must start the retained chain from genesis"
    );
    let retained_chain_checksum = chain_checksum(compacted_lines[0]);
    assert_ne!(retained_chain_checksum, 0);
    assert_eq!(
        previous_chain_checksum(compacted_lines[1]),
        retained_chain_checksum,
        "second retained record must point to the rechained predecessor"
    );
    let forensic_chain_checksum = chain_checksum(compacted_lines[1]);
    assert_ne!(forensic_chain_checksum, retained_chain_checksum);

    let compacted_query = reopened
        .replay_with_evidence(
            &DurableAuditReplayQuery::all(),
            DurableAuditReplayWindow::ALL,
        )
        .expect("compacted journal exposes replay evidence");
    assert!(compacted_query.evidence.chain_anchor_present);
    assert_eq!(
        compacted_query.evidence.tail_chain_checksum, forensic_chain_checksum,
        "replay evidence must expose the retained chain tail checksum"
    );

    let expired_query = reopened
        .replay_with_evidence(
            &replay_lsn_query(expired.evidence.record_lsn),
            DurableAuditReplayWindow::ALL,
        )
        .expect("expired LSN replay inspection is valid after compaction");
    assert_eq!(expired_query.evidence.records_scanned, 2);
    assert_eq!(expired_query.evidence.records_returned, 0);
    assert!(expired_query.evidence.filter_applied);
    assert!(expired_query.evidence.chain_anchor_present);
    assert_eq!(
        expired_query.evidence.first_scanned_lsn,
        Some(retained.evidence.record_lsn)
    );
    assert_eq!(
        expired_query.evidence.last_scanned_lsn,
        Some(forensic_hold.evidence.record_lsn)
    );
    assert_ne!(
        expired_query.evidence.tail_chain_checksum, 0,
        "compaction must rechain retained records behind a durable anchor"
    );

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
}

#[test]
fn durable_audit_compaction_retains_wal_segment_without_archive_proof() {
    let (_journal, mut sink) = open_journal("retention-compaction-missing-proof");
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
        .expect("retained WAL segment remains replayable");
    assert_eq!(replayed.len(), 1);
    assert_eq!(replayed[0].report, wal_segment);
}

#[test]
fn durable_audit_compaction_preserves_high_water_lsn_when_all_records_are_prunable() {
    let (_journal, mut sink) = open_journal("retention-compaction-high-water");
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

    let first_archive_proof = archive_proof(first, "backup-archive:high-water-first");
    let second_archive_proof = archive_proof(second, "backup-archive:high-water-second");
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
}
