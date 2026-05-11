use andromeda_error::AndromedaResult;
use andromeda_manifest::DatabaseManifest;
use andromeda_recovery::{
    ConceptualRedoPlan, RecoveryWalReplayAdapter, ReplayContext, WalReplayReport,
    execute_redo_plan_with_adapter, replay_wal_record,
};
use andromeda_storage_heap::HeapRowRedoPayloadV1;
use andromeda_storage_page::{PageId, PageSize};
use andromeda_types::TransactionId;
use andromeda_wal::{InMemoryWal, Lsn, WalRecord, WalRecordKind};

const HREDOV1_MAGIC: &[u8; 8] = b"HREDOV1\0";
const NONE_SLOT_ID: u16 = u16::MAX;

#[derive(Debug, Clone, Copy)]
enum HeapOp {
    Insert,
    Delete,
    Update,
}

pub(crate) fn record(
    kind: WalRecordKind,
    lsn: Lsn,
    tx: TransactionId,
    payload: Vec<u8>,
) -> WalRecord {
    WalRecord::from_parts(kind, lsn, None, Some(tx), payload)
        .expect("test WAL record should be structurally valid")
}

pub(crate) fn row_insert_record(
    lsn: Lsn,
    tx: TransactionId,
    page_id: PageId,
    slot_id: u16,
    expected_previous_page_lsn: Lsn,
    tuple: &[u8],
) -> WalRecord {
    record(
        WalRecordKind::RowInsert,
        lsn,
        tx,
        heap_redo_payload(
            HeapOp::Insert,
            page_id,
            NONE_SLOT_ID,
            slot_id,
            expected_previous_page_lsn,
            lsn,
            tuple,
        ),
    )
}

pub(crate) fn row_delete_record(
    lsn: Lsn,
    tx: TransactionId,
    page_id: PageId,
    slot_id: u16,
    expected_previous_page_lsn: Lsn,
) -> WalRecord {
    record(
        WalRecordKind::RowDelete,
        lsn,
        tx,
        heap_redo_payload(
            HeapOp::Delete,
            page_id,
            slot_id,
            NONE_SLOT_ID,
            expected_previous_page_lsn,
            lsn,
            b"",
        ),
    )
}

pub(crate) fn row_update_record(
    lsn: Lsn,
    tx: TransactionId,
    page_id: PageId,
    before_slot_id: u16,
    after_slot_id: u16,
    expected_previous_page_lsn: Lsn,
    tuple: &[u8],
) -> WalRecord {
    record(
        WalRecordKind::RowUpdate,
        lsn,
        tx,
        heap_redo_payload(
            HeapOp::Update,
            page_id,
            before_slot_id,
            after_slot_id,
            expected_previous_page_lsn,
            lsn,
            tuple,
        ),
    )
}

pub(crate) fn assert_missing_snapshot_base_state(
    ctx: &ReplayContext,
    page_id: PageId,
    error_message: &str,
) {
    assert!(error_message.contains("explicit heap page base state"));
    assert!(error_message.contains("snapshot-hydrated"));
    assert_eq!(ctx.error_records.len(), 1);
    assert_eq!(ctx.heap_redo_page_count(), 0);
    assert_eq!(ctx.heap_redo_page_origin(page_id), None);
}

pub(crate) fn test_manifest(required_wal_start_lsn: Lsn) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::ZERO,
        required_wal_start_lsn,
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xdead_beef,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    }
}

pub(crate) fn append_hredov1_insert(
    wal: &mut InMemoryWal,
    tx: TransactionId,
    page_id: PageId,
    slot_id: u16,
    expected_previous_page_lsn: Lsn,
    tuple: &[u8],
) -> Lsn {
    let resulting_lsn = wal.next_lsn();
    let payload = HeapRowRedoPayloadV1::row_insert(
        page_id,
        PageSize::KiB16,
        slot_id,
        expected_previous_page_lsn,
        resulting_lsn,
        tuple.to_vec(),
    )
    .expect("HREDOV1 insert payload");
    wal.append_payload(payload.wal_record_kind(), Some(tx), payload.encode())
        .expect("append HREDOV1 insert")
}

pub(crate) fn append_hredov1_update(
    wal: &mut InMemoryWal,
    tx: TransactionId,
    page_id: PageId,
    before_slot_id: u16,
    after_slot_id: u16,
    expected_previous_page_lsn: Lsn,
    tuple: &[u8],
) -> Lsn {
    let resulting_lsn = wal.next_lsn();
    let payload = HeapRowRedoPayloadV1::row_update(
        page_id,
        PageSize::KiB16,
        before_slot_id,
        after_slot_id,
        expected_previous_page_lsn,
        resulting_lsn,
        tuple.to_vec(),
    )
    .expect("HREDOV1 update payload");
    wal.append_payload(payload.wal_record_kind(), Some(tx), payload.encode())
        .expect("append HREDOV1 update")
}

pub(crate) fn append_hredov1_delete(
    wal: &mut InMemoryWal,
    tx: TransactionId,
    page_id: PageId,
    slot_id: u16,
    expected_previous_page_lsn: Lsn,
) -> Lsn {
    let resulting_lsn = wal.next_lsn();
    let payload = HeapRowRedoPayloadV1::row_delete(
        page_id,
        PageSize::KiB16,
        slot_id,
        expected_previous_page_lsn,
        resulting_lsn,
    )
    .expect("HREDOV1 delete payload");
    wal.append_payload(payload.wal_record_kind(), Some(tx), payload.encode())
        .expect("append HREDOV1 delete")
}

pub(crate) fn execute_redo_plan_into_context(
    plan: &ConceptualRedoPlan,
    durable_records: &[WalRecord],
    ctx: &mut ReplayContext,
) -> AndromedaResult<WalReplayReport> {
    let mut adapter = TestRecoveryReplayAdapter;
    execute_redo_plan_with_adapter(plan, durable_records, ctx, &mut adapter)
}

struct TestRecoveryReplayAdapter;

impl RecoveryWalReplayAdapter<ReplayContext> for TestRecoveryReplayAdapter {
    fn replay_record(
        &mut self,
        ctx: &mut ReplayContext,
        record: &WalRecord,
    ) -> AndromedaResult<()> {
        replay_wal_record(ctx, record)
    }
}

fn heap_redo_payload(
    op: HeapOp,
    page_id: PageId,
    before_slot_id: u16,
    after_slot_id: u16,
    expected_previous_page_lsn: Lsn,
    resulting_page_lsn: Lsn,
    tuple: &[u8],
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(44 + tuple.len());
    payload.extend_from_slice(HREDOV1_MAGIC);
    payload.extend_from_slice(&1u16.to_le_bytes());
    payload.push(match op {
        HeapOp::Insert => 1,
        HeapOp::Delete => 2,
        HeapOp::Update => 3,
    });
    payload.push(match PageSize::KiB16 {
        PageSize::KiB16 => 1,
        PageSize::KiB32 => 2,
    });
    payload.extend_from_slice(&page_id.get().to_le_bytes());
    payload.extend_from_slice(&before_slot_id.to_le_bytes());
    payload.extend_from_slice(&after_slot_id.to_le_bytes());
    payload.extend_from_slice(&expected_previous_page_lsn.get().to_le_bytes());
    payload.extend_from_slice(&resulting_page_lsn.get().to_le_bytes());
    payload.extend_from_slice(&(tuple.len() as u32).to_le_bytes());
    payload.extend_from_slice(tuple);
    payload
}
