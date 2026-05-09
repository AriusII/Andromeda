use crate::support::{push_rows_and_complete, stream_with_metadata, test_row};
use andromeda_result_stream::{BackpressuredResultStream, CompletionStatus, ResultStreamMetadata};
use andromeda_srpl_cardinality::Cardinality;
use std::sync::Arc;

#[tokio::test]
async fn test_completion_requires_valid_lsn() {
    let stream = stream_with_metadata(10);

    let result = stream.complete(CompletionStatus::Committed, 0).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("durable LSN"));

    let result = stream.complete(CompletionStatus::Committed, 42).await;
    assert!(result.is_ok());

    let completion = stream.completion().await;
    assert!(completion.is_some());
    let comp = completion.unwrap();
    assert_eq!(comp.status(), CompletionStatus::Committed);
    assert_eq!(comp.lsn(), 42);
}

#[tokio::test]
async fn test_completion_requires_metadata_before_terminal_signal() {
    let stream = BackpressuredResultStream::new(10).unwrap();

    let err = stream
        .complete(CompletionStatus::Committed, 42)
        .await
        .expect_err("terminal completion must not flow before metadata");

    assert!(err.to_string().contains("metadata"));
    assert!(err.to_string().contains("terminal completion"));
    assert!(stream.completion().await.is_none());
}

#[tokio::test]
async fn test_payload_row_must_match_result_metadata_shape() {
    let mut stream = BackpressuredResultStream::new(10).unwrap();
    stream
        .emit_metadata(ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: None,
            row_count_max: Some(10),
            column_count: 2,
            cardinality: Cardinality::Many,
        })
        .unwrap();

    let err = stream
        .push_row(test_row())
        .await
        .expect_err("row shape must match metadata before enqueue");

    assert!(err.to_string().contains("column_count"));
    assert_eq!(stream.metrics().total_rows_pushed, 0);
}

#[tokio::test]
async fn test_completion_is_single_terminal_signal() {
    let stream = stream_with_metadata(10);

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
async fn test_completion_waits_for_admitted_blocked_rows_before_terminal_count() {
    let stream = Arc::new(stream_with_metadata(1));

    stream.push_row(test_row()).await.unwrap();

    let producer_stream = stream.clone();
    let blocked_producer = tokio::spawn(async move { producer_stream.push_row(test_row()).await });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    assert_eq!(stream.metrics().queue_depth, 1);

    let completer_stream = stream.clone();
    let completer = tokio::spawn(async move {
        completer_stream
            .complete(CompletionStatus::Committed, 777)
            .await
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    assert!(
        stream.completion().await.is_none(),
        "completion must not publish terminal evidence while an admitted row send is blocked"
    );

    assert!(stream.next_row().await.is_some());
    blocked_producer
        .await
        .expect("producer task should finish")
        .expect("admitted row should enqueue before completion closes the stream");

    assert!(stream.next_row().await.is_some());
    completer
        .await
        .expect("completion task should finish")
        .expect("completion should enqueue after admitted rows");

    let completion = stream.completion().await.unwrap();
    assert_eq!(completion.status(), CompletionStatus::Committed);
    assert_eq!(completion.lsn(), 777);
    assert_eq!(completion.row_count(), 2);
    assert!(stream.next_row().await.is_none());
}

#[tokio::test]
async fn test_completion_state_persistence() {
    let stream = Arc::new(stream_with_metadata(16));

    push_rows_and_complete(stream.clone(), 10, 999).await;

    let completion = stream.completion().await;
    assert!(completion.is_some());
    let comp = completion.unwrap();
    assert_eq!(comp.status(), CompletionStatus::Committed);
    assert_eq!(comp.lsn(), 999);
    assert_eq!(comp.row_count(), 10);
}

#[tokio::test]
async fn test_rolled_back_completion_rejects_payload_rows() {
    let stream = stream_with_metadata(16);

    stream.push_row(test_row()).await.unwrap();

    let err = stream
        .complete(CompletionStatus::RolledBack, 999)
        .await
        .expect_err("rolled-back completion must not report payload rows");

    assert!(err.to_string().contains("zero rows"));
    assert!(stream.completion().await.is_none());

    stream
        .complete(CompletionStatus::Committed, 1_000)
        .await
        .expect("failed terminal validation must not poison the stream");
    let completion = stream
        .completion()
        .await
        .expect("committed completion should be recorded after retry");
    assert_eq!(completion.status(), CompletionStatus::Committed);
    assert_eq!(completion.row_count(), 1);
}
