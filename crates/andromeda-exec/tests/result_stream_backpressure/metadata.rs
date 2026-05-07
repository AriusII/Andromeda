use crate::support::test_row;
use andromeda_exec::{
    BackpressuredResultStream, DEFAULT_RESULT_STREAM_CAPACITY, MAX_RESULT_STREAM_CAPACITY,
    MIN_RESULT_STREAM_CAPACITY, ResultStreamMetadata,
};
use andromeda_srpl::Cardinality;

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
async fn test_payload_before_metadata_rejected() {
    let stream = BackpressuredResultStream::new(10).unwrap();

    let err = stream
        .push_row(test_row())
        .await
        .expect_err("payload must not flow before metadata");

    assert!(err.to_string().contains("metadata"));
    assert!(err.to_string().contains("before payload"));
}

#[tokio::test]
async fn test_queue_capacity_enforced() {
    for capacity in [0, MAX_RESULT_STREAM_CAPACITY + 1] {
        assert!(BackpressuredResultStream::new(capacity).is_err());
    }

    for capacity in [
        MIN_RESULT_STREAM_CAPACITY,
        DEFAULT_RESULT_STREAM_CAPACITY,
        MAX_RESULT_STREAM_CAPACITY,
    ] {
        assert!(BackpressuredResultStream::new(capacity).is_ok());
    }
}
