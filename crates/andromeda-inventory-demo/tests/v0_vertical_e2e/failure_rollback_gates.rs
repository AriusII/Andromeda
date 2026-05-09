use crate::support::{
    CountingProductStockStore, assert_product_stock_untouched, context, encoded_execute_frame,
    encoded_execute_frame_with_transaction_id, executable_procedure, inventory_catalog_snapshot,
    request, stock,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_inventory_demo::V0InventoryRecoverableRuntime;
use andromeda_inventory_demo::inventory_reserve_stock_contract;
use andromeda_observe::{
    EventEmitter, EventEnvelope, EventSink, InMemoryEventSink, TraceEvent, TraceId,
    TransitionReasonCode,
};
use andromeda_rpc_protocol::FrameCodec;
use andromeda_types::{ContractHash, InvocationId};
use andromeda_wal::InMemoryWal;

#[test]
fn v0_inventory_rejects_malformed_execute_frame_before_product_stock_or_wal() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut product_stock = CountingProductStockStore::new(stock());

    let err = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            b"short frame",
            &procedure,
            request(&contract, 715),
            &context(&contract, 7015),
            &mut product_stock,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(err.message().contains("AE-V0-RSVSTK-FRAME-LEN"));
    assert!(runtime.wal().is_empty());
    assert_product_stock_untouched(&product_stock);
}

#[test]
fn v0_inventory_rejects_invalid_payload_domain_before_product_stock_or_wal() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut product_stock = CountingProductStockStore::new(stock());
    let mut encoded = encoded_execute_frame();
    encoded[FrameCodec::HEADER_LEN] ^= 0x01;

    let err = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            &encoded,
            &procedure,
            request(&contract, 716),
            &context(&contract, 7016),
            &mut product_stock,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(err.message().contains("AE-V0-RSVSTK-PAYLOAD-DOMAIN"));
    assert!(runtime.wal().is_empty());
    assert_product_stock_untouched(&product_stock);
}

#[test]
fn v0_inventory_rejects_transaction_bearing_execute_frame_before_product_stock_or_wal() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut product_stock = CountingProductStockStore::new(stock());

    let err = runtime
        .execute_encoded_inventory_reserve_stock_with_product_stock(
            &encoded_execute_frame_with_transaction_id(),
            &procedure,
            request(&contract, 717),
            &context(&contract, 7017),
            &mut product_stock,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Protocol);
    assert!(err.message().contains("AE-V0-RSVSTK-FRAME-TXID"));
    assert!(runtime.wal().is_empty());
    assert_product_stock_untouched(&product_stock);
}

#[test]
fn v0_inventory_rejects_contract_mismatch_before_wal_append() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut stale_request = request(&contract, 701);
    stale_request.expected_contract_hash = ContractHash::test_vector(0xBA);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());

    let err = runtime
        .execute_encoded_inventory_reserve_stock(
            &encoded_execute_frame(),
            &procedure,
            stale_request,
            &context(&contract, 7001),
            stock(),
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(runtime.wal().is_empty());
}

#[test]
fn v0_inventory_rejects_missing_permission_before_wal_append() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());

    let err = runtime
        .execute_encoded_inventory_reserve_stock(
            &encoded_execute_frame(),
            &procedure,
            request(&contract, 702),
            &andromeda_exec::InvocationContext::new(TraceId::new(7002), Vec::new()),
            stock(),
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(runtime.wal().is_empty());
}

#[test]
fn v0_inventory_observed_contract_rejection_emits_pre_transaction_evidence() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut stale_request = request(&contract, 707);
    stale_request.expected_contract_hash = ContractHash::test_vector(0xBA);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut sink = InMemoryEventSink::new();

    let err = runtime
        .execute_encoded_inventory_reserve_stock_observed(
            &encoded_execute_frame(),
            &procedure,
            stale_request,
            &context(&contract, 7007),
            stock(),
            &mut sink,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(runtime.wal().is_empty());
    assert_eq!(sink.events().len(), 2);
    assert!(
        sink.events()
            .iter()
            .all(|event| event.correlation.has_no_transaction_evidence())
    );
    assert!(
        sink.events()
            .iter()
            .all(|event| event.correlation.has_request_session())
    );
    assert!(matches!(
        &sink.events()[0].event,
        TraceEvent::ContractRejected(trace)
            if trace.has_reason() && trace.has_contract_evidence()
    ));
    assert!(matches!(
        &sink.events()[1].event,
        TraceEvent::ExecutionTransition(trace)
            if trace.reason_code == TransitionReasonCode::PRE_TRANSACTION_REJECTION
                && trace.transaction_id.is_none()
                && trace.durable_lsn.is_none()
    ));
}

