use crate::support::{
    context, encoded_execute_frame, executable_procedure, inventory_catalog_snapshot, request,
    stock,
};
use andromeda_catalog::inventory_reserve_stock_contract;
use andromeda_exec::CompletionStatus;
use andromeda_inventory_demo::V0InventoryRecoverableRuntime;
use andromeda_observe::{CriticalDecisionKind, InMemoryEventSink, TraceEvent};
use andromeda_rpc_protocol::{
    FrameType, StreamRole, validate_result_stream_sequence, validate_single_frame_on_stream,
};
use andromeda_wal::{InMemoryWal, Lsn};

#[test]
fn v0_inventory_executes_from_bound_pdf_style_srpl_and_emits_ordered_result_frames() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());

    assert_eq!(procedure.contract.as_ref(), contract.as_ref());
    assert_eq!(procedure.binding, contract.binding());
    assert_ne!(procedure.srpl_source_digest, [0; 32]);

    let outcome = runtime
        .execute_encoded_inventory_reserve_stock(
            &encoded_execute_frame(),
            &procedure,
            request(&contract, 700),
            &context(&contract, 7000),
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
        outcome.srpl_plan.evidence.procedure_contract,
        contract.as_ref()
    );
    assert_eq!(outcome.srpl_plan, procedure.srpl_plan);
    assert_eq!(outcome.effect.result.remaining_quantity, 7);
    assert_eq!(runtime.wal().durable_lsn(), Lsn::new(3));
    assert_eq!(runtime.wal().replay_durable().len(), 3);

    assert_eq!(outcome.result_frames.len(), 3);
    assert_eq!(
        outcome.result_frames[0].header.frame_type,
        FrameType::RpcMetadata
    );
    assert_eq!(
        outcome.result_frames[1].header.frame_type,
        FrameType::RpcBatch
    );
    assert_eq!(
        outcome.result_frames[2].header.frame_type,
        FrameType::RpcCompletion
    );
    validate_result_stream_sequence(&outcome.result_frames).unwrap();
    for frame in &outcome.result_frames {
        validate_single_frame_on_stream(frame, StreamRole::ResultUnidirectional).unwrap();
    }
}

#[test]
fn v0_inventory_observed_path_emits_required_audit_envelopes() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut sink = InMemoryEventSink::new();

    runtime
        .execute_encoded_inventory_reserve_stock_observed(
            &encoded_execute_frame(),
            &procedure,
            request(&contract, 705),
            &context(&contract, 7005),
            stock(),
            &mut sink,
        )
        .unwrap();

    assert_eq!(sink.events().len(), 4);
    assert!(sink.events()[0].correlation.has_contract_catalog());
    assert!(sink.events()[0].correlation.has_no_transaction_evidence());
    assert_eq!(sink.events()[0].event_id.get(), 1);
    assert_eq!(sink.events()[3].event_id.get(), 4);
    assert!(matches!(
        &sink.events()[0].event,
        TraceEvent::Decision(decision)
            if decision.decision == CriticalDecisionKind::ResourceGovernance
    ));
    assert!(matches!(
        &sink.events()[1].event,
        TraceEvent::Decision(decision)
            if decision.decision == CriticalDecisionKind::ContractValidation
    ));
    assert!(matches!(
        &sink.events()[2].event,
        TraceEvent::Decision(decision)
            if decision.decision == CriticalDecisionKind::SecurityAuthorization
    ));
    assert_eq!(sink.events()[3].correlation.durable_lsn, Some(3));
}
