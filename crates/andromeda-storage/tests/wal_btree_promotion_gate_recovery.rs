//! B-Tree WAL record recovery promotion-gate tests.
//!
//! This module validates that B-Tree WAL record types are accepted as
//! cataloged WAL variants and that recovery handlers fail-stop with a clear
//! "not promoted" error while durable page-backed B-Tree replay is gated.
//!
//! # Scope
//!
//! - Verify BTreeInsert, BTreeDelete, BTreeSplit, BTreeMerge variants exist
//! - Verify these variants are mapped to tags 23-26
//! - Verify recovery handlers return promotion-gate errors with clear messages
//! - Verify B-Tree records require transaction IDs (like other mutations)
//! - Verify non-B-Tree records are unaffected by B-Tree changes
//! - Verify error messages include durable replay promotion context

use andromeda_core::{AndromedaErrorKind, TransactionId};
use andromeda_storage::{
    Lsn, ReplayContext, ReplayOutcome, WalRecord, WalRecordKind, encode_wal_record,
    replay_wal_record, wal_record_kind_from_tag, wal_record_kind_tag,
};

fn assert_btree_replay_stops_at_promotion_gate(kind: WalRecordKind, lsn: Lsn, payload: &[u8]) {
    let tx_id = TransactionId::new(1000 + lsn.get());
    let record = WalRecord::from_parts(kind, lsn, None, Some(tx_id), payload.to_vec())
        .expect("valid B-Tree WAL record");

    let mut ctx = ReplayContext::new();
    let error = replay_wal_record(&mut ctx, &record).expect_err("B-Tree replay must fail-stop");

    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert_eq!(ctx.last_replayed_lsn, Some(lsn));
    assert_eq!(ctx.applied_count, 0);
    assert_eq!(ctx.skipped_count, 0);
    assert_eq!(ctx.error_records.len(), 1);

    let error_record = &ctx.error_records[0];
    assert_eq!(error_record.lsn, lsn);
    assert_eq!(error_record.kind, kind);
    assert_eq!(error_record.outcome, ReplayOutcome::NotYetImplemented);

    let stored_message = error_record.error.as_deref().expect("stored replay error");
    assert_eq!(error.message(), stored_message);
    assert!(
        stored_message.contains(&format!("{kind:?}")),
        "promotion-gate error should identify the record kind: {stored_message}"
    );
    assert!(
        stored_message.contains("not promoted") && stored_message.contains("idempotent"),
        "promotion-gate error should explain why replay is gated: {stored_message}"
    );
}

#[test]
fn btree_variants_exist_in_wal_record_kind() {
    // Verify that all four B-Tree variants exist
    let btree_insert = WalRecordKind::BTreeInsert;
    let btree_delete = WalRecordKind::BTreeDelete;
    let btree_split = WalRecordKind::BTreeSplit;
    let btree_merge = WalRecordKind::BTreeMerge;

    // Variants should be distinct
    assert_ne!(btree_insert, btree_delete);
    assert_ne!(btree_delete, btree_split);
    assert_ne!(btree_split, btree_merge);
    assert_ne!(btree_insert, btree_merge);
}

#[test]
fn btree_variants_have_correct_tags() {
    // Verify that B-Tree variants are mapped to tags 23-26
    assert_eq!(wal_record_kind_tag(WalRecordKind::BTreeInsert), 23);
    assert_eq!(wal_record_kind_tag(WalRecordKind::BTreeDelete), 24);
    assert_eq!(wal_record_kind_tag(WalRecordKind::BTreeSplit), 25);
    assert_eq!(wal_record_kind_tag(WalRecordKind::BTreeMerge), 26);
}

