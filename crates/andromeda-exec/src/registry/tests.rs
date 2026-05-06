use super::*;
use crate::{
    InventoryReserveStockExecutor, InventoryStock, InvocationContext, LocalProcedure,
    PreTransactionDispatchEvidence, ProcedureDispatchRequest, ProcedureDispatcher,
    RemoteProcedureDispatcherUnavailable, ReserveStockCommand, ResultStreamMetadata,
};
use andromeda_catalog::{
    INVENTORY_RESERVE_STOCK_PERMISSION, ProcedureContractRef, inventory_reserve_stock_contract,
};
use andromeda_core::{
    AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, ProcedureId,
};
use andromeda_observe::{CriticalDecisionKind, DecisionTrace, TraceId};
use andromeda_srpl::Cardinality;

const TEST_INVENTORY_QUERY_STOCK_PROCEDURE_ID: ProcedureId = ProcedureId::new(0x5153);
const TEST_INVENTORY_QUERY_STOCK_PERMISSION: &str = "Inventory.QueryStock.Execute";
const TEST_INVENTORY_QUERY_STOCK_STREAM_ID: u64 = 2;
const TEST_INVENTORY_QUERY_STOCK_COLUMN_COUNT: u32 = 3;
const TEST_INVENTORY_RELEASE_STOCK_PROCEDURE_ID: ProcedureId = ProcedureId::new(0x524c);
const TEST_INVENTORY_RELEASE_STOCK_PERMISSION: &str = "Inventory.ReleaseStock.Execute";
const TEST_INVENTORY_RELEASE_STOCK_STREAM_ID: u64 = 3;
const TEST_INVENTORY_RELEASE_STOCK_COLUMN_COUNT: u32 = 4;

#[derive(Debug, Clone, Copy)]
struct FakeProcedureHandler {
    contract: ProcedureContractRef,
    result_contract: ProcedureContractRef,
    rows_affected: u64,
    invalid_payload_shape: bool,
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
struct QueryStockFakeProcedureHandler {
    contract: ProcedureContractRef,
    stock: InventoryStock,
}

impl QueryStockFakeProcedureHandler {
    fn new(stock: InventoryStock) -> Self {
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
            required_permissions: vec![TEST_INVENTORY_QUERY_STOCK_PERMISSION.to_string()],
            result_metadata: self.result_metadata(),
            mutation_payload: self.payload(),
            rows_affected: 0,
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct ReleaseStockFakeProcedureHandler {
    contract: ProcedureContractRef,
    previous_stock: InventoryStock,
    released_quantity: i64,
}

impl ReleaseStockFakeProcedureHandler {
    fn new(previous_stock: InventoryStock, released_quantity: i64) -> Self {
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
            required_permissions: vec![TEST_INVENTORY_RELEASE_STOCK_PERMISSION.to_string()],
            result_metadata: self.result_metadata(),
            mutation_payload: self.payload(),
            rows_affected: 2,
        })
    }
}

fn test_inventory_query_stock_contract_ref() -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: TEST_INVENTORY_QUERY_STOCK_PROCEDURE_ID,
        contract_hash: ContractHash::test_vector(0x53),
        catalog_version: CatalogVersion::new(1),
    }
}

fn test_inventory_release_stock_contract_ref() -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: TEST_INVENTORY_RELEASE_STOCK_PROCEDURE_ID,
        contract_hash: ContractHash::test_vector(0x4c),
        catalog_version: CatalogVersion::new(1),
    }
}

fn contract(procedure_id: u64) -> ProcedureContractRef {
    ProcedureContractRef {
        procedure_id: ProcedureId::new(procedure_id),
        contract_hash: ContractHash::test_vector(procedure_id as u8),
        catalog_version: CatalogVersion::new(3),
    }
}

fn handler(procedure_id: u64) -> FakeProcedureHandler {
    let contract = contract(procedure_id);
    FakeProcedureHandler {
        contract,
        result_contract: contract,
        rows_affected: 1,
        invalid_payload_shape: false,
    }
}

fn context() -> InvocationContext {
    InvocationContext::new(TraceId::new(9), vec!["inventory.reserve".to_string()])
}

fn reserve_context() -> InvocationContext {
    InvocationContext::new(
        TraceId::new(9),
        vec![INVENTORY_RESERVE_STOCK_PERMISSION.to_string()],
    )
}

fn query_context() -> InvocationContext {
    InvocationContext::new(
        TraceId::new(9),
        vec![TEST_INVENTORY_QUERY_STOCK_PERMISSION.to_string()],
    )
}

fn release_context() -> InvocationContext {
    InvocationContext::new(
        TraceId::new(9),
        vec![TEST_INVENTORY_RELEASE_STOCK_PERMISSION.to_string()],
    )
}

fn decision(trace_id: TraceId, decision: CriticalDecisionKind) -> DecisionTrace {
    DecisionTrace {
        trace_id,
        decision,
        reason: format!("{decision:?} accepted before transaction creation"),
    }
}

