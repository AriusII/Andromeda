use crate::support::{
    context, encoded_execute_frame, executable_procedure, inventory_catalog_snapshot, request,
    stock,
};
use andromeda_catalog::inventory_reserve_stock_contract;
use andromeda_core::{
    AndromedaErrorKind, ColumnDescriptor, ContractHash, RequestId, ScalarType, TypeDescriptor,
};
use andromeda_exec::{BackpressuredResultStream, CompletionStatus, ResultStreamMetadata};
use andromeda_inventory_demo::{V0InventoryRecoverableOutcome, V0InventoryRecoverableRuntime};
use andromeda_proto::{RowCountPolicy, StructuredObjectHeader, StructuredObjectLayout};
use andromeda_rpc_protocol::{
    BackpressureReason, FrameCodec, FrameType, StreamRole, validate_result_stream_sequence,
    validate_single_frame_on_stream,
};
use andromeda_srpl_ir::Cardinality;
use andromeda_wal::{InMemoryWal, Lsn};

fn execute_v0_inventory_reserve_stock(
    invocation_id: u64,
    trace_id: u64,
) -> V0InventoryRecoverableOutcome {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());

    runtime
        .execute_encoded_inventory_reserve_stock(
            &encoded_execute_frame(),
            &procedure,
            request(&contract, invocation_id),
            &context(&contract, trace_id),
            stock(),
        )
        .unwrap()
}

fn result_stream_row() -> StructuredObjectHeader {
    let fields = vec![ColumnDescriptor {
        name: "reservation_id".to_string(),
        data_type: TypeDescriptor::required(ScalarType::U64),
        ordinal: 0,
    }];
    let layout = StructuredObjectLayout::RowMajor;

    StructuredObjectHeader {
        name: "Inventory.ReserveStock.Reservation".to_string(),
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

#[test]
fn v0_inventory_result_stream_metadata_precedes_batch_and_typed_completion() {
    let outcome = execute_v0_inventory_reserve_stock(718, 7018);

    assert_eq!(
        outcome.vertical.completion.status(),
        CompletionStatus::Committed
    );
    assert_eq!(outcome.vertical.completion.rows_affected(), Some(2));
    assert_eq!(outcome.vertical.completion.durable_lsn(), Some(Lsn::new(3)));
    assert_eq!(
        outcome
            .result_frames
            .iter()
            .map(|frame| frame.header.frame_type)
            .collect::<Vec<_>>(),
        vec![
            FrameType::RpcMetadata,
            FrameType::RpcBatch,
            FrameType::RpcCompletion,
        ]
    );
    assert!(
        outcome
            .result_frames
            .iter()
            .all(|frame| frame.header.payload_length > 0)
    );

    // V0 still carries legacy raw ResultStream payloads. This packet proves the
    // binary frame boundary before applying legacy sequence validation.
    let mut encoded_result_stream = Vec::new();
    for frame in &outcome.result_frames {
        encoded_result_stream.extend(FrameCodec::encode(frame).unwrap());
    }
    let scanned_result_frames = FrameCodec::scan_all(&encoded_result_stream).unwrap();
    assert_eq!(
        scanned_result_frames
            .iter()
            .map(|frame| frame.header.frame_type)
            .collect::<Vec<_>>(),
        vec![
            FrameType::RpcMetadata,
            FrameType::RpcBatch,
            FrameType::RpcCompletion,
        ]
    );

    validate_result_stream_sequence(&scanned_result_frames).unwrap();
    for frame in &scanned_result_frames {
        validate_single_frame_on_stream(frame, StreamRole::ResultUnidirectional).unwrap();
    }
}

#[test]
fn v0_inventory_result_stream_rejects_batch_after_terminal_completion() {
    let outcome = execute_v0_inventory_reserve_stock(719, 7019);
    let mut frames = outcome.result_frames.clone();
    frames.push(outcome.result_frames[1].clone());

    let error = validate_result_stream_sequence(&frames)
        .expect_err("ResultStream sequence must reject payload after completion");

    assert_eq!(error.kind(), AndromedaErrorKind::Protocol);
    assert!(
        error.message().contains("batch must not follow completion"),
        "terminal frame ordering rejection should identify the late batch"
    );
}

#[tokio::test]
async fn backpressured_result_stream_signal_is_request_scoped_and_preserves_row_counts() {
    let mut stream = BackpressuredResultStream::new(1).unwrap();
    stream
        .emit_metadata(ResultStreamMetadata::bounded(77, 1, Cardinality::Many, 1))
        .unwrap();

    stream.push_row(result_stream_row()).await.unwrap();
    let metrics = stream.metrics();
    assert_eq!(metrics.queue_depth, 1);
    assert_eq!(metrics.total_rows_pushed, 1);
    assert!(stream.is_backpressured());

    let signal = stream
        .backpressure_signal(Some(RequestId::new(42)))
        .unwrap();
    assert_eq!(signal.reason, BackpressureReason::ResultSpoolGrowth);
    assert_eq!(signal.request_id, Some(RequestId::new(42)));
    assert_eq!(signal.retry_after_millis, Some(10));
    assert!(
        stream.backpressure_signal(None).is_none(),
        "ResultStream backpressure evidence must remain request scoped"
    );

    assert!(stream.next_row().await.is_some());
    stream
        .complete(CompletionStatus::Committed, 44)
        .await
        .unwrap();

    let completion = stream.completion().await.unwrap();
    assert_eq!(completion.status(), CompletionStatus::Committed);
    assert_eq!(completion.lsn(), 44);
    assert_eq!(completion.row_count(), 1);
    assert_eq!(stream.metrics().total_rows_consumed, 1);
    assert!(stream.next_row().await.is_none());
}