#[test]
fn btree_tags_reconstruct_to_correct_variants() {
    // Verify reverse mapping from tags to variants
    assert_eq!(
        wal_record_kind_from_tag(23),
        Some(WalRecordKind::BTreeInsert)
    );
    assert_eq!(
        wal_record_kind_from_tag(24),
        Some(WalRecordKind::BTreeDelete)
    );
    assert_eq!(
        wal_record_kind_from_tag(25),
        Some(WalRecordKind::BTreeSplit)
    );
    assert_eq!(
        wal_record_kind_from_tag(26),
        Some(WalRecordKind::BTreeMerge)
    );
}

#[test]
fn btree_variants_require_transaction_id() {
    // Verify that B-Tree mutations require transaction IDs (like other mutations)
    assert!(WalRecordKind::BTreeInsert.requires_transaction_id());
    assert!(WalRecordKind::BTreeDelete.requires_transaction_id());
    assert!(WalRecordKind::BTreeSplit.requires_transaction_id());
    assert!(WalRecordKind::BTreeMerge.requires_transaction_id());

    // Verify non-mutation variants don't require transaction ID
    assert!(!WalRecordKind::CheckpointBegin.requires_transaction_id());
    assert!(!WalRecordKind::ManifestSwitch.requires_transaction_id());
}

#[test]
fn btree_variants_are_not_redo_relevant_until_durable_replay_is_promoted() {
    // B-Tree records are not redo-relevant while durable page-backed recovery
    // handlers are gated behind promotion.
    assert!(!WalRecordKind::BTreeInsert.is_redo_relevant());
    assert!(!WalRecordKind::BTreeDelete.is_redo_relevant());
    assert!(!WalRecordKind::BTreeSplit.is_redo_relevant());
    assert!(!WalRecordKind::BTreeMerge.is_redo_relevant());

    // Verify existing mutations still are redo-relevant
    assert!(WalRecordKind::RowInsert.is_redo_relevant());
    assert!(WalRecordKind::PageAllocate.is_redo_relevant());
}

#[test]
fn btree_insert_record_can_be_created_with_transaction_id() {
    let tx_id = TransactionId::new(42);
    let record = WalRecord::from_parts(
        WalRecordKind::BTreeInsert,
        Lsn::new(1),
        None,
        Some(tx_id),
        b"btree-payload".to_vec(),
    )
    .expect("BTreeInsert with transaction id should validate");

    assert_eq!(record.header.kind, WalRecordKind::BTreeInsert);
    assert_eq!(record.header.transaction_id, Some(tx_id));
    assert_eq!(record.payload(), b"btree-payload");
}

#[test]
fn btree_record_without_transaction_id_fails_validation() {
    // B-Tree mutations require transaction IDs, so records without them should fail
    let record_result = WalRecord::from_parts(
        WalRecordKind::BTreeInsert,
        Lsn::new(1),
        None,
        None,
        b"btree-payload".to_vec(),
    );

    let error = record_result.expect_err("B-Tree mutations require transaction ids");
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert!(error.message().contains("transaction id"));
}

#[test]
fn btree_insert_record_survives_encoding_roundtrip() {
    // Verify that BTreeInsert records can be encoded and decoded without loss
    let tx_id = TransactionId::new(100);
    let original = WalRecord::from_parts(
        WalRecordKind::BTreeInsert,
        Lsn::new(5),
        Some(Lsn::new(4)),
        Some(tx_id),
        b"test-btree-data".to_vec(),
    )
    .unwrap();

    let encoded = encode_wal_record(&original).expect("encode failed");
    assert!(!encoded.is_empty());

    // Verify the encoded data can be decoded
    let decoded = andromeda_storage::decode_wal_record_frame(&encoded).expect("decode failed");
    assert!(decoded.is_some());

    let (recovered_record, _) = decoded.unwrap();
    assert_eq!(recovered_record.header.kind, WalRecordKind::BTreeInsert);
    assert_eq!(recovered_record.header.lsn, Lsn::new(5));
    assert_eq!(recovered_record.header.transaction_id, Some(tx_id));
}

