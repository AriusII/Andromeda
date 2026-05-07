use super::support::{
    ALL_WAL_RECORD_KINDS, FUTURE_WORK_KINDS, SKIPPED_OR_IMPLEMENTED_KINDS,
    is_index_btree_recovery_kind, record_for_kind,
};
use andromeda_storage::{Lsn, ReplayContext, ReplayOutcome, WalRecordKind, replay_wal_record};

#[test]
fn all_record_kinds_are_classified_exactly_once() {
    assert_eq!(ALL_WAL_RECORD_KINDS.len(), 26);
    assert_eq!(SKIPPED_OR_IMPLEMENTED_KINDS.len(), 12);
    assert_eq!(FUTURE_WORK_KINDS.len(), 14);

    for kind in &ALL_WAL_RECORD_KINDS {
        let kind_name = format!("{kind:?}");
        let handled = SKIPPED_OR_IMPLEMENTED_KINDS.contains(kind);
        let deferred = FUTURE_WORK_KINDS.contains(kind);
        assert_ne!(
            handled, deferred,
            "{kind_name} must be classified as exactly one recovery category",
        );
    }
}

#[test]
fn marker_and_boundary_records_are_skipped_without_errors() {
    let skipped_kinds = [
        WalRecordKind::TxBegin,
        WalRecordKind::TxCommit,
        WalRecordKind::TxRollback,
        WalRecordKind::CheckpointBegin,
        WalRecordKind::CheckpointEnd,
        WalRecordKind::SnapshotBegin,
        WalRecordKind::SnapshotEnd,
        WalRecordKind::SecurityAuditAppend,
    ];

    let mut ctx = ReplayContext::new();
    for (idx, kind) in skipped_kinds.iter().copied().enumerate() {
        let record = record_for_kind(kind, Lsn::new(idx as u64 + 1));
        replay_wal_record(&mut ctx, &record).expect("skipped handler should not fail");
    }

    assert_eq!(ctx.applied_count, 0);
    assert_eq!(ctx.skipped_count, skipped_kinds.len());
    assert!(!ctx.has_errors());
    assert_eq!(
        ctx.last_replayed_lsn,
        Some(Lsn::new(skipped_kinds.len() as u64))
    );
}

#[test]
fn future_work_records_fail_stop_with_clear_error_and_context() {
    for (idx, kind) in FUTURE_WORK_KINDS.into_iter().enumerate() {
        let mut ctx = ReplayContext::new();
        let record = record_for_kind(kind, Lsn::new(idx as u64 + 10));
        let err =
            replay_wal_record(&mut ctx, &record).expect_err("future work handlers must fail-stop");
        let message = err.message();

        if is_index_btree_recovery_kind(kind) {
            assert!(
                message.contains("malformed"),
                "{kind:?} error must identify malformed rebuild evidence payloads"
            );
        } else {
            assert!(
                message.contains("not promoted"),
                "{kind:?} error must identify the recovery promotion gate"
            );
        }
        assert!(
            message.contains(&format!("{kind:?}")),
            "{kind:?} error must include record kind"
        );
        assert!(ctx.has_errors());
        assert_eq!(ctx.error_records.len(), 1);
        assert_eq!(ctx.error_records[0].kind, kind);
        assert_eq!(
            ctx.error_records[0].outcome,
            ReplayOutcome::NotYetImplemented
        );
    }
}
