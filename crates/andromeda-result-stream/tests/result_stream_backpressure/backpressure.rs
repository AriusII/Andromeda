use crate::support::{
    consume_until_completion, drain_rows, push_rows, push_rows_and_complete, stream_with_metadata,
    test_row,
};
use andromeda_types::RequestId;
use std::sync::Arc;

#[tokio::test]
async fn test_slow_client_backpressure_engaged() {
    let stream = Arc::new(stream_with_metadata(10));
    let row_count = 20;

    let producer_stream = stream.clone();
    let producer = tokio::spawn(async move {
        for i in 0..row_count {
            if let Err(e) = producer_stream.push_row(test_row()).await {
                eprintln!("Producer error at row {}: {}", i, e);
                return i;
            }
        }
        row_count
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    assert!(
        stream.is_backpressured(),
        "slow consumer should engage bounded result-stream backpressure"
    );
    let metrics = stream.metrics();
    assert!(
        metrics.backpressure_count > 0,
        "producer should observe at least one full bounded queue"
    );
    assert!(
        metrics.peak_queue_depth <= 10,
        "bounded queue depth must not exceed capacity"
    );

    for _ in 0..row_count {
        if stream.next_row().await.is_some() {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    }

    assert_eq!(producer.await.unwrap(), row_count);
}

#[tokio::test]
async fn test_fast_client_no_backpressure() {
    let stream = Arc::new(stream_with_metadata(100));

    let producer = tokio::spawn(push_rows_and_complete(stream.clone(), 50, 50));
    let consumer = tokio::spawn(consume_until_completion(stream.clone()));

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
}

#[tokio::test]
async fn test_no_silent_drops() {
    let stream = Arc::new(stream_with_metadata(20));
    let row_count = 100;

    let producer = tokio::spawn(push_rows_and_complete(stream.clone(), row_count, 100));
    let consumer = tokio::spawn(consume_until_completion(stream.clone()));

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
}

#[tokio::test]
async fn test_backpressure_signal_generation() {
    let stream = Arc::new(stream_with_metadata(10));

    let producer = tokio::spawn(push_rows(stream.clone(), 50));
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    assert!(
        stream.is_backpressured(),
        "filled bounded stream should report backpressure before draining"
    );
    let signal = stream.backpressure_signal(Some(RequestId::new(42)));
    assert!(
        signal.is_some(),
        "Should generate signal when backpressured"
    );
    let sig = signal.unwrap();
    assert_eq!(sig.request_id, Some(RequestId::new(42)));
    assert_eq!(sig.retry_after_millis, Some(10));

    drain_rows(&stream, 50).await;
    producer.await.unwrap();
}

#[tokio::test]
async fn test_backpressure_signal_requires_request_identity() {
    let stream = Arc::new(stream_with_metadata(10));

    let producer = tokio::spawn(push_rows(stream.clone(), 50));
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    assert!(
        stream.is_backpressured(),
        "filled bounded stream should report backpressure before draining"
    );
    assert!(
        stream.backpressure_signal(None).is_none(),
        "backpressure evidence must carry request identity"
    );

    drain_rows(&stream, 50).await;
    producer.await.unwrap();
}

#[tokio::test]
async fn test_metrics_peak_tracking() {
    let stream = Arc::new(stream_with_metadata(50));

    let producer = tokio::spawn(push_rows(stream.clone(), 300));
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let metrics = stream.metrics();

    assert!(
        metrics.peak_queue_depth > 0,
        "Peak queue depth should be recorded"
    );
    assert!(
        metrics.peak_queue_depth <= 50,
        "Peak should not exceed queue capacity"
    );

    drain_rows(&stream, 300).await;
    producer.await.unwrap();
}
