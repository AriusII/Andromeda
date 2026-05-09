use crate::support::{push_rows_and_complete, stream_with_metadata, test_row};
use andromeda_result_stream::CompletionStatus;
use std::sync::Arc;
use tokio::sync::Barrier;

#[tokio::test]
async fn test_concurrent_produce_consume_varying_speed() {
    let stream = Arc::new(stream_with_metadata(30));
    let barrier = Arc::new(Barrier::new(2));

    let producer_stream = stream.clone();
    let producer_barrier = barrier.clone();
    let producer = tokio::spawn(async move {
        producer_barrier.wait().await;
        for i in 0..200 {
            producer_stream.push_row(test_row()).await.unwrap();

            if i % 30 == 0 {
                tokio::task::yield_now().await;
            }

            if i == 199 {
                producer_stream
                    .complete(CompletionStatus::Committed, 200)
                    .await
                    .unwrap();
            }
        }
    });

    let consumer_stream = stream.clone();
    let consumer_barrier = barrier.clone();
    let consumer = tokio::spawn(async move {
        consumer_barrier.wait().await;
        let mut count = 0;
        while consumer_stream.next_row().await.is_some() {
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
    assert!(
        metrics.backpressure_count > 0,
        "Should experience backpressure with 200 rows in 30-capacity queue"
    );
}

#[tokio::test]
async fn test_multiple_concurrent_streams() {
    let stream1 = Arc::new(stream_with_metadata(128));
    let stream2 = Arc::new(stream_with_metadata(128));
    let stream3 = Arc::new(stream_with_metadata(128));

    let task1 = tokio::spawn(push_rows_and_complete(stream1.clone(), 50, 50));
    let task2 = tokio::spawn(push_rows_and_complete(stream2.clone(), 75, 75));
    let task3 = tokio::spawn(push_rows_and_complete(stream3.clone(), 100, 100));

    task1.await.unwrap();
    task2.await.unwrap();
    task3.await.unwrap();

    let m1 = stream1.metrics();
    let m2 = stream2.metrics();
    let m3 = stream3.metrics();

    assert_eq!(m1.total_rows_pushed, 50);
    assert_eq!(m2.total_rows_pushed, 75);
    assert_eq!(m3.total_rows_pushed, 100);
}
