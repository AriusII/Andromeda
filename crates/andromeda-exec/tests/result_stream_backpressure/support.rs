use andromeda_core::{ColumnDescriptor, ContractHash, ScalarType, TypeDescriptor};
use andromeda_exec::{BackpressuredResultStream, CompletionStatus, ResultStreamMetadata};
use andromeda_proto::{RowCountPolicy, StructuredObjectHeader, StructuredObjectLayout};
use andromeda_srpl::Cardinality;
use std::sync::Arc;

pub fn test_row() -> StructuredObjectHeader {
    let fields = vec![ColumnDescriptor {
        name: "value".to_string(),
        data_type: TypeDescriptor::required(ScalarType::I64),
        ordinal: 0,
    }];
    let layout = StructuredObjectLayout::RowMajor;

    StructuredObjectHeader {
        name: "test.row".to_string(),
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

pub const fn test_metadata() -> ResultStreamMetadata {
    ResultStreamMetadata {
        stream_id: 1,
        row_count_exact: None,
        row_count_max: Some(1_000),
        column_count: 1,
        cardinality: Cardinality::Many,
    }
}

pub fn stream_with_metadata(capacity: usize) -> BackpressuredResultStream {
    let mut stream = BackpressuredResultStream::new(capacity).unwrap();
    stream.emit_metadata(test_metadata()).unwrap();
    stream
}

pub async fn push_rows(stream: Arc<BackpressuredResultStream>, row_count: usize) {
    for _ in 0..row_count {
        stream.push_row(test_row()).await.unwrap();
    }
}

pub async fn push_rows_and_complete(
    stream: Arc<BackpressuredResultStream>,
    row_count: usize,
    lsn: u64,
) -> usize {
    push_rows(stream.clone(), row_count).await;
    stream
        .complete(CompletionStatus::Committed, lsn)
        .await
        .expect("completion should enqueue after all rows");
    row_count
}

pub async fn consume_until_completion(stream: Arc<BackpressuredResultStream>) -> usize {
    let mut count = 0;
    while stream.next_row().await.is_some() {
        count += 1;
    }
    count
}

pub async fn drain_rows(stream: &BackpressuredResultStream, row_count: usize) {
    for _ in 0..row_count {
        let _ = stream.next_row().await;
    }
}
