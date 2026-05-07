use crate::support::{
    context, encoded_execute_frame, executable_procedure, inventory_catalog_snapshot, request,
    stock,
};
use andromeda_catalog::inventory_reserve_stock_contract;
use andromeda_exec::{CompletionStatus, V0InventoryRecoverableRuntime};
use andromeda_quic::{
    FrameType, StreamRole, validate_result_stream_sequence, validate_single_frame_on_stream,
};
use andromeda_storage::{InMemoryWal, Lsn};

#[test]
fn v0_inventory_result_stream_metadata_precedes_batch_and_typed_completion() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());

    let outcome = runtime
        .execute_encoded_inventory_reserve_stock(
            &encoded_execute_frame(),
            &procedure,
            request(&contract, 718),
            &context(&contract, 7018),
            stock(),
        )
        .unwrap();

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

    validate_result_stream_sequence(&outcome.result_frames).unwrap();
    for frame in &outcome.result_frames {
        validate_single_frame_on_stream(frame, StreamRole::ResultUnidirectional).unwrap();
    }
}
