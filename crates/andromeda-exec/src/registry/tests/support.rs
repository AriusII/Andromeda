pub(crate) use super::super::{ProcedureHandler, ProcedureRegistry, ReserveStockProcedureHandler};
pub(crate) use crate::{
    InventoryReserveStockExecutor, InventoryStock, InvocationContext, LocalProcedure,
    PreTransactionDispatchEvidence, ProcedureDispatchRequest, ProcedureDispatcher,
    RemoteProcedureDispatcherUnavailable, ReserveStockCommand, ResultStreamMetadata,
};
pub(crate) use andromeda_catalog::{
    INVENTORY_RESERVE_STOCK_PERMISSION, PolicyVersion, ProcedureContractBinding,
    ProcedureContractRef, StatsVersion, inventory_reserve_stock_contract,
};
pub(crate) use andromeda_core::{
    AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, InvocationId, ProcedureId,
};
pub(crate) use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
pub(crate) use andromeda_srpl::Cardinality;

pub(crate) const TEST_INVENTORY_QUERY_STOCK_PROCEDURE_ID: ProcedureId = ProcedureId::new(0x5153);
pub(crate) const TEST_INVENTORY_QUERY_STOCK_PERMISSION: &str = "Inventory.QueryStock.Execute";
pub(crate) const TEST_INVENTORY_QUERY_STOCK_STREAM_ID: u64 = 2;
pub(crate) const TEST_INVENTORY_QUERY_STOCK_COLUMN_COUNT: u32 = 3;
pub(crate) const TEST_INVENTORY_RELEASE_STOCK_PROCEDURE_ID: ProcedureId = ProcedureId::new(0x524c);
pub(crate) const TEST_INVENTORY_RELEASE_STOCK_PERMISSION: &str = "Inventory.ReleaseStock.Execute";
pub(crate) const TEST_INVENTORY_RELEASE_STOCK_STREAM_ID: u64 = 3;
pub(crate) const TEST_INVENTORY_RELEASE_STOCK_COLUMN_COUNT: u32 = 4;

#[derive(Debug, Clone, Copy)]
pub(crate) struct FakeProcedureHandler {
    pub(crate) contract: ProcedureContractRef,
    pub(crate) result_contract: ProcedureContractRef,
    pub(crate) rows_affected: u64,
    pub(crate) invalid_payload_shape: bool,
}

impl ProcedureHandler for FakeProcedureHandler {
    fn procedure_id(&self) -> ProcedureId {
        self.contract.procedure_id
    }

    fn contract(&self) -> ProcedureContractRef {
        self.contract
    }

    fn result_metadata(&self) -> ResultStreamMetadata {
        ResultStreamMetadata::exact(7, 1, Cardinality::One, 1)
    }

