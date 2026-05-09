#![forbid(unsafe_code)]

use andromeda_storage::{
    DatabaseManifest, Datum, HeapPage, HeapPageInsert, InMemoryWal, Lsn,
    PRODUCT_STOCK_ROW_ENCODED_LEN, PageId, PageSize, ProductStockRow, RecoveryPlan, ReplayContext,
    StartupMode, WalRecord, WalRecordKind, execute_redo_plan_into_context, replay_wal_record,
    write_ahead_log::HeapRowRedoPayloadV1,
};
use andromeda_types::TransactionId;

#[test]
fn product_stock_row_encoding_has_stable_golden_bytes() {
    let row = ProductStockRow::new(42, 10).expect("valid row");
    let encoded = row.encode().expect("encode ProductStock");

    let mut expected = Vec::new();
    expected.push(0x00);
    expected.extend_from_slice(&42i64.to_le_bytes());
    expected.extend_from_slice(&10i64.to_le_bytes());

    assert_eq!(encoded.len(), PRODUCT_STOCK_ROW_ENCODED_LEN);
    assert_eq!(encoded, expected);
    assert_eq!(
        ProductStockRow::decode(&encoded).expect("decode ProductStock"),
        row
    );
}

#[test]
fn product_stock_row_rejects_invalid_business_shape_before_heap_insert() {
    assert!(ProductStockRow::new(0, 10).is_err());
    assert!(ProductStockRow::new(42, -1).is_err());
}

#[test]
fn product_stock_row_decode_rejects_non_canonical_tuple_length() {
    let row = ProductStockRow::new(42, 10).expect("valid row");
    let encoded = row.encode().expect("encode ProductStock");

    let mut trailing = encoded.clone();
    trailing.push(0xAA);
    let trailing_err =
        ProductStockRow::decode(&trailing).expect_err("trailing bytes must be rejected");
    assert!(trailing_err.message().contains("encoded length mismatch"));

    let truncated = &encoded[..encoded.len() - 1];
    let truncated_err =
        ProductStockRow::decode(truncated).expect_err("truncated row must be rejected");
    assert!(truncated_err.message().contains("encoded length mismatch"));

    let mut reserved_bitmap_bit = encoded;
    reserved_bitmap_bit[0] = 0b0000_0100;
    let bitmap_err = ProductStockRow::decode(&reserved_bitmap_bit)
        .expect_err("reserved null bitmap bits must be rejected");
    assert!(bitmap_err.message().contains("null bitmap"));
}

#[test]
fn product_stock_row_decode_rejects_null_bitmap() {
    let row = ProductStockRow::new(42, 10).expect("valid row");
    let mut encoded = row.encode().expect("encode ProductStock");
    encoded[0] = 0b0000_0001;

    let error = ProductStockRow::decode(&encoded).expect_err("ProductStock has no nullable fields");

    assert!(error.message().contains("null bitmap"));
}

#[test]
fn product_stock_structured_insert_rejects_null_and_type_mismatch() {
    let mut page = HeapPageInsert::for_product_stock(PageId::new(10_000), PageSize::KiB16)
        .expect("ProductStock heap page");

    let null_error = page
        .insert_tuple(&[Datum::Null, Datum::Int64(10)])
        .expect_err("ProductStock insert_tuple must reject Null ProductId");
    assert!(null_error.message().contains("not nullable"));

    let type_error = page
        .insert_tuple(&[Datum::UInt64(42), Datum::Int64(10)])
        .expect_err("ProductStock insert_tuple must reject non-contract ProductId type");
    assert!(type_error.message().contains("expected Int64"));

    assert_eq!(page.slot_count(), 0);
    assert_eq!(page.active_slot_count(), 0);
}