#[test]
fn btree_delete_record_survives_encoding_roundtrip() {
    let tx_id = TransactionId::new(101);
    let original = WalRecord::from_parts(
        WalRecordKind::BTreeDelete,
        Lsn::new(10),
        Some(Lsn::new(9)),
        Some(tx_id),
        b"key-to-delete".to_vec(),
    )
    .unwrap();

    let encoded = encode_wal_record(&original).expect("encode failed");
    let decoded = andromeda_storage::decode_wal_record_frame(&encoded).expect("decode failed");
    assert!(decoded.is_some());

    let (recovered_record, _) = decoded.unwrap();
    assert_eq!(recovered_record.header.kind, WalRecordKind::BTreeDelete);
    assert_eq!(recovered_record.header.lsn, Lsn::new(10));
}

#[test]
fn recovery_btree_insert_returns_promotion_gate_error() {
    assert_btree_replay_stops_at_promotion_gate(
        WalRecordKind::BTreeInsert,
        Lsn::new(1),
        b"insert-data",
    );
}

#[test]
fn recovery_btree_delete_returns_promotion_gate_error() {
    assert_btree_replay_stops_at_promotion_gate(
        WalRecordKind::BTreeDelete,
        Lsn::new(2),
        b"delete-data",
    );
}

#[test]
fn recovery_btree_split_returns_promotion_gate_error() {
    assert_btree_replay_stops_at_promotion_gate(
        WalRecordKind::BTreeSplit,
        Lsn::new(3),
        b"split-data",
    );
}

#[test]
fn recovery_btree_merge_returns_promotion_gate_error() {
    assert_btree_replay_stops_at_promotion_gate(
        WalRecordKind::BTreeMerge,
        Lsn::new(4),
        b"merge-data",
    );
}

#[test]
fn btree_error_messages_include_record_kind() {
    // Verify error messages specifically identify which B-Tree variant failed
    let variants = [
        WalRecordKind::BTreeInsert,
        WalRecordKind::BTreeDelete,
        WalRecordKind::BTreeSplit,
        WalRecordKind::BTreeMerge,
    ];

    for variant in &variants {
        let tx_id = TransactionId::new(200);
        let record =
            WalRecord::from_parts(*variant, Lsn::new(1), None, Some(tx_id), vec![]).unwrap();

        let mut ctx = ReplayContext::new();
        let result = replay_wal_record(&mut ctx, &record);

        assert!(result.is_err());
        assert!(ctx.has_errors());

        let error_record = &ctx.error_records[0];
        let error_msg = error_record.error.as_ref().unwrap();
        assert!(
            error_msg.contains(&format!("{variant:?}")) && error_msg.contains("not promoted"),
            "Error should mention the specific B-Tree gate: {}",
            error_msg
        );
    }
}

#[test]
fn non_btree_records_unaffected_by_btree_changes() {
    // Verify that non-B-Tree records still work correctly
    let records = vec![
        (WalRecordKind::TxBegin, TransactionId::new(1), Lsn::new(1)),
        (WalRecordKind::RowInsert, TransactionId::new(1), Lsn::new(2)),
        (WalRecordKind::RowUpdate, TransactionId::new(1), Lsn::new(3)),
        (WalRecordKind::TxCommit, TransactionId::new(1), Lsn::new(4)),
    ];

    for (kind, tx_id, lsn) in records {
        let record = WalRecord::from_parts(kind, lsn, None, Some(tx_id), vec![]).unwrap();
        let mut ctx = ReplayContext::new();
        let result = replay_wal_record(&mut ctx, &record);

        // Non-B-Tree records should not return the B-Tree "not implemented" error
        // They may skip (TxBegin, TxCommit) or fail with other reasons (RowInsert)
        // but they shouldn't fail because B-Tree isn't implemented
        if let Err(error) = result {
            let error_msg = error.to_string();
            assert!(
                !error_msg.contains("BTreeInsert")
                    && !error_msg.contains("BTreeDelete")
                    && !error_msg.contains("BTreeSplit")
                    && !error_msg.contains("BTreeMerge"),
                "Non-B-Tree record should not mention B-Tree errors: {}",
                error_msg
            );
        }
    }
}