#[test]
fn v0_inventory_observed_authorization_denial_emits_pre_transaction_evidence() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut sink = InMemoryEventSink::new();

    let err = runtime
        .execute_encoded_inventory_reserve_stock_observed(
            &encoded_execute_frame(),
            &procedure,
            request(&contract, 708),
            &andromeda_exec::InvocationContext::new(TraceId::new(7008), Vec::new()),
            stock(),
            &mut sink,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(runtime.wal().is_empty());
    assert_eq!(sink.events().len(), 2);
    assert!(
        sink.events()
            .iter()
            .all(|event| event.correlation.has_no_transaction_evidence())
    );
    assert!(matches!(
        &sink.events()[0].event,
        TraceEvent::AuthorizationDenied(trace)
            if trace.has_reason()
                && trace.has_permission_evidence()
                && trace.denied_permission == "Inventory.ReserveStock.Execute"
    ));
    assert!(matches!(
        &sink.events()[1].event,
        TraceEvent::ExecutionTransition(trace)
            if trace.reason_code == TransitionReasonCode::PERMISSION_DENIED
                && trace.transaction_id.is_none()
                && trace.durable_lsn.is_none()
    ));
}

#[test]
fn v0_inventory_observed_admission_rejection_emits_pre_transaction_evidence() {
    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut zero_invocation = request(&contract, 709);
    zero_invocation.invocation_id = InvocationId::new(0);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut sink = InMemoryEventSink::new();

    let err = runtime
        .execute_encoded_inventory_reserve_stock_observed(
            &encoded_execute_frame(),
            &procedure,
            zero_invocation,
            &context(&contract, 7009),
            stock(),
            &mut sink,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    assert!(runtime.wal().is_empty());
    assert_eq!(sink.events().len(), 1);
    assert!(sink.events()[0].correlation.has_no_transaction_evidence());
    assert!(matches!(
        &sink.events()[0].event,
        TraceEvent::ContractRejected(trace)
            if trace.has_reason()
                && trace.has_contract_evidence()
                && trace.reason.contains("InvocationId")
    ));
}

#[test]
fn v0_inventory_observed_path_surfaces_and_counts_emitter_failure() {
    #[derive(Debug)]
    struct FailingAfterAccepted {
        accepted_before_failure: usize,
        attempts: usize,
    }

    impl EventSink for FailingAfterAccepted {
        fn emit(&mut self, _event: EventEnvelope) -> AndromedaResult<()> {
            self.attempts += 1;
            if self.attempts > self.accepted_before_failure {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Internal,
                    "injected observe sink failure",
                ));
            }
            Ok(())
        }
    }

    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut emitter = EventEmitter::new(FailingAfterAccepted {
        accepted_before_failure: 2,
        attempts: 0,
    });

    let err = runtime
        .execute_encoded_inventory_reserve_stock_observed_with_emitter(
            &encoded_execute_frame(),
            &procedure,
            request(&contract, 706),
            &context(&contract, 7006),
            stock(),
            &mut emitter,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Internal);
    assert!(err.message().contains("injected observe sink failure"));
    assert_eq!(emitter.accepted_count(), 2);
    assert_eq!(emitter.rejected_count(), 1);
    assert_eq!(emitter.sink().attempts, 3);
    assert_eq!(
        emitter.peek_next_event_id().map(|event_id| event_id.get()),
        Some(3)
    );
    assert_eq!(runtime.wal().replay_durable().len(), 3);
}

#[test]
fn v0_inventory_observed_rejection_surfaces_and_counts_emitter_failure_without_wal() {
    #[derive(Debug)]
    struct FailingImmediately {
        attempts: usize,
    }

    impl EventSink for FailingImmediately {
        fn emit(&mut self, _event: EventEnvelope) -> AndromedaResult<()> {
            self.attempts += 1;
            Err(AndromedaError::new(
                AndromedaErrorKind::Internal,
                "injected pre-transaction observe sink failure",
            ))
        }
    }

    let catalog = inventory_catalog_snapshot();
    let contract = inventory_reserve_stock_contract().unwrap();
    let procedure = executable_procedure(&catalog, &contract);
    let mut stale_request = request(&contract, 710);
    stale_request.expected_contract_hash = ContractHash::test_vector(0xBA);
    let mut runtime = V0InventoryRecoverableRuntime::new(InMemoryWal::new());
    let mut emitter = EventEmitter::new(FailingImmediately { attempts: 0 });

    let err = runtime
        .execute_encoded_inventory_reserve_stock_observed_with_emitter(
            &encoded_execute_frame(),
            &procedure,
            stale_request,
            &context(&contract, 7010),
            stock(),
            &mut emitter,
        )
        .unwrap_err();

    assert_eq!(err.kind(), AndromedaErrorKind::Internal);
    assert!(
        err.message()
            .contains("injected pre-transaction observe sink failure")
    );
    assert_eq!(emitter.accepted_count(), 0);
    assert_eq!(emitter.rejected_count(), 1);
    assert_eq!(emitter.sink().attempts, 1);
    assert!(runtime.wal().is_empty());
}
