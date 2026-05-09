//! HREDOV1 heap redo WAL production contracts.

use andromeda_storage::{
    Lsn, PageId, PageSize, WalRecord, decode_wal_record_frame, encode_wal_record,
    write_ahead_log::{
        HEAP_ROW_REDO_HEADER_LEN, HEAP_ROW_REDO_NONE_SLOT_ID, HeapRowRedoOperation,
        HeapRowRedoPayloadV1,
    },
};
use andromeda_types::TransactionId;

#[test]
fn wal_record_heap_redo_insert_payload_roundtrips_through_frame_codec() {
    let payload = HeapRowRedoPayloadV1::row_insert(
        PageId::new(900),
        PageSize::KiB16,
        2,
        Lsn::ZERO,
        Lsn::new(10),
        b"alpha".to_vec(),
    )
    .expect("insert payload should build");
    assert_eq!(
        payload.encode().len(),
        HEAP_ROW_REDO_HEADER_LEN + b"alpha".len(),
        "HREDOV1 remains a storage-local v1 payload; catalog/procedure binding is adjacent exec evidence"
    );
    let record = wal_record_from_payload(&payload, TransactionId::new(101), None);

    let encoded = encode_wal_record(&record).expect("WAL frame encode");
    let (decoded, consumed) = decode_wal_record_frame(&encoded)
        .expect("WAL frame decode")
        .expect("record should be present");

    assert_eq!(consumed, encoded.len());
    assert_eq!(decoded, record);
    let decoded_payload =
        HeapRowRedoPayloadV1::decode(decoded.payload(), decoded.header.kind).expect("HREDOV1");
    assert_eq!(decoded_payload, payload);
    assert_eq!(decoded_payload.operation(), HeapRowRedoOperation::Insert);
    assert_eq!(decoded_payload.wal_record_kind(), decoded.header.kind);
    assert_eq!(decoded_payload.tuple(), b"alpha");
}

#[test]
fn wal_record_heap_redo_payload_bytes_are_wal_checksum_protected() {
    let payload = HeapRowRedoPayloadV1::row_insert(
        PageId::new(916),
        PageSize::KiB16,
        2,
        Lsn::ZERO,
        Lsn::new(43),
        b"alpha".to_vec(),
    )
    .expect("insert payload should build");
    let mut record = wal_record_from_payload(&payload, TransactionId::new(103), None);

    record.payload[HEAP_ROW_REDO_HEADER_LEN] = b'b';
    let error = record
        .validate()
        .expect_err("tampered HREDOV1 tuple bytes must break WAL checksum");

    assert!(error.message().contains("checksum"));
}

#[test]
fn wal_record_heap_redo_delete_payload_roundtrips_without_tuple_bytes() {
    let payload = HeapRowRedoPayloadV1::row_delete(
        PageId::new(901),
        PageSize::KiB32,
        4,
        Lsn::new(10),
        Lsn::new(11),
    )
    .expect("delete payload should build");

    let encoded = payload
        .encode_for_wal_kind(payload.wal_record_kind())
        .expect("kind-compatible encode");
    let decoded = HeapRowRedoPayloadV1::decode(&encoded, payload.wal_record_kind())
        .expect("delete payload should decode");

    assert_eq!(decoded, payload);
    assert_eq!(decoded.tuple(), b"");
    assert_eq!(decoded.before_slot_id(), 4);
    assert_eq!(decoded.after_slot_id(), HEAP_ROW_REDO_NONE_SLOT_ID);
}

#[test]
fn wal_record_heap_redo_decode_rejects_conflicting_record_kind() {
    let payload = HeapRowRedoPayloadV1::row_insert(
        PageId::new(903),
        PageSize::KiB16,
        3,
        Lsn::ZERO,
        Lsn::new(30),
        b"alpha".to_vec(),
    )
    .expect("insert payload");

    let error = HeapRowRedoPayloadV1::decode(
        &payload.encode(),
        andromeda_storage::WalRecordKind::RowUpdate,
    )
    .expect_err("conflicting kind must be rejected");

    assert!(error.message().contains("does not match record kind"));
    assert!(
        payload
            .encode_for_wal_kind(andromeda_storage::WalRecordKind::RowDelete)
            .is_err()
    );
}

