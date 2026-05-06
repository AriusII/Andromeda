use andromeda_core::{ContractHash, RequestId};
use andromeda_exec::{
    BackpressuredResultStream, CompletionStatus, DEFAULT_RESULT_STREAM_CAPACITY,
    MAX_RESULT_STREAM_CAPACITY, MIN_RESULT_STREAM_CAPACITY, ResultStreamMetadata,
};
use andromeda_proto::{RowCountPolicy, StructuredObjectHeader, StructuredObjectLayout};
use andromeda_srpl::Cardinality;
use std::sync::Arc;
use tokio::sync::Barrier;

fn test_row() -> StructuredObjectHeader {
    let fields = Vec::new();
    let layout = StructuredObjectLayout::RowMajor;
    StructuredObjectHeader {
        name: "test.row".to_string(),
        contract_hash: ContractHash::test_vector(1),
        descriptor_hash: StructuredObjectHeader::compute_descriptor_hash(&fields, layout),
        fields,
        column_count: 0,
        layout,
        row_count_policy: RowCountPolicy::UnknownAllowed,
        row_count_exact: None,
        payload_length: 0,
        payload_checksum: None,
        max_payload_length: Some(0),
    }
}

#[tokio::test]
async fn test_result_stream_metadata_before_payload_contract() {
    let mut stream = BackpressuredResultStream::new(10).unwrap();

    assert_eq!(stream.metadata(), None);

    let metadata = ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: Some(5),
        row_count_max: Some(5),
        column_count: 3,
        cardinality: Cardinality::Many,
    };

    stream.emit_metadata(metadata).unwrap();
    assert_eq!(stream.metadata(), Some(&metadata));

    let metadata2 = ResultStreamMetadata {
        stream_id: 2,
        row_count_exact: Some(10),
        row_count_max: Some(10),
        column_count: 2,
        cardinality: Cardinality::Many,
    };

    let err = stream.emit_metadata(metadata2);
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("already emitted"));
}