#[test]
fn product_stock_heap_insert_read_and_serialized_scan_roundtrip() {
    let mut page = HeapPageInsert::for_product_stock(PageId::new(10_001), PageSize::KiB16)
        .expect("ProductStock heap page");
    let row_a = ProductStockRow::new(42, 10).expect("row A");
    let row_b = ProductStockRow::new(43, 25).expect("row B");

    let insert_a = page.insert_product_stock(row_a).expect("insert row A");
    let insert_b = page.insert_product_stock(row_b).expect("insert row B");

    assert_eq!(insert_a.slot_id(), 0);
    assert_eq!(insert_a.page_id(), PageId::new(10_001));
    assert_eq!(insert_a.page_size(), PageSize::KiB16);
    assert_eq!(insert_a.row(), row_a);
    assert_eq!(insert_b.slot_id(), 1);
    assert_eq!(
        page.read_product_stock(insert_a.slot_id())
            .expect("read row A"),
        row_a
    );
    assert_eq!(
        page.read_product_stock(insert_b.slot_id())
            .expect("read row B"),
        row_b
    );

    let image = page.serialize().expect("serialize heap image");
    let recovered = HeapPage::from_image(PageSize::KiB16, &image).expect("read heap image");
    let rows = recovered
        .scan_product_stock()
        .collect::<Result<Vec<_>, _>>()
        .expect("scan ProductStock rows");

    assert_eq!(rows, vec![(0, row_a), (1, row_b)]);
}

#[test]
fn product_stock_scan_rejects_malformed_product_stock_tuple() {
    let mut page = HeapPageInsert::new(PageId::new(10_003), PageSize::KiB16).expect("heap page");
    let mut malformed = ProductStockRow::new(42, 10)
        .expect("valid row")
        .encode()
        .expect("encode ProductStock");
    malformed[0] = 0b0000_0001;

    page.insert_raw_tuple(&malformed)
        .expect("insert malformed raw tuple");
    let image = page.serialize().expect("serialize heap image");
    let recovered = HeapPage::from_image(PageSize::KiB16, &image).expect("read heap image");

    let error = recovered
        .scan_product_stock()
        .next()
        .expect("one scanned tuple")
        .expect_err("malformed ProductStock tuple must fail typed scan");

    assert!(error.message().contains("null bitmap"));
}

#[test]
fn product_stock_heap_insert_rejects_overflow_without_partial_slot() {
    let mut page = HeapPageInsert::for_product_stock(PageId::new(10_004), PageSize::KiB16)
        .expect("ProductStock heap page");

    let mut product_id = 1i64;
    let failure = loop {
        let row = ProductStockRow::new(product_id, 0).expect("valid ProductStock row");
        match page.insert_product_stock(row) {
            Ok(_) => product_id += 1,
            Err(error) => break error,
        }
    };

    let inserted = (product_id - 1) as usize;
    assert!(inserted > 0);
    assert_eq!(page.slot_count(), inserted);
    assert_eq!(page.active_slot_count(), inserted);
    assert!(
        failure.message().contains("slot limit")
            || failure.message().contains("insufficient free space")
            || failure.message().contains("full"),
        "unexpected overflow error: {}",
        failure.message()
    );
    assert_eq!(page.slot_count(), inserted);
    assert_eq!(page.active_slot_count(), inserted);
}