fn pre_transaction_evidence(trace_id: TraceId) -> PreTransactionDispatchEvidence {
    PreTransactionDispatchEvidence {
        admission_trace: decision(trace_id, CriticalDecisionKind::ResourceGovernance),
        contract_trace: decision(trace_id, CriticalDecisionKind::ContractValidation),
        authorization_trace: Some(decision(
            trace_id,
            CriticalDecisionKind::SecurityAuthorization,
        )),
    }
}

fn dispatch_request(procedure: ProcedureContractRef) -> ProcedureDispatchRequest {
    let context = context();
    ProcedureDispatchRequest {
        procedure,
        pre_transaction: pre_transaction_evidence(context.trace_id),
        context,
    }
}

#[test]
fn procedure_handler_exposes_metadata_and_existing_local_result_shape() {
    let contract = contract(42);
    let handler = FakeProcedureHandler {
        contract,
        result_contract: contract,
        rows_affected: 1,
        invalid_payload_shape: false,
    };

    assert_eq!(handler.procedure_id(), ProcedureId::new(42));
    assert_eq!(handler.contract(), contract);
    assert_eq!(handler.result_metadata().row_count_exact, Some(1));

    let procedure = handler
        .execute(context())
        .expect("fake handler should produce a local procedure");

    procedure
        .validate()
        .expect("fake handler result should satisfy existing LocalProcedure shape");

    assert_eq!(procedure.contract, contract);
    assert_eq!(procedure.rows_affected, 1);
}

#[test]
fn registry_registers_and_dispatches_handler() {
    let mut registry = ProcedureRegistry::new();
    registry.register(handler(42)).unwrap();

    assert_eq!(registry.len(), 1);
    assert!(registry.contains(ProcedureId::new(42)));

    let procedure = registry.dispatch(ProcedureId::new(42), context()).unwrap();
    assert_eq!(procedure.contract.procedure_id, ProcedureId::new(42));
    assert_eq!(procedure.rows_affected, 1);
}

#[test]
fn procedure_dispatcher_invokes_registry_handler_after_pre_transaction_evidence() {
    let mut registry = ProcedureRegistry::new();
    let handler = handler(42);
    let request = dispatch_request(handler.contract());
    registry.register(handler).unwrap();

    let procedure = ProcedureDispatcher::dispatch_procedure(&registry, request).unwrap();

    assert_eq!(procedure.contract.procedure_id, ProcedureId::new(42));
    assert_eq!(procedure.rows_affected, 1);
}

#[test]
fn procedure_dispatcher_rejects_invalid_evidence_before_registry_lookup() {
    let registry = ProcedureRegistry::new();
    let mut request = dispatch_request(contract(42));
    request.pre_transaction.contract_trace.decision = CriticalDecisionKind::WalAppend;

    let error = ProcedureDispatcher::dispatch_procedure(&registry, request).unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
}

#[test]
fn procedure_dispatcher_rejects_contract_mismatch_before_handler_execution() {
    let mut registry = ProcedureRegistry::new();
    registry.register(handler(42)).unwrap();
    let mut mismatched_contract = contract(42);
    mismatched_contract.contract_hash = ContractHash::test_vector(99);
    let request = dispatch_request(mismatched_contract);

    let error = ProcedureDispatcher::dispatch_procedure(&registry, request).unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
}

#[test]
fn remote_procedure_dispatcher_unavailable_returns_typed_transport_error() {
    let dispatcher = RemoteProcedureDispatcherUnavailable::unsupported();
    let request = dispatch_request(contract(42));

    let error = dispatcher.dispatch_procedure(request).unwrap_err();

    assert_eq!(error.kind(), AndromedaErrorKind::Transport);
}

#[test]
fn registry_rejects_duplicate_procedure_id() {
    let mut registry = ProcedureRegistry::new();
    registry.register(handler(42)).unwrap();

    let duplicate = registry.register(handler(42)).unwrap_err();
    assert_eq!(duplicate.kind(), AndromedaErrorKind::Contract);
}

#[test]
fn registry_returns_explicit_unknown_procedure_error() {
    let registry = ProcedureRegistry::new();

    let error = registry
        .dispatch(ProcedureId::new(404), context())
        .unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Execution);
}

#[test]
fn registry_rejects_handler_contract_mismatch() {
    let mut registry = ProcedureRegistry::new();
    let mut handler = handler(42);
    handler.contract = ProcedureContractRef {
        procedure_id: ProcedureId::new(42),
        contract_hash: ContractHash::test_vector(7),
        catalog_version: CatalogVersion::new(0),
    };

    let error = registry.register(handler).unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
}

#[test]
fn registry_rejects_dispatch_result_contract_mismatch() {
    let mut registry = ProcedureRegistry::new();
    let mut handler = handler(42);
    handler.result_contract = contract(43);
    registry.register(handler).unwrap();

    let error = registry
        .dispatch(ProcedureId::new(42), context())
        .unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Contract);
}

#[test]
fn registry_rejects_invalid_dispatch_result_shape() {
    let mut registry = ProcedureRegistry::new();
    let mut handler = handler(42);
    handler.invalid_payload_shape = true;
    registry.register(handler).unwrap();

    let error = registry
        .dispatch(ProcedureId::new(42), context())
        .unwrap_err();
    assert_eq!(error.kind(), AndromedaErrorKind::Execution);
}