#[test]
fn btree_and_row_records_have_consistent_transaction_id_requirements() {
    // Verify that B-Tree mutations and Row mutations have same transaction ID requirement
    let btree_kinds = [
        WalRecordKind::BTreeInsert,
        WalRecordKind::BTreeDelete,
        WalRecordKind::BTreeSplit,
        WalRecordKind::BTreeMerge,
    ];

    let row_kinds = [
        WalRecordKind::RowInsert,
        WalRecordKind::RowUpdate,
        WalRecordKind::RowDelete,
    ];

    for btree_kind in &btree_kinds {
        for row_kind in &row_kinds {
            assert_eq!(
                btree_kind.requires_transaction_id(),
                row_kind.requires_transaction_id(),
                "B-Tree and Row kinds should have consistent TX ID requirements"
            );
        }
    }
}

#[test]
fn gated_btree_replay_prevents_silent_data_loss() {
    // This test documents the critical invariant: if B-Tree records exist in WAL,
    // recovery MUST fail with a clear error, not silently ignore them.

    let tx_id = TransactionId::new(999);
    let btree_record = WalRecord::from_parts(
        WalRecordKind::BTreeInsert,
        Lsn::new(100),
        None,
        Some(tx_id),
        b"critical-index-mutation".to_vec(),
    )
    .unwrap();

    let mut ctx = ReplayContext::new();
    let recovery_result = replay_wal_record(&mut ctx, &btree_record);

    assert!(
        recovery_result.is_err(),
        "B-Tree record recovery must fail, not silently ignore"
    );
    assert!(
        ctx.has_errors(),
        "Recovery context must record B-Tree errors"
    );

    let error = recovery_result.unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Storage);
    assert_eq!(ctx.error_records.len(), 1);
    assert_eq!(ctx.error_records[0].kind, WalRecordKind::BTreeInsert);
    assert_eq!(ctx.error_records[0].lsn, Lsn::new(100));

    let error_msg = error.message();
    assert!(
        error_msg.contains("BTreeInsert") && error_msg.contains("not promoted"),
        "Error message must identify the gated B-Tree mutation: {error_msg}"
    );
}

#[test]
fn btree_recovery_context_accumulates_errors() {
    // Verify that if multiple B-Tree records are replayed, all errors are recorded
    let tx_id = TransactionId::new(500);

    let records = [
        WalRecord::from_parts(
            WalRecordKind::BTreeInsert,
            Lsn::new(1),
            None,
            Some(tx_id),
            vec![],
        )
        .unwrap(),
        WalRecord::from_parts(
            WalRecordKind::BTreeDelete,
            Lsn::new(2),
            Some(Lsn::new(1)),
            Some(tx_id),
            vec![],
        )
        .unwrap(),
    ];

    let mut ctx = ReplayContext::new();

    for (index, record) in records.iter().enumerate() {
        let error = replay_wal_record(&mut ctx, record).expect_err("B-Tree replay must fail-stop");
        assert_eq!(error.kind(), AndromedaErrorKind::Storage);
        assert_eq!(ctx.error_records.len(), index + 1);
        assert_eq!(ctx.error_records[index].kind, record.header.kind);
        assert_eq!(ctx.error_records[index].lsn, record.header.lsn);
        assert_eq!(
            ctx.error_records[index].outcome,
            ReplayOutcome::NotYetImplemented
        );
    }

    assert!(ctx.has_errors());
    assert_eq!(
        ctx.error_records
            .iter()
            .map(|record| record.kind)
            .collect::<Vec<_>>(),
        vec![WalRecordKind::BTreeInsert, WalRecordKind::BTreeDelete]
    );
}