#[test]
fn product_stock_heap_insert_produces_hredov1_redo_material_for_recovery() {
    let page_id = PageId::new(10_002);
    let mut page = HeapPageInsert::for_product_stock(page_id, PageSize::KiB16)
        .expect("ProductStock heap page");
    let row = ProductStockRow::new(42, 7).expect("row");

    let insert = page.insert_product_stock(row).expect("insert row");
    let payload = insert
        .row_insert_redo_payload(Lsn::ZERO, Lsn::new(6))
        .expect("HREDOV1 insert payload");

    assert_eq!(payload.page_id(), page_id);
    assert_eq!(payload.page_size(), PageSize::KiB16);
    assert_eq!(payload.after_slot_id(), insert.slot_id());
    assert_eq!(payload.tuple(), insert.tuple());
    assert_eq!(
        ProductStockRow::decode(payload.tuple()).expect("decode"),
        row
    );

    let record = WalRecord::from_parts(
        payload.wal_record_kind(),
        Lsn::new(6),
        None,
        Some(TransactionId::new(700)),
        payload.encode(),
    )
    .expect("valid row insert WAL record");
    let mut replay = ReplayContext::new();

    replay_wal_record(&mut replay, &record).expect("HREDOV1 replay");

    let recovered_page = replay.heap_redo_page(page_id).expect("recovered page");
    assert_eq!(replay.heap_redo_page_origin(page_id), Some("redo-created"));
    let recovered_tuple = recovered_page
        .read_tuple(insert.slot_id())
        .expect("recovered ProductStock tuple");
    assert_eq!(
        ProductStockRow::decode(recovered_tuple).expect("decode recovered tuple"),
        row
    );
    assert_eq!(
        recovered_page
            .read_product_stock_recovery_row(insert.slot_id())
            .expect("typed recovered ProductStock row"),
        Some((insert.slot_id(), Lsn::new(6), row))
    );
    assert_eq!(recovered_page.page_lsn(), Lsn::new(6));
    assert!(!replay.has_errors());
}

#[test]
fn product_stock_redo_payload_rejects_non_advancing_lsn() {
    let mut page = HeapPageInsert::for_product_stock(PageId::new(10_006), PageSize::KiB16)
        .expect("ProductStock heap page");
    let row = ProductStockRow::new(42, 7).expect("row");
    let insert = page.insert_product_stock(row).expect("insert row");

    let same_lsn = insert
        .row_insert_redo_payload(Lsn::new(6), Lsn::new(6))
        .expect_err("redo evidence must advance page LSN");
    assert!(same_lsn.message().contains("precede"));

    let zero_lsn = insert
        .row_insert_redo_payload(Lsn::ZERO, Lsn::ZERO)
        .expect_err("redo evidence must include a resulting page LSN");
    assert!(zero_lsn.message().contains("must not be zero"));
}

#[test]
fn product_stock_committed_wal_replay_reconstructs_deterministic_rows_from_durable_prefix() {
    let page_id = PageId::new(10_003);
    let tx = TransactionId::new(701);
    let row_a = ProductStockRow::new(42, 10).expect("row A");
    let row_b = ProductStockRow::new(43, 25).expect("row B");
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(tx).expect("begin");
    let row_a_lsn = append_product_stock_insert(&mut wal, tx, page_id, 0, Lsn::ZERO, row_a);
    let row_b_lsn = append_product_stock_insert(&mut wal, tx, page_id, 1, row_a_lsn, row_b);
    wal.append_tx_commit(tx).expect("commit");
    wal.flush_all().expect("durable WAL");

    let records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("recovery plan");
    let mut left = ReplayContext::new();
    let mut right = ReplayContext::new();

    let left_report = execute_redo_plan_into_context(&plan, &records, &mut left)
        .expect("left committed ProductStock replay");
    let right_report = execute_redo_plan_into_context(&plan, &records, &mut right)
        .expect("right committed ProductStock replay");

    assert_eq!(left_report.applied_count, 2);
    assert_eq!(left_report.committed_transaction_count, 1);
    assert_eq!(left_report.incomplete_transaction_count, 0);
    assert_eq!(left_report.replay_end_lsn, Some(row_b_lsn));
    assert!(!left_report.has_replay_errors);
    assert_eq!(left_report, right_report);
    assert_eq!(left.heap_redo_page(page_id), right.heap_redo_page(page_id));

    let recovered_page = left.heap_redo_page(page_id).expect("recovered page");
    assert_eq!(recovered_page.page_lsn(), row_b_lsn);
    assert_eq!(recovered_page.live_slot_count(), 2);
    assert_eq!(
        recovered_page
            .decode_product_stock_rows()
            .expect("typed ProductStock replay projection"),
        vec![(0, row_a), (1, row_b)]
    );
    assert_eq!(
        recovered_page
            .decode_product_stock_recovery_rows()
            .expect("typed ProductStock replay projection with WAL source LSN"),
        vec![(0, row_a_lsn, row_a), (1, row_b_lsn, row_b)]
    );
}