#[test]
fn wal_record_heap_redo_decode_rejects_truncated_and_malformed_payloads() {
    let payload = HeapRowRedoPayloadV1::row_insert(
        PageId::new(904),
        PageSize::KiB16,
        3,
        Lsn::ZERO,
        Lsn::new(40),
        b"alpha".to_vec(),
    )
    .expect("insert payload")
    .encode();

    let truncated = &payload[..payload.len() - 1];
    let error =
        HeapRowRedoPayloadV1::decode(truncated, andromeda_storage::WalRecordKind::RowInsert)
            .expect_err("tuple length mismatch must be rejected");
    assert!(error.message().contains("tuple length"));

    let mut bad_magic = payload.clone();
    bad_magic[0] ^= 0x7f;
    let error =
        HeapRowRedoPayloadV1::decode(&bad_magic, andromeda_storage::WalRecordKind::RowInsert)
            .expect_err("bad magic must be rejected");
    assert!(error.message().contains("HREDOV1"));

    let short_header = &payload[..8];
    let error =
        HeapRowRedoPayloadV1::decode(short_header, andromeda_storage::WalRecordKind::RowInsert)
            .expect_err("short header must be rejected");
    assert!(error.message().contains("expected at least"));
}

#[test]
fn wal_record_heap_redo_decode_rejects_all_header_prefixes_without_panic() {
    let payload = HeapRowRedoPayloadV1::row_insert(
        PageId::new(914),
        PageSize::KiB16,
        3,
        Lsn::ZERO,
        Lsn::new(41),
        b"alpha".to_vec(),
    )
    .expect("insert payload")
    .encode();

    for prefix_len in 0..HEAP_ROW_REDO_HEADER_LEN {
        let error = HeapRowRedoPayloadV1::decode(
            &payload[..prefix_len],
            andromeda_storage::WalRecordKind::RowInsert,
        )
        .expect_err("truncated prefix must be a typed decode error");
        assert!(!error.message().is_empty());
    }
}

#[test]
fn wal_record_heap_redo_decode_rejects_version_page_size_and_unsupported_kind() {
    let payload = HeapRowRedoPayloadV1::row_insert(
        PageId::new(915),
        PageSize::KiB16,
        3,
        Lsn::ZERO,
        Lsn::new(42),
        b"alpha".to_vec(),
    )
    .expect("insert payload")
    .encode();

    let mut bad_version = payload.clone();
    bad_version[8..10].copy_from_slice(&2u16.to_le_bytes());
    let error =
        HeapRowRedoPayloadV1::decode(&bad_version, andromeda_storage::WalRecordKind::RowInsert)
            .expect_err("unsupported version must be rejected");
    assert!(error.message().contains("version"));

    let mut bad_page_size = payload.clone();
    bad_page_size[11] = 9;
    let error =
        HeapRowRedoPayloadV1::decode(&bad_page_size, andromeda_storage::WalRecordKind::RowInsert)
            .expect_err("bad page size tag must be rejected");
    assert!(error.message().contains("page size"));

    let error = HeapRowRedoPayloadV1::decode(&payload, andromeda_storage::WalRecordKind::TxBegin)
        .expect_err("non-row WAL kind must be rejected before decode promotion");
    assert!(error.message().contains("cannot decode WAL record kind"));
}

#[test]
fn wal_record_heap_redo_builders_reject_invalid_slot_and_lsn_shapes() {
    let insert = HeapRowRedoPayloadV1::row_insert(
        PageId::new(905),
        PageSize::KiB16,
        HEAP_ROW_REDO_NONE_SLOT_ID,
        Lsn::ZERO,
        Lsn::new(50),
        b"alpha".to_vec(),
    );
    assert!(
        insert
            .expect_err("insert must require concrete slot")
            .message()
            .contains("RowInsert")
    );

    let delete = HeapRowRedoPayloadV1::row_delete(
        PageId::new(905),
        PageSize::KiB16,
        1,
        Lsn::new(50),
        Lsn::new(50),
    );
    assert!(
        delete
            .expect_err("resulting LSN must advance")
            .message()
            .contains("precede")
    );

    let update = HeapRowRedoPayloadV1::row_update(
        PageId::new(905),
        PageSize::KiB16,
        1,
        1,
        Lsn::new(50),
        Lsn::new(51),
        b"beta".to_vec(),
    );
    assert!(
        update
            .expect_err("update must move slot")
            .message()
            .contains("RowUpdate")
    );
}

fn wal_record_from_payload(
    payload: &HeapRowRedoPayloadV1,
    tx: TransactionId,
    previous_lsn: Option<Lsn>,
) -> WalRecord {
    WalRecord::from_parts(
        payload.wal_record_kind(),
        payload.resulting_page_lsn(),
        previous_lsn,
        Some(tx),
        payload.encode(),
    )
    .expect("heap redo WAL record should be structurally valid")
}
