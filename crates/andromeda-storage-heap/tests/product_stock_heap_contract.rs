#![forbid(unsafe_code)]

use andromeda_storage_heap::{
    Datum, HeapPage, HeapPageInsert, PRODUCT_STOCK_ROW_ENCODED_LEN, ProductStockRow,
};
use andromeda_storage_page::{PageId, PageSize};
use andromeda_wal::Lsn;

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
fn product_stock_heap_insert_produces_hredov1_redo_material() {
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
