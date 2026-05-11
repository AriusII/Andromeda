//! WAL non-starvation integration test.
//!
//! Proves P08 D3 readiness: the "no WAL starvation" requirement documented in
//! the A3 backpressure audit (gap G1).
//!
//! When a [`BackpressuredResultStream`] is fully backpressured — producer
//! blocked waiting for a consumer that never arrives — a concurrent
//! [`FileWal`] append + fsync on an independent WAL file must complete within
//! a generous bounded time (5 s).  This verifies that the two subsystems share
//! **no common lock**: `BackpressuredResultStream` backpressure is bounded to
//! its internal mpsc channel and the `emission_gate` `RwLock`; `FileWal`
//! operates exclusively on its own `File` handle and an `Arc<AtomicU64>`.
//!
//! [`BackpressuredResultStream`]: andromeda_result_stream::BackpressuredResultStream
//! [`FileWal`]: andromeda_wal::FileWal

use std::sync::Arc;
use std::time::Duration;

use andromeda_result_stream::{BackpressuredResultStream, ResultStreamMetadata};
use andromeda_srpl_cardinality::Cardinality;
use andromeda_structured_object::{RowCountPolicy, StructuredObjectHeader, StructuredObjectLayout};
use andromeda_types::{ColumnDescriptor, ContractHash, ScalarType, TransactionId, TypeDescriptor};
use andromeda_wal::{FileWal, WalRecordKind};

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

fn single_column_row() -> StructuredObjectHeader {
    let fields = vec![ColumnDescriptor {
        name: "value".to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal: 0,
    }];
    let layout = StructuredObjectLayout::RowMajor;
    StructuredObjectHeader {
        name: "wal_no_starvation.row".to_string(),
        contract_hash: ContractHash::test_vector(1),
        descriptor_hash: StructuredObjectHeader::compute_descriptor_hash(&fields, layout),
        column_count: fields.len() as u32,
        fields,
        layout,
        row_count_policy: RowCountPolicy::UnknownAllowed,
        row_count_exact: None,
        payload_length: 0,
        payload_checksum: None,
        max_payload_length: Some(0),
    }
}

fn stream_with_metadata(capacity: usize) -> BackpressuredResultStream {
    let mut stream = BackpressuredResultStream::new(capacity).unwrap();
    stream
        .emit_metadata(ResultStreamMetadata {
            stream_id: 42,
            row_count_exact: None,
            row_count_max: Some(1_000),
            column_count: 1,
            cardinality: Cardinality::Many,
        })
        .unwrap();
    stream
}

// ---------------------------------------------------------------------------
// Test
// ---------------------------------------------------------------------------

/// Proves that a fully backpressured `BackpressuredResultStream` (producer
/// blocked on a full channel, no consumer draining rows) does **not** prevent
/// a concurrent `FileWal::flush_all` from completing.
///
/// # Acceptance criteria
///
/// - Stream is backpressured (capacity=1, no consumer).
/// - WAL append + fsync completes within 5 seconds while the stream is
///   backpressured.
/// - No deadlock, no panic, no error.
///
/// # Why `multi_thread` matters
///
/// With `flavor = "multi_thread"` the blocked producer task and the WAL
/// `spawn_blocking` task can execute on independent OS threads simultaneously.
/// This is the only configuration that actually exercises concurrent forward
/// progress between the two subsystems.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slow_client_does_not_starve_wal_fsync() {
    // -----------------------------------------------------------------------
    // Phase 1 — saturate the result stream so the producer blocks.
    //
    // Capacity = 1: the mpsc channel can hold exactly one message.  The first
    // push_row fills the slot and returns immediately; the second push_row
    // blocks because the channel is full and no consumer is running.
    // -----------------------------------------------------------------------
    let stream = Arc::new(stream_with_metadata(1));

    let producer_stream = Arc::clone(&stream);
    let producer = tokio::spawn(async move {
        // Row 1 — fits in the single-slot channel, returns immediately.
        producer_stream.push_row(single_column_row()).await.unwrap();
        // Row 2 — blocks: channel is full, no consumer is running.
        producer_stream.push_row(single_column_row()).await.unwrap();
    });

    // Allow the runtime to schedule the producer and let it reach the blocking
    // send on row 2.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Confirm the stream is actually backpressured before we proceed so the
    // test is self-checking rather than vacuously passing.
    assert!(
        stream.is_backpressured(),
        "result stream must be backpressured (capacity=1, no consumer) before WAL fsync starts",
    );

    // -----------------------------------------------------------------------
    // Phase 2 — perform WAL append + fsync while the stream is backpressured.
    //
    // FileWal is purely synchronous (no async/await).  We run it on a Tokio
    // blocking thread so it doesn't starve the async executor, and to place
    // it on a fully independent OS thread relative to the producer task.
    // -----------------------------------------------------------------------
    let wal_task = tokio::task::spawn_blocking(|| {
        let tmp = tempfile::tempdir().expect("create tempdir for WAL");
        let wal_path = tmp.path().join("no-starvation-test.wal");

        let mut wal = FileWal::open(&wal_path).expect("FileWal::open");

        let tx = TransactionId::new(1);
        wal.append_tx_begin(tx).expect("append TxBegin");
        wal.append_payload(WalRecordKind::RowInsert, Some(tx), b"starvation-probe")
            .expect("append RowInsert");
        wal.append_tx_commit(tx).expect("append TxCommit");

        // flush_all() calls sync_data() then sync_all() — the actual fsync.
        // Under a starvation bug (shared lock with the result stream), this
        // call would time out.
        let durable_lsn = wal.flush_all().expect("FileWal::flush_all");

        // Return the raw LSN value so the outer test can assert on it.
        durable_lsn.get()
    });

    // -----------------------------------------------------------------------
    // Phase 3 — assert WAL fsync completed within 5 s while backpressured.
    //
    // A 5-second bound is deliberately generous: on any sane OS, three WAL
    // records + two fsync calls should complete in under 500 ms.  The bound
    // exists purely to catch a deadlock / starvation regression, not to
    // measure performance.
    // -----------------------------------------------------------------------
    let wal_result = tokio::time::timeout(Duration::from_secs(5), wal_task).await;

    assert!(
        wal_result.is_ok(),
        "WAL fsync must complete within 5 s — \
         ResultStream backpressure must not starve WAL (P08 D3 non-starvation requirement)",
    );

    let durable_lsn_raw = wal_result
        .expect("timeout did not fire")
        .expect("spawn_blocking task must not panic");

    assert_eq!(
        durable_lsn_raw, 3,
        "WAL must be durable through LSN 3 (TxBegin=1, RowInsert=2, TxCommit=3)",
    );

    // -----------------------------------------------------------------------
    // Phase 4 — drain the stream so the producer task can exit cleanly.
    //
    // Consuming row 1 unblocks the producer's row-2 send; consuming row 2
    // lets the producer task return.  Without this the test would hang on the
    // final `producer.await`.
    // -----------------------------------------------------------------------
    assert!(
        stream.next_row().await.is_some(),
        "row 1 must be readable from stream"
    );
    assert!(
        stream.next_row().await.is_some(),
        "row 2 must be readable after producer unblocks"
    );

    producer
        .await
        .expect("producer task must complete without panic");
}