#[test]
fn product_stock_rolled_back_and_incomplete_wal_records_do_not_reconstruct_visible_state() {
    let page_id = PageId::new(10_004);
    let committed_tx = TransactionId::new(702);
    let rolled_back_tx = TransactionId::new(703);
    let incomplete_tx = TransactionId::new(704);
    let committed = ProductStockRow::new(50, 8).expect("committed row");
    let rolled_back = ProductStockRow::new(51, 99).expect("rolled back row");
    let incomplete = ProductStockRow::new(52, 77).expect("incomplete row");
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(committed_tx).expect("begin committed");
    let committed_lsn =
        append_product_stock_insert(&mut wal, committed_tx, page_id, 0, Lsn::ZERO, committed);
    wal.append_tx_commit(committed_tx)
        .expect("commit committed");

    wal.append_tx_begin(rolled_back_tx)
        .expect("begin rolled back");
    append_product_stock_insert(
        &mut wal,
        rolled_back_tx,
        page_id,
        1,
        committed_lsn,
        rolled_back,
    );
    wal.append_tx_rollback(rolled_back_tx)
        .expect("rollback rolled back");

    wal.append_tx_begin(incomplete_tx)
        .expect("begin incomplete");
    append_product_stock_insert(
        &mut wal,
        incomplete_tx,
        page_id,
        2,
        committed_lsn,
        incomplete,
    );
    wal.flush_all().expect("durable crash survivor");

    let records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("recovery plan");
    let mut replay = ReplayContext::new();

    let report = execute_redo_plan_into_context(&plan, &records, &mut replay)
        .expect("rolled back and incomplete ProductStock rows are skipped by plan");

    assert_eq!(report.applied_count, 1);
    assert_eq!(report.committed_transaction_count, 1);
    assert_eq!(report.incomplete_transaction_count, 1);
    assert!(report.has_discarded_transactions());
    assert!(!report.has_replay_errors);

    let recovered_page = replay.heap_redo_page(page_id).expect("committed page");
    assert_eq!(recovered_page.page_lsn(), committed_lsn);
    assert_eq!(recovered_page.live_slot_count(), 1);
    assert_eq!(
        recovered_page
            .decode_product_stock_rows()
            .expect("typed ProductStock replay projection"),
        vec![(0, committed)]
    );
    assert_eq!(
        recovered_page
            .decode_product_stock_recovery_rows()
            .expect("typed ProductStock replay projection with WAL source LSN"),
        vec![(0, committed_lsn, committed)]
    );
    assert_eq!(recovered_page.read_tuple(1), None);
    assert_eq!(recovered_page.read_tuple(2), None);
}

