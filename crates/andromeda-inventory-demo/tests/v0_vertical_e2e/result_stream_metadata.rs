use crate::support::{
    context, encoded_execute_frame, executable_procedure, inventory_catalog_snapshot, request,
    stock,
};
use andromeda_error::AndromedaErrorKind;
use andromeda_inventory_demo::inventory_reserve_stock_contract;
use andromeda_inventory_demo::{V0InventoryRecoverableOutcome, V0InventoryRecoverableRuntime};
use andromeda_result_stream::CompletionStatus;
use andromeda_rpc_protocol::{
    FrameCodec, FrameType, StreamRole, validate_result_stream_sequence,
    validate_single_frame_on_stream,
};
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

    // V0 still carries compatibility raw ResultStream payloads. This packet proves the
    // binary frame boundary before applying compatibility sequence validation.
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