#[test]
fn reserve_stock_handler_exposes_existing_contract_and_local_shape() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let effect = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 3,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 7,
        },
    )
    .unwrap();
    let handler = ReserveStockProcedureHandler::new(&contract, &effect).unwrap();

    assert_eq!(handler.procedure_id(), contract.procedure_id);
    assert_eq!(handler.contract(), contract.as_ref());
    assert_eq!(handler.result_metadata().row_count_exact, Some(1));

    let procedure = handler.execute(context()).unwrap();
    procedure.validate().unwrap();
    assert_eq!(procedure.contract, contract.as_ref());
    assert_eq!(procedure.rows_affected, effect.rows_affected);
}

#[test]
fn reserve_stock_handler_registers_and_dispatches_without_runtime_integration() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let effect = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 1,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 5,
            version: 1,
        },
    )
    .unwrap();
    let handler = ReserveStockProcedureHandler::new(&contract, &effect).unwrap();
    let mut registry = ProcedureRegistry::new();
    registry.register(handler).unwrap();

    let procedure = registry
        .dispatch(contract.procedure_id, reserve_context())
        .unwrap();
    assert_eq!(procedure.contract, contract.as_ref());
    assert_eq!(procedure.rows_affected, effect.rows_affected);
}

#[test]
fn registry_registers_and_dispatches_reserve_stock_query_stock_and_release_stock() {
    let reserve_contract = inventory_reserve_stock_contract().unwrap();
    let reserve_effect = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 1,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 5,
            version: 1,
        },
    )
    .unwrap();
    let reserve_handler =
        ReserveStockProcedureHandler::new(&reserve_contract, &reserve_effect).unwrap();
    let query_handler = QueryStockFakeProcedureHandler::new(InventoryStock {
        product_id: 42,
        available_quantity: 4,
        version: 2,
    });
    let query_contract = query_handler.contract();
    let release_handler = ReleaseStockFakeProcedureHandler::new(
        InventoryStock {
            product_id: 42,
            available_quantity: 4,
            version: 2,
        },
        1,
    );
    let release_contract = release_handler.contract();

    let mut registry = ProcedureRegistry::new();
    registry.register(reserve_handler).unwrap();
    registry.register(query_handler).unwrap();
    registry.register(release_handler).unwrap();

    assert_eq!(registry.len(), 3);
    assert!(registry.contains(reserve_contract.procedure_id));
    assert!(registry.contains(query_contract.procedure_id));
    assert!(registry.contains(release_contract.procedure_id));

    let reserve_procedure = registry
        .dispatch(reserve_contract.procedure_id, reserve_context())
        .unwrap();
    reserve_procedure.validate().unwrap();
    assert_eq!(reserve_procedure.contract, reserve_contract.as_ref());
    assert_eq!(
        reserve_procedure.rows_affected,
        reserve_effect.rows_affected
    );

    let query_procedure = registry
        .dispatch(query_contract.procedure_id, query_context())
        .unwrap();
    query_procedure.validate().unwrap();

    assert_eq!(query_procedure.contract, query_contract);
    assert_eq!(
        query_procedure.required_permissions,
        vec![TEST_INVENTORY_QUERY_STOCK_PERMISSION.to_string()]
    );
    assert_eq!(
        query_procedure.result_metadata.stream_id,
        TEST_INVENTORY_QUERY_STOCK_STREAM_ID
    );
    assert_eq!(
        query_procedure.result_metadata.column_count,
        TEST_INVENTORY_QUERY_STOCK_COLUMN_COUNT
    );
    assert_eq!(query_procedure.result_metadata.row_count_exact, Some(1));
    assert_eq!(query_procedure.result_metadata.row_count_max, Some(1));
    assert_eq!(
        query_procedure.result_metadata.cardinality,
        Cardinality::One
    );
    assert_eq!(query_procedure.rows_affected, 0);
    assert!(!query_procedure.mutation_payload.is_empty());

    let release_procedure = registry
        .dispatch(release_contract.procedure_id, release_context())
        .unwrap();
    release_procedure.validate().unwrap();

    assert_eq!(release_procedure.contract, release_contract);
    assert_eq!(
        release_procedure.required_permissions,
        vec![TEST_INVENTORY_RELEASE_STOCK_PERMISSION.to_string()]
    );
    assert_eq!(
        release_procedure.result_metadata.stream_id,
        TEST_INVENTORY_RELEASE_STOCK_STREAM_ID
    );
    assert_eq!(
        release_procedure.result_metadata.column_count,
        TEST_INVENTORY_RELEASE_STOCK_COLUMN_COUNT
    );
    assert_eq!(release_procedure.result_metadata.row_count_exact, Some(1));
    assert_eq!(release_procedure.result_metadata.row_count_max, Some(1));
    assert_eq!(
        release_procedure.result_metadata.cardinality,
        Cardinality::One
    );
    assert_eq!(release_procedure.rows_affected, 2);
    assert!(!release_procedure.mutation_payload.is_empty());
}