#[test]
fn product_stock_wal_row_without_durable_commit_is_not_reconstructed() {
    let page_id = PageId::new(10_007);
    let tx = TransactionId::new(706);
    let row = ProductStockRow::new(61, 4).expect("row");
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(tx).expect("begin");
    let row_lsn = append_product_stock_insert(&mut wal, tx, page_id, 0, Lsn::ZERO, row);
    wal.append_tx_commit(tx)
        .expect("commit appended but not durable");
    wal.flush_through(row_lsn)
        .expect("only row WAL record durable; commit is not durable");

    let records = wal.replay_durable();
    assert_eq!(
        records.last().map(|record| record.header.lsn),
        Some(row_lsn),
        "durable prefix must stop before the TxCommit record"
    );

    let manifest = test_manifest(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("recovery plan from durable prefix");
    let mut replay = ReplayContext::new();

    let report = execute_redo_plan_into_context(&plan, &records, &mut replay)
        .expect("row without durable commit evidence must be skipped");

    assert_eq!(report.applied_count, 0);
    assert_eq!(report.committed_transaction_count, 0);
    assert_eq!(report.incomplete_transaction_count, 1);
    assert!(report.has_discarded_transactions());
    assert!(!report.has_replay_errors);
    assert_eq!(replay.heap_redo_page_count(), 0);
}

#[test]
fn product_stock_committed_malformed_hredov1_payload_fails_closed_before_page_state() {
    let tx = TransactionId::new(707);
    let mut wal = InMemoryWal::new();

    wal.append_tx_begin(tx).expect("begin");
    wal.append_payload(WalRecordKind::RowInsert, Some(tx), b"HREDOV1".to_vec())
        .expect("append malformed HREDOV1 row payload");
    wal.append_tx_commit(tx).expect("commit");
    wal.flush_all().expect("durable WAL");

    let records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("recovery plan");
    let mut replay = ReplayContext::new();

    let error = execute_redo_plan_into_context(&plan, &records, &mut replay)
        .expect_err("malformed committed HREDOV1 payload must fail closed");

    assert!(error.message().contains("HREDOV1"));
    assert!(error.message().contains("fail closed"));
    assert_eq!(replay.heap_redo_page_count(), 0);
    assert!(replay.has_errors());
}

#[test]
fn product_stock_malformed_recovered_tuple_fails_closed_on_product_stock_decode() {
    let page_id = PageId::new(10_005);
    let tx = TransactionId::new(705);
    let mut malformed_tuple = ProductStockRow::new(60, 3)
        .expect("row")
        .encode()
        .expect("encode row");
    malformed_tuple[0] = 0b0000_0100;
    let decode_error =
        ProductStockRow::decode(&malformed_tuple).expect_err("malformed tuple must fail closed");
    assert!(decode_error.message().contains("null bitmap"));

    let mut wal = InMemoryWal::new();
    wal.append_tx_begin(tx).expect("begin");
    append_product_stock_tuple(&mut wal, tx, page_id, 0, Lsn::ZERO, malformed_tuple);
    wal.append_tx_commit(tx).expect("commit");
    wal.flush_all().expect("durable WAL");

    let records = wal.replay_durable();
    let manifest = test_manifest(Lsn::new(1));
    let plan = RecoveryPlan::from_manifest_and_wal(&manifest, StartupMode::SafeStart, &records)
        .expect("recovery plan");
    let mut replay = ReplayContext::new();

    let report = execute_redo_plan_into_context(&plan, &records, &mut replay)
        .expect("generic heap redo preserves bytes for ProductStock validation");

    assert_eq!(report.applied_count, 1);
    assert!(!report.has_replay_errors);
    let recovered_page = replay.heap_redo_page(page_id).expect("recovered page");
    let recovered_decode_error = recovered_page
        .decode_product_stock_rows()
        .expect_err("malformed recovered ProductStock tuple must fail closed");
    assert!(recovered_decode_error.message().contains("null bitmap"));
}

fn test_manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::ZERO,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xdead_beef,
    }
}

fn append_product_stock_insert(
    wal: &mut InMemoryWal,
    tx: TransactionId,
    page_id: PageId,
    slot_id: u16,
    expected_previous_page_lsn: Lsn,
    row: ProductStockRow,
) -> Lsn {
    append_product_stock_tuple(
        wal,
        tx,
        page_id,
        slot_id,
        expected_previous_page_lsn,
        row.encode().expect("encode ProductStock row"),
    )
}

fn append_product_stock_tuple(
    wal: &mut InMemoryWal,
    tx: TransactionId,
    page_id: PageId,
    slot_id: u16,
    expected_previous_page_lsn: Lsn,
    tuple: Vec<u8>,
) -> Lsn {
    let resulting_lsn = wal.next_lsn();
    let payload = HeapRowRedoPayloadV1::row_insert(
        page_id,
        PageSize::KiB16,
        slot_id,
        expected_previous_page_lsn,
        resulting_lsn,
        tuple,
    )
    .expect("HREDOV1 ProductStock insert payload");
    let appended_lsn = wal
        .append_payload(WalRecordKind::RowInsert, Some(tx), payload.encode())
        .expect("append ProductStock HREDOV1 row insert");
    assert_eq!(appended_lsn, resulting_lsn);
    appended_lsn
}
