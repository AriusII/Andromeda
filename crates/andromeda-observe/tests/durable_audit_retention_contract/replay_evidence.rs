use crate::support::*;

#[test]
fn durable_audit_journal_replay_with_evidence_reports_stable_scan_counts() {
    let (_journal, mut sink) = open_journal("replay-evidence");
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
