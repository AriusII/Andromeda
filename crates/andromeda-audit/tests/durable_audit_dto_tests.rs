use andromeda_audit::{
    DurableAuditEventFamily, DurableAuditFailureKind, DurableAuditReplayLsnRange,
    DurableAuditReplayQuery, DurableAuditReplayWindow, DurableAuditWalEvidence,
};
use andromeda_observability::TraceId;

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
