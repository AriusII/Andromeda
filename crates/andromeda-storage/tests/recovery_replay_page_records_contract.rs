//! Page WAL replay promotion-gate contracts.
//!
//! PageAllocate/PageFormat are redo-relevant records, but their durable payload
//! schema and replay apply target are not promoted yet. Recovery must fail
//! closed for both empty and non-empty payloads instead of inferring a format.

use andromeda_core::AndromedaErrorKind;
use andromeda_storage::{
    Lsn, ReplayContext, ReplayOutcome, WalRecord, WalRecordKind, replay_wal_record,
};

#[test]
fn page_allocate_replay_fails_closed_until_payload_and_apply_contract_exist() {
    assert_page_record_fails_closed(
        WalRecordKind::PageAllocate,
        Lsn::new(10),
        Vec::new(),
        "durable PageAllocate payload schema",
    );
    assert_page_record_fails_closed(
        WalRecordKind::PageAllocate,
        Lsn::new(11),
        b"page-id=42;allocation-id=7".to_vec(),
        "durable PageAllocate payload schema",
    );
}

#[test]
fn page_format_replay_fails_closed_until_payload_and_apply_contract_exist() {
    assert_page_record_fails_closed(
        WalRecordKind::PageFormat,
        Lsn::new(20),
        Vec::new(),
        "durable PageFormat payload schema",
    );
    assert_page_record_fails_closed(
        WalRecordKind::PageFormat,
        Lsn::new(21),
        b"page-id=42;format=v1;page-type=fixed-row".to_vec(),
        "durable PageFormat payload schema",
    );
}

fn assert_page_record_fails_closed(
    kind: WalRecordKind,
    lsn: Lsn,
    payload: Vec<u8>,
    missing_contract: &str,
) {
    let record = WalRecord::from_parts(kind, lsn, None, None, payload)
        .expect("page WAL record should be structurally valid");
    let mut ctx = ReplayContext::new();

    let err = replay_wal_record(&mut ctx, &record)
        .expect_err("page replay must fail closed until promoted");

    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert_eq!(ctx.last_replayed_lsn, Some(lsn));
    assert_eq!(ctx.applied_count, 0);
    assert_eq!(ctx.skipped_count, 0);
    assert_eq!(ctx.error_records.len(), 1);

    let error_record = &ctx.error_records[0];
    assert_eq!(error_record.lsn, lsn);
    assert_eq!(error_record.kind, kind);
    assert_eq!(error_record.outcome, ReplayOutcome::NotYetImplemented);
    assert_eq!(
        error_record.error.as_deref(),
        Some(err.message()),
        "returned error and replay context must expose the same fail-closed gate",
    );

    let message = err.message();
    assert!(
        message.contains(&format!("{kind:?}")),
        "gate error must identify the record kind: {message}"
    );
    assert!(
        message.contains("not promoted"),
        "gate error must identify the promotion boundary: {message}"
    );
    assert!(
        message.contains(missing_contract),
        "gate error must document the missing page replay contract: {message}"
    );
    assert!(
        message.contains("fail closed") && message.contains("idempotent"),
        "gate error must reject inferred formats and require idempotent replay: {message}"
    );
}
