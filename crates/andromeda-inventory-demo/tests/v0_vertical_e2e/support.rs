use andromeda_catalog::{CatalogSnapshot, CatalogSystemStore};
use andromeda_error::AndromedaResult;
use andromeda_exec::{InvocationContext, InvocationRequest};
use andromeda_inventory_demo::{
    INVENTORY_DATABASE_ID, INVENTORY_NAMESPACE_ID, InventoryProductStockCommitEvidence,
    InventoryProductStockReservationIntent, InventoryProductStockStore, InventoryStock,
    V0InventoryReserveStockExecutableProcedure, V0InventoryReserveStockRpcPayload,
    bind_inventory_reserve_stock_v0_pdf_executable_procedure,
    encode_inventory_reserve_stock_v0_execute_frame, inventory_domain_definition_batch,
};
use andromeda_observability::TraceId;
use andromeda_procedure_contract::ProcedureContract;
use andromeda_rpc_protocol::{
    FRAME_HEADER_CRC_UNCHECKED, FrameBytes, FrameCodec, FrameHeader, FrameType,
};
use andromeda_storage_heap::LocalHeapRowInsertRedoTemplate;
use andromeda_storage_page::{PageId, PageSize};
use andromeda_types::CatalogVersion;
use andromeda_types::{InvocationId, RequestId, SessionId, TransactionId};

pub(crate) fn inventory_catalog_snapshot() -> CatalogSnapshot {
    let mut store = CatalogSystemStore::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        CatalogVersion::new(0),
    );
    let mut next_lsn: u64 = 0;
    store
        .apply_definition_batch_durably(
            &inventory_domain_definition_batch().unwrap(),
            |_kind, _payload| {
                next_lsn += 1;
                Ok(next_lsn)
            },
            Ok,
        )
        .unwrap();
    store.into_snapshot()
}

pub(crate) fn request(contract: &ProcedureContract, invocation_id: u64) -> InvocationRequest {
    InvocationRequest {
        invocation_id: InvocationId::new(invocation_id),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    }
}

pub(crate) fn context(contract: &ProcedureContract, trace_id: u64) -> InvocationContext {
    InvocationContext::new(
        TraceId::new(trace_id.into()),
        contract.required_permissions.clone(),
    )
}

pub(crate) fn executable_procedure(
    catalog: &CatalogSnapshot,
    contract: &ProcedureContract,
) -> V0InventoryReserveStockExecutableProcedure {
    bind_inventory_reserve_stock_v0_pdf_executable_procedure(catalog, contract).unwrap()
}

pub(crate) fn stock() -> InventoryStock {
    InventoryStock {
        product_id: 42,
        available_quantity: 10,
        version: 7,
    }
}

pub(crate) fn encoded_execute_frame() -> Vec<u8> {
    encode_inventory_reserve_stock_v0_execute_frame(
        RequestId::new(900),
        SessionId::new(901),
        V0InventoryReserveStockRpcPayload::new(42, 3).unwrap(),
    )
    .unwrap()
}

pub(crate) fn encoded_execute_frame_with_transaction_id() -> Vec<u8> {
    let payload = V0InventoryReserveStockRpcPayload::new(42, 3)
        .unwrap()
        .encode()
        .unwrap();
    FrameCodec::encode(&FrameBytes {
        header: FrameHeader {
            frame_type: FrameType::RpcExecuteRequest,
            request_id: RequestId::new(902),
            session_id: SessionId::new(903),
            tx_id: Some(TransactionId::new(904)),
            payload_length: payload.len() as u64,
            flags: 0,
            header_crc: FRAME_HEADER_CRC_UNCHECKED,
        },
        payload,
    })
    .unwrap()
}

#[derive(Debug)]
pub(crate) struct CountingProductStockStore {
    pub(crate) inner: andromeda_inventory_demo::HeapInventoryProductStockStore,
    pub(crate) prepare_count: usize,
    pub(crate) publish_count: usize,
    pub(crate) abort_count: usize,
}

impl CountingProductStockStore {
    pub(crate) fn new(stock: InventoryStock) -> Self {
        Self {
            inner: andromeda_inventory_demo::HeapInventoryProductStockStore::from_cold_snapshot(
                PageId::new(42_900),
                PageSize::KiB16,
                stock,
            )
            .unwrap(),
            prepare_count: 0,
            publish_count: 0,
            abort_count: 0,
        }
    }
}

impl InventoryProductStockStore for CountingProductStockStore {
    fn prepare_reserve_stock(
        &mut self,
        command: andromeda_inventory_demo::ReserveStockCommand,
    ) -> AndromedaResult<InventoryProductStockReservationIntent> {
        self.prepare_count += 1;
        self.inner.prepare_reserve_stock(command)
    }

    fn prepared_reserve_stock_redo_template(
        &self,
        intent: &InventoryProductStockReservationIntent,
    ) -> AndromedaResult<LocalHeapRowInsertRedoTemplate> {
        self.inner.prepared_reserve_stock_redo_template(intent)
    }

    fn publish_committed_reserve_stock_with_redo(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        commit: InventoryProductStockCommitEvidence,
        redo: andromeda_inventory_demo::InventoryProductStockDurableRedoEvidence,
    ) -> AndromedaResult<()> {
        self.publish_count += 1;
        self.inner
            .publish_committed_reserve_stock_with_redo(intent, commit, redo)
    }

    fn abort_prepared_reserve_stock(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        reason: &str,
    ) -> AndromedaResult<()> {
        self.abort_count += 1;
        self.inner.abort_prepared_reserve_stock(intent, reason)
    }
}

pub(crate) fn assert_product_stock_untouched(product_stock: &CountingProductStockStore) {
    assert_eq!(product_stock.prepare_count, 0);
    assert_eq!(product_stock.publish_count, 0);
    assert_eq!(product_stock.abort_count, 0);
    assert_eq!(product_stock.inner.visible_stock().unwrap(), stock());
    assert!(product_stock.inner.prepared_intent().is_none());
    assert!(product_stock.inner.published_commit().is_none());
}
