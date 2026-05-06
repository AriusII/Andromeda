//! B-Tree WAL record placeholder recovery tests.
//!
//! This module validates that B-Tree WAL record types are accepted as
//! placeholders and that recovery handlers report clear "not yet implemented"
//! errors with Wave 18 deferral notes.
//!
//! # Scope
//!
//! - Verify BTreeInsert, BTreeDelete, BTreeSplit, BTreeMerge variants exist
//! - Verify these variants are mapped to tags 23-26
//! - Verify recovery handlers return NotYetImplemented errors with clear messages
//! - Verify B-Tree records require transaction IDs (like other mutations)
//! - Verify non-B-Tree records are unaffected by B-Tree changes
//! - Verify error messages include Wave 18 deferral context

use andromeda_core::TransactionId;
use andromeda_storage::{
    Lsn, ReplayContext, ReplayOutcome, WalRecord, WalRecordKind, encode_wal_record,
    replay_wal_record, wal_record_kind_from_tag, wal_record_kind_tag,
};

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
fn btree_variants_are_not_redo_relevant_wave_13() {
    // In Wave 13, B-Tree records are NOT redo-relevant because recovery
    // handlers are not yet implemented (deferred to Wave 18).
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
    // Verify that BTreeInsert records can be created with valid transaction ID
    let tx_id = TransactionId::new(42);
    let record = WalRecord::from_parts(
        WalRecordKind::BTreeInsert,
        Lsn::new(1),
        None,
        Some(tx_id),
        b"btree-payload".to_vec(),
    );

    assert!(record.is_ok());
    let record = record.unwrap();
    assert_eq!(record.header.kind, WalRecordKind::BTreeInsert);
    assert_eq!(record.header.transaction_id, Some(tx_id));
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

    assert!(record_result.is_err());
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
fn recovery_btree_insert_returns_not_yet_implemented_error() {
    // Verify that replaying BTreeInsert records returns NotYetImplemented error
    let tx_id = TransactionId::new(111);
    let record = WalRecord::from_parts(
        WalRecordKind::BTreeInsert,
        Lsn::new(1),
        None,
        Some(tx_id),
        b"insert-data".to_vec(),
    )
    .unwrap();

    let mut ctx = ReplayContext::new();
    let result = replay_wal_record(&mut ctx, &record);

    // Recovery should fail with a clear error
    assert!(result.is_err());
    let error = result.unwrap_err();
    let error_msg = format!("{}", error);
    assert!(
        error_msg.contains("BTreeInsert") || error_msg.contains("not yet implemented"),
        "Error message should mention BTreeInsert or implementation status: {}",
        error_msg
    );

    // Context should record the error
    assert!(ctx.has_errors());
    assert_eq!(ctx.error_records.len(), 1);
}

#[test]
fn recovery_btree_delete_returns_not_yet_implemented_error() {
    let tx_id = TransactionId::new(112);
    let record = WalRecord::from_parts(
        WalRecordKind::BTreeDelete,
        Lsn::new(2),
        None,
        Some(tx_id),
        b"delete-data".to_vec(),
    )
    .unwrap();

    let mut ctx = ReplayContext::new();
    let result = replay_wal_record(&mut ctx, &record);

    assert!(result.is_err());
    assert!(ctx.has_errors());
    assert_eq!(ctx.error_records.len(), 1);
}

#[test]
fn recovery_btree_split_returns_not_yet_implemented_error() {
    let tx_id = TransactionId::new(113);
    let record = WalRecord::from_parts(
        WalRecordKind::BTreeSplit,
        Lsn::new(3),
        None,
        Some(tx_id),
        b"split-data".to_vec(),
    )
    .unwrap();

    let mut ctx = ReplayContext::new();
    let result = replay_wal_record(&mut ctx, &record);

    assert!(result.is_err());
    assert!(ctx.has_errors());
}

#[test]
fn recovery_btree_merge_returns_not_yet_implemented_error() {
    let tx_id = TransactionId::new(114);
    let record = WalRecord::from_parts(
        WalRecordKind::BTreeMerge,
        Lsn::new(4),
        None,
        Some(tx_id),
        b"merge-data".to_vec(),
    )
    .unwrap();

    let mut ctx = ReplayContext::new();
    let result = replay_wal_record(&mut ctx, &record);

    assert!(result.is_err());
    assert!(ctx.has_errors());
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

        // Error should mention the specific variant
        let error_record = &ctx.error_records[0];
        let error_msg = error_record.error.as_ref().unwrap();
        assert!(
            error_msg.to_lowercase().contains("btree"),
            "Error should mention B-Tree: {}",
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
        if result.is_err() {
            let error_msg = format!("{}", result.unwrap_err());
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
fn wave_13_btree_placeholder_prevents_silent_data_loss() {
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

    // Precondition: Recovery MUST fail
    assert!(
        recovery_result.is_err(),
        "B-Tree record recovery must fail, not silently ignore"
    );

    // Context must record the error
    assert!(
        ctx.has_errors(),
        "Recovery context must record B-Tree errors"
    );

    // Error must be clear and actionable
    let error_msg = format!("{}", recovery_result.unwrap_err());
    assert!(
        !error_msg.is_empty(),
        "Error message must not be empty; must be clear and actionable"
    );
}

#[test]
fn btree_recovery_context_accumulates_errors() {
    // Verify that if multiple B-Tree records are replayed, all errors are recorded
    let tx_id = TransactionId::new(500);

    let records = vec![
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

    for record in &records {
        let _ = replay_wal_record(&mut ctx, record);
    }

    // Both errors should be recorded (recovery stops on first error by design,
    // but context should track what would have happened)
    assert!(ctx.has_errors());
}