    fn execute(&self, _context: InvocationContext) -> AndromedaResult<LocalProcedure> {
        Ok(LocalProcedure {
            contract: self.result_contract,
            contract_binding: binding_for(self.result_contract),
            required_permissions: vec!["inventory.reserve".to_string()],
            result_metadata: self.result_metadata(),
            mutation_payload: if self.invalid_payload_shape {
                Vec::new()
            } else {
                vec![1]
            },
            rows_affected: self.rows_affected,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct QueryStockFakeProcedureHandler {
    contract: ProcedureContractRef,
    stock: InventoryStock,
}

impl QueryStockFakeProcedureHandler {
    pub(crate) fn new(stock: InventoryStock) -> Self {
        Self {
            contract: test_inventory_query_stock_contract_ref(),
            stock,
        }
    }

    fn payload(self) -> Vec<u8> {
        let mut payload = Vec::with_capacity(48);
        payload.extend_from_slice(b"test.Inventory.QueryStock");
        payload.push(0);
        payload.extend_from_slice(&self.stock.product_id.to_le_bytes());
        payload.extend_from_slice(&self.stock.available_quantity.to_le_bytes());
        payload.extend_from_slice(&self.stock.version.to_le_bytes());
        payload
    }
}

impl ProcedureHandler for QueryStockFakeProcedureHandler {
    fn procedure_id(&self) -> ProcedureId {
        self.contract.procedure_id
    }

    fn contract(&self) -> ProcedureContractRef {
        self.contract
    }

    fn result_metadata(&self) -> ResultStreamMetadata {
        ResultStreamMetadata::exact(
            TEST_INVENTORY_QUERY_STOCK_STREAM_ID,
            TEST_INVENTORY_QUERY_STOCK_COLUMN_COUNT,
            Cardinality::One,
            1,
        )
    }

    fn execute(&self, _context: InvocationContext) -> AndromedaResult<LocalProcedure> {
        Ok(LocalProcedure {
            contract: self.contract,
            contract_binding: binding_for(self.contract),
            required_permissions: vec![TEST_INVENTORY_QUERY_STOCK_PERMISSION.to_string()],
            result_metadata: self.result_metadata(),
            mutation_payload: self.payload(),
            rows_affected: 0,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ReleaseStockFakeProcedureHandler {
    contract: ProcedureContractRef,
    previous_stock: InventoryStock,
    released_quantity: i64,
}

impl ReleaseStockFakeProcedureHandler {
    pub(crate) fn new(previous_stock: InventoryStock, released_quantity: i64) -> Self {
        Self {
            contract: test_inventory_release_stock_contract_ref(),
            previous_stock,
            released_quantity,
        }
    }

    fn next_stock(self) -> InventoryStock {
        InventoryStock {
            product_id: self.previous_stock.product_id,
            available_quantity: self.previous_stock.available_quantity + self.released_quantity,
            version: self.previous_stock.version + 1,
        }
    }

    fn payload(self) -> Vec<u8> {
        let next_stock = self.next_stock();
        let mut payload = Vec::with_capacity(64);
        payload.extend_from_slice(b"test.Inventory.ReleaseStock");
        payload.push(0);
        payload.extend_from_slice(&self.previous_stock.product_id.to_le_bytes());
        payload.extend_from_slice(&self.released_quantity.to_le_bytes());
        payload.extend_from_slice(&next_stock.available_quantity.to_le_bytes());
        payload.extend_from_slice(&next_stock.version.to_le_bytes());
        payload
    }
}

impl ProcedureHandler for ReleaseStockFakeProcedureHandler {
    fn procedure_id(&self) -> ProcedureId {
        self.contract.procedure_id
    }

    fn contract(&self) -> ProcedureContractRef {
        self.contract
    }

    fn result_metadata(&self) -> ResultStreamMetadata {
        ResultStreamMetadata::exact(
            TEST_INVENTORY_RELEASE_STOCK_STREAM_ID,
            TEST_INVENTORY_RELEASE_STOCK_COLUMN_COUNT,
            Cardinality::One,
            1,
        )
    }

    fn execute(&self, _context: InvocationContext) -> AndromedaResult<LocalProcedure> {
        Ok(LocalProcedure {
            contract: self.contract,
            contract_binding: binding_for(self.contract),
            required_permissions: vec![TEST_INVENTORY_RELEASE_STOCK_PERMISSION.to_string()],
            result_metadata: self.result_metadata(),
            mutation_payload: self.payload(),
            rows_affected: 2,
        })
    }
}

pub(crate) fn test_inventory_query_stock_contract_ref() -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: TEST_INVENTORY_QUERY_STOCK_PROCEDURE_ID,
        contract_hash: ContractHash::test_vector(0x53),
        catalog_version: CatalogVersion::new(1),
    }
}

pub(crate) fn test_inventory_release_stock_contract_ref() -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: TEST_INVENTORY_RELEASE_STOCK_PROCEDURE_ID,
        contract_hash: ContractHash::test_vector(0x4c),
        catalog_version: CatalogVersion::new(1),
    }
}

pub(crate) fn contract(procedure_id: u64) -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: ProcedureId::new(procedure_id),
        contract_hash: ContractHash::test_vector(procedure_id as u8),
        catalog_version: CatalogVersion::new(3),
    }
}

pub(crate) fn binding_for(contract: ProcedureContractRef) -> ProcedureContractBinding {
    ProcedureContractBinding {
        procedure_id: contract.procedure_id,
        catalog_version: contract.catalog_version,
        contract_hash: contract.contract_hash,
        stats_version: StatsVersion::new(1),
        policy_version: PolicyVersion::new([contract.procedure_id.get() as u8; PolicyVersion::LEN]),
    }
}

pub(crate) fn handler(procedure_id: u64) -> FakeProcedureHandler {
    let contract = contract(procedure_id);
    FakeProcedureHandler {
        contract,
        result_contract: contract,
        rows_affected: 1,
        invalid_payload_shape: false,
    }
}

pub(crate) fn context() -> InvocationContext {
    InvocationContext::new(TraceId::new(9), vec!["inventory.reserve".to_string()])
}

pub(crate) fn reserve_context() -> InvocationContext {
    InvocationContext::new(
        TraceId::new(9),
        vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
    )
}

pub(crate) fn query_context() -> InvocationContext {
    InvocationContext::new(
        TraceId::new(9),
        vec![TEST_INVENTORY_QUERY_STOCK_PERMISSION.to_string()],
    )
}

pub(crate) fn release_context() -> InvocationContext {
    InvocationContext::new(
        TraceId::new(9),
        vec![TEST_INVENTORY_RELEASE_STOCK_PERMISSION.to_string()],
    )
}

pub(crate) fn decision(trace_id: TraceId, decision: CriticalDecisionKind) -> DecisionTrace {
    DecisionTrace {
        trace_id,
        decision,
        reason: format!("{decision:?} accepted before transaction creation"),
    }
}

pub(crate) fn pre_transaction_evidence(trace_id: TraceId) -> PreTransactionDispatchEvidence {
    PreTransactionDispatchEvidence {
        admission_trace: decision(trace_id, CriticalDecisionKind::ResourceGovernance),
        contract_trace: decision(trace_id, CriticalDecisionKind::ContractValidation),
        authorization_trace: Some(decision(
            trace_id,
            CriticalDecisionKind::SecurityAuthorization,
        )),
    }
}

pub(crate) fn dispatch_request(procedure: ProcedureContractRef) -> ProcedureDispatchRequest {
    let context = context();
    ProcedureDispatchRequest {
        invocation_id: InvocationId::new(700),
        procedure,
        procedure_binding: Some(binding_for(procedure)),
        pre_transaction: pre_transaction_evidence(context.trace_id),
        context,
    }
}

pub(crate) fn stock(product_id: i64, available_quantity: i64, version: u64) -> InventoryStock {
    InventoryStock {
        product_id,
        available_quantity,
        version,
    }
}

pub(crate) fn reserve_command(product_id: i64, quantity: i64) -> ReserveStockCommand {
    ReserveStockCommand {
        product_id,
        quantity,
    }
}

pub(crate) fn assert_inventory_result_shape(
    procedure: &LocalProcedure,
    expected_contract: ProcedureContractRef,
    expected_permission: &str,
    expected_stream_id: u64,
    expected_column_count: u32,
    expected_rows_affected: u64,
) {
    procedure.validate().unwrap();
    assert_eq!(procedure.contract, expected_contract);
    assert_eq!(
        procedure.required_permissions,
        vec![expected_permission.to_string()]
    );
    assert_eq!(procedure.result_metadata.stream_id, expected_stream_id);
    assert_eq!(
        procedure.result_metadata.column_count,
        expected_column_count
    );
    assert_eq!(procedure.result_metadata.row_count_exact, Some(1));
    assert_eq!(procedure.result_metadata.row_count_max, Some(1));
    assert_eq!(procedure.result_metadata.cardinality, Cardinality::One);
    assert_eq!(procedure.rows_affected, expected_rows_affected);
    assert!(!procedure.mutation_payload.is_empty());
}