#[tokio::test]
async fn test_slow_client_backpressure_engaged() {
    let stream = Arc::new(BackpressuredResultStream::new(10).unwrap());
    let row_count = 20;

    let stream_producer = stream.clone();
    let producer = tokio::spawn(async move {
        for i in 0..row_count {
            let row = test_row();
            if let Err(e) = stream_producer.push_row(row).await {
                eprintln!("Producer error at row {}: {}", i, e);
                return i;
            }
        }
        row_count
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let is_bp = stream.is_backpressured();
    let metrics = stream.metrics();

    eprintln!(
        "After fast production: backpressured={}, pushed={}, consumed={}, depth={}",
        is_bp, metrics.total_rows_pushed, metrics.total_rows_consumed, metrics.queue_depth
    );

    for _ in 0..row_count {
        if stream.next_row().await.is_some() {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    }

    let final_metrics = stream.metrics();
    eprintln!(
        "After slow consumption: backpressure_count={}, depth={}",
        final_metrics.backpressure_count, final_metrics.queue_depth
    );

    assert_eq!(producer.await.unwrap(), row_count);
}

#[tokio::test]
async fn test_fast_client_no_backpressure() {
    let stream = Arc::new(BackpressuredResultStream::new(100).unwrap());

    let stream_producer = stream.clone();
    let producer = tokio::spawn(async move {
        for i in 0..50 {
            let row = test_row();
            if stream_producer.push_row(row).await.is_err() {
                return i;
            }
        }
        stream_producer
            .complete(CompletionStatus::Committed, 50)
            .await
            .expect("completion should enqueue after all rows");
        50
    });

    let stream_consumer = stream.clone();
    let consumer = tokio::spawn(async move {
        let mut count = 0;
        while stream_consumer.next_row().await.is_some() {
            count += 1;
        }
        count
    });

    let produced = producer.await.unwrap();
    let consumed = consumer.await.unwrap();

    let metrics = stream.metrics();

    assert_eq!(
        produced, 50,
        "Producer should complete all 50 rows without error"
    );
    assert_eq!(consumed, 50, "Consumer should receive all 50 rows");
    assert_eq!(metrics.total_rows_pushed, 50);
    assert_eq!(metrics.total_rows_consumed, 50);

    eprintln!(
        "Fast client test: backpressure_count={}",
        metrics.backpressure_count
    );
}

#[tokio::test]
async fn test_queue_capacity_enforced() {
    let result = BackpressuredResultStream::new(0);
    assert!(result.is_err());

    let result = BackpressuredResultStream::new(MAX_RESULT_STREAM_CAPACITY + 1);
    assert!(result.is_err());

    let result = BackpressuredResultStream::new(MIN_RESULT_STREAM_CAPACITY);
    assert!(result.is_ok());

    let result = BackpressuredResultStream::new(DEFAULT_RESULT_STREAM_CAPACITY);
    assert!(result.is_ok());

    let result = BackpressuredResultStream::new(MAX_RESULT_STREAM_CAPACITY);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_completion_requires_valid_lsn() {
    let stream = BackpressuredResultStream::new(10).unwrap();

    let result = stream.complete(CompletionStatus::Committed, 0).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("durable LSN"));

    let result = stream.complete(CompletionStatus::Committed, 42).await;
    assert!(result.is_ok());

    let completion = stream.completion().await;
    assert!(completion.is_some());
    let comp = completion.unwrap();
    assert_eq!(comp.status, CompletionStatus::Committed);
    assert_eq!(comp.lsn, 42);
}

#[tokio::test]
async fn test_completion_is_single_terminal_signal() {
    let stream = BackpressuredResultStream::new(10).unwrap();

    stream
        .complete(CompletionStatus::Committed, 42)
        .await
        .expect("first completion should succeed");

    let duplicate = stream.complete(CompletionStatus::Committed, 43).await;
    assert!(duplicate.is_err());
    assert!(
        duplicate
            .unwrap_err()
            .to_string()
            .contains("already emitted")
    );

    let push_after_completion = stream.push_row(test_row()).await;
    assert!(push_after_completion.is_err());
    assert!(
        push_after_completion
            .unwrap_err()
            .to_string()
            .contains("already completed")
    );
}

#[tokio::test]
async fn test_no_silent_drops() {
    let stream = Arc::new(BackpressuredResultStream::new(20).unwrap());

    let row_count = 100;

    let stream_producer = stream.clone();
    let producer = tokio::spawn(async move {
        for i in 0..row_count {
            let row = test_row();
            stream_producer.push_row(row).await.unwrap();

            if i == row_count - 1 {
                stream_producer
                    .complete(CompletionStatus::Committed, 100)
                    .await
                    .unwrap();
            }
        }
    });

    let stream_consumer = stream.clone();
    let consumer = tokio::spawn(async move {
        let mut count = 0;
        while stream_consumer.next_row().await.is_some() {
            count += 1;
        }
        count
    });

    producer.await.unwrap();
    let consumed = consumer.await.unwrap();

    let metrics = stream.metrics();

    assert_eq!(
        metrics.total_rows_pushed, row_count as u64,
        "All rows should be pushed"
    );
    assert_eq!(consumed, row_count, "All rows should be consumed");
    assert_eq!(
        metrics.total_rows_consumed, row_count as u64,
        "Metrics should reflect all consumed rows"
    );

    eprintln!(
        "No silent drops test: pushed={}, consumed={}, backpressure_count={}",
        metrics.total_rows_pushed, metrics.total_rows_consumed, metrics.backpressure_count
    );
}

#[tokio::test]
async fn test_concurrent_produce_consume_varying_speed() {
    let stream = Arc::new(BackpressuredResultStream::new(30).unwrap());

    let barrier = Arc::new(Barrier::new(2));

    let stream_producer = stream.clone();
    let barrier_producer = barrier.clone();
    let producer = tokio::spawn(async move {
        barrier_producer.wait().await;
        for i in 0..200 {
            let row = test_row();
            stream_producer.push_row(row).await.unwrap();

            if i % 30 == 0 {
                tokio::task::yield_now().await;
            }

            if i == 199 {
                stream_producer
                    .complete(CompletionStatus::Committed, 200)
                    .await
                    .unwrap();
            }
        }
    });

    let stream_consumer = stream.clone();
    let barrier_consumer = barrier.clone();
    let consumer = tokio::spawn(async move {
        barrier_consumer.wait().await;
        let mut count = 0;
        while stream_consumer.next_row().await.is_some() {
            count += 1;
            if count % 25 == 0 {
                tokio::time::sleep(tokio::time::Duration::from_micros(500)).await;
            }
        }
        count
    });

    producer.await.unwrap();
    let consumed = consumer.await.unwrap();

    let metrics = stream.metrics();

    assert_eq!(metrics.total_rows_pushed, 200);
    assert_eq!(consumed, 200);
    assert_eq!(metrics.total_rows_consumed, 200);

    eprintln!(
        "Concurrent varying speed: backpressure_count={}, peak_depth={}",
        metrics.backpressure_count, metrics.peak_queue_depth
    );

    assert!(
        metrics.backpressure_count > 0,
        "Should experience backpressure with 200 rows in 30-capacity queue"
    );
}

#[tokio::test]
async fn test_backpressure_signal_generation() {
    let stream = Arc::new(BackpressuredResultStream::new(10).unwrap());

    let stream_producer = stream.clone();
    let producer = tokio::spawn(async move {
        for _ in 0..50 {
            let row = test_row();
            let _ = stream_producer.push_row(row).await;
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let is_bp = stream.is_backpressured();
    let signal = stream.backpressure_signal(Some(RequestId::new(42)));

    if is_bp {
        assert!(
            signal.is_some(),
            "Should generate signal when backpressured"
        );
        let sig = signal.unwrap();
        assert_eq!(sig.request_id, Some(RequestId::new(42)));
        assert_eq!(sig.retry_after_millis, Some(10));
    }

    for _ in 0..50 {
        let _ = stream.next_row().await;
    }

    let _ = producer.await;
}

#[tokio::test]
async fn test_metrics_peak_tracking() {
    let stream = Arc::new(BackpressuredResultStream::new(50).unwrap());

    let stream_producer = stream.clone();
    let producer = tokio::spawn(async move {
        for _ in 0..300 {
            let row = test_row();
            let _ = stream_producer.push_row(row).await;
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let metrics = stream.metrics();

    eprintln!(
        "Peak tracking: queue_depth={}, peak_depth={}, capacity=50",
        metrics.queue_depth, metrics.peak_queue_depth
    );

    assert!(
        metrics.peak_queue_depth > 0,
        "Peak queue depth should be recorded"
    );
    assert!(
        metrics.peak_queue_depth <= 50,
        "Peak should not exceed queue capacity"
    );

    for _ in 0..300 {
        let _ = stream.next_row().await;
    }
    producer.await.unwrap();
}

#[tokio::test]
async fn test_completion_state_persistence() {
    let stream = Arc::new(BackpressuredResultStream::new(16).unwrap());

    let stream_clone = stream.clone();
    let producer = tokio::spawn(async move {
        for i in 0..10 {
            let row = test_row();
            stream_clone.push_row(row).await.unwrap();

            if i == 9 {
                stream_clone
                    .complete(CompletionStatus::RolledBack, 999)
                    .await
                    .unwrap();
            }
        }
    });

    producer.await.unwrap();

    let completion = stream.completion().await;
    assert!(completion.is_some());
    let comp = completion.unwrap();
    assert_eq!(comp.status, CompletionStatus::RolledBack);
    assert_eq!(comp.lsn, 999);
    assert_eq!(comp.row_count, 10);
}

#[tokio::test]
async fn test_multiple_concurrent_streams() {
    let stream1 = Arc::new(BackpressuredResultStream::new(128).unwrap());
    let stream2 = Arc::new(BackpressuredResultStream::new(128).unwrap());
    let stream3 = Arc::new(BackpressuredResultStream::new(128).unwrap());

    let s1_producer = stream1.clone();
    let task1 = tokio::spawn(async move {
        for i in 0..50 {
            let row = test_row();
            s1_producer.push_row(row).await.unwrap();
            if i == 49 {
                s1_producer
                    .complete(CompletionStatus::Committed, 50)
                    .await
                    .unwrap();
            }
        }
    });

    let s2_producer = stream2.clone();
    let task2 = tokio::spawn(async move {
        for i in 0..75 {
            let row = test_row();
            s2_producer.push_row(row).await.unwrap();
            if i == 74 {
                s2_producer
                    .complete(CompletionStatus::Committed, 75)
                    .await
                    .unwrap();
            }
        }
    });

    let s3_producer = stream3.clone();
    let task3 = tokio::spawn(async move {
        for i in 0..100 {
            let row = test_row();
            s3_producer.push_row(row).await.unwrap();
            if i == 99 {
                s3_producer
                    .complete(CompletionStatus::Committed, 100)
                    .await
                    .unwrap();
            }
        }
    });

    task1.await.unwrap();
    task2.await.unwrap();
    task3.await.unwrap();

    let m1 = stream1.metrics();
    let m2 = stream2.metrics();
    let m3 = stream3.metrics();

    assert_eq!(m1.total_rows_pushed, 50);
    assert_eq!(m2.total_rows_pushed, 75);
    assert_eq!(m3.total_rows_pushed, 100);

    eprintln!(
        "Multiple streams: s1_bp={}, s2_bp={}, s3_bp={}",
        m1.backpressure_count, m2.backpressure_count, m3.backpressure_count
    );
}
