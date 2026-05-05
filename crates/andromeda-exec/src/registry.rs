use std::{collections::HashMap, sync::Arc};

use andromeda_catalog::{ProcedureContract, ProcedureContractRef};
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ProcedureId};

use crate::{
    InvocationContext, LocalProcedure, ProcedureDispatchRequest, ProcedureDispatcher,
    ReserveStockEffect, ResultStreamMetadata,
};

/// Executable Procedure adapter used by future registry-backed local dispatch.
///
/// Callers must invoke a handler only after admission, surface authorization,
/// permission checks, and catalog contract validation have accepted the
/// invocation. The handler does not own transaction allocation, WAL dispatch,
/// remote dispatch, or terminal completion mapping.
pub trait ProcedureHandler {
    /// Stable Procedure identifier used for request matching and registry keys.
    fn procedure_id(&self) -> ProcedureId;

    /// Contract metadata bound to the executable Procedure implementation.
    fn contract(&self) -> ProcedureContractRef;

    /// Result stream metadata that must be validated before payload emission.
    fn result_metadata(&self) -> ResultStreamMetadata;

    /// Execute an already-admitted invocation into the existing local runtime
    /// result shape.
    fn execute(&self, context: InvocationContext) -> AndromedaResult<LocalProcedure>;
}

/// Post-gate adapter for the V0 `Inventory.ReserveStock` local Procedure.
///
/// The adapter preserves the existing V0 business semantics by accepting a
/// [`ReserveStockEffect`] already produced by `InventoryReserveStockExecutor`
/// and converting it through the existing `ReserveStockEffect::to_local_procedure`
/// path. It does not perform admission, authorization, transaction allocation,
/// WAL dispatch, or production registry integration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReserveStockProcedureHandler {
    contract: ProcedureContractRef,
    local_procedure: LocalProcedure,
}

impl ReserveStockProcedureHandler {
    pub fn new(contract: &ProcedureContract, effect: &ReserveStockEffect) -> AndromedaResult<Self> {
        let local_procedure = effect.to_local_procedure(contract)?;
        local_procedure.validate()?;
        Ok(Self {
            contract: contract.as_ref(),
            local_procedure,
        })
    }
}

impl ProcedureHandler for ReserveStockProcedureHandler {
    fn procedure_id(&self) -> ProcedureId {
        self.contract.procedure_id
    }

    fn contract(&self) -> ProcedureContractRef {
        self.contract
    }

    fn result_metadata(&self) -> ResultStreamMetadata {
        self.local_procedure.result_metadata
    }

    fn execute(&self, _context: InvocationContext) -> AndromedaResult<LocalProcedure> {
        Ok(self.local_procedure.clone())
    }
}

/// Registry for local executable Procedure handlers.
///
/// The registry is keyed by [`ProcedureId`], not by raw names, SQL text, remote
/// addresses, or transport method names. Callers must reach this boundary only
/// after admission, surface authorization, permission checks, and catalog
/// contract validation have accepted the invocation.
#[derive(Clone, Default)]
pub struct ProcedureRegistry {
    handlers: HashMap<ProcedureId, Arc<dyn ProcedureHandler + Send + Sync>>,
}

impl ProcedureRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    pub fn register<H>(&mut self, handler: H) -> AndromedaResult<()>
    where
        H: ProcedureHandler + Send + Sync + 'static,
    {
        self.register_arc(Arc::new(handler))
    }

    pub fn register_arc(
        &mut self,
        handler: Arc<dyn ProcedureHandler + Send + Sync>,
    ) -> AndromedaResult<()> {
        let procedure_id = handler.procedure_id();

        if self.handlers.contains_key(&procedure_id) {
            return Err(duplicate_procedure_error(procedure_id));
        }

        validate_handler_registration(handler.as_ref())?;
        self.handlers.insert(procedure_id, handler);
        Ok(())
    }

    pub fn contains(&self, procedure_id: ProcedureId) -> bool {
        self.handlers.contains_key(&procedure_id)
    }

    pub fn lookup(
        &self,
        procedure_id: ProcedureId,
    ) -> Option<Arc<dyn ProcedureHandler + Send + Sync>> {
        self.handlers.get(&procedure_id).cloned()
    }

    /// Dispatch an already-admitted local Procedure invocation.
    ///
    /// This method accepts only a canonical [`ProcedureId`]. It does not perform
    /// remote dispatch, transaction allocation, WAL emission, or terminal
    /// completion mapping.
    pub fn dispatch(
        &self,
        procedure_id: ProcedureId,
        context: InvocationContext,
    ) -> AndromedaResult<LocalProcedure> {
        let handler = self
            .lookup(procedure_id)
            .ok_or_else(|| unknown_procedure_error(procedure_id))?;
        let procedure = handler.execute(context)?;
        validate_dispatch_result(procedure_id, handler.as_ref(), &procedure)?;
        Ok(procedure)
    }
}

impl ProcedureDispatcher for ProcedureRegistry {
    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<LocalProcedure> {
        request.validate()?;

        let procedure_id = request.procedure.procedure_id;
        let handler = self
            .lookup(procedure_id)
            .ok_or_else(|| unknown_procedure_error(procedure_id))?;

        if handler.contract() != request.procedure {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "dispatch request contract must match registered handler contract before execution",
            ));
        }

        let procedure = handler.execute(request.context)?;
        validate_dispatch_result(procedure_id, handler.as_ref(), &procedure)?;
        Ok(procedure)
    }
}

fn validate_handler_registration(
    handler: &(dyn ProcedureHandler + Send + Sync),
) -> AndromedaResult<()> {
    let procedure_id = handler.procedure_id();

    if procedure_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "registered Procedure id must not be zero",
        ));
    }

    let contract = handler.contract();
    contract.validate()?;

    if contract.procedure_id != procedure_id {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "registered Procedure handler id must match its contract Procedure id",
        ));
    }

    handler.result_metadata().validate_before_payload()?;
    Ok(())
}

fn validate_dispatch_result(
    procedure_id: ProcedureId,
    handler: &(dyn ProcedureHandler + Send + Sync),
    procedure: &LocalProcedure,
) -> AndromedaResult<()> {
    procedure.validate()?;

    if procedure.contract != handler.contract() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "dispatched Procedure contract must match registered handler contract",
        ));
    }

    if procedure.contract.procedure_id != procedure_id {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "dispatched Procedure id must match requested Procedure id",
        ));
    }

    Ok(())
}

fn duplicate_procedure_error(procedure_id: ProcedureId) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Contract,
        format!(
            "ProcedureRegistry already contains ProcedureId {}",
            procedure_id.get()
        ),
    )
}

fn unknown_procedure_error(procedure_id: ProcedureId) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Execution,
        format!(
            "ProcedureRegistry does not contain ProcedureId {}",
            procedure_id.get()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        InventoryReserveStockExecutor, InventoryStock, PreTransactionDispatchEvidence,
        RemoteProcedureDispatcherUnavailable, ReserveStockCommand,
    };
    use andromeda_catalog::{ProcedureContractRef, inventory_reserve_stock_contract};
    use andromeda_core::{CatalogVersion, ContractHash, ProcedureId};
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
    fn remote_procedure_dispatcher_placeholder_returns_typed_transport_error() {
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

        let procedure = registry.dispatch(contract.procedure_id, context()).unwrap();
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
            .dispatch(reserve_contract.procedure_id, context())
            .unwrap();
        reserve_procedure.validate().unwrap();
        assert_eq!(reserve_procedure.contract, reserve_contract.as_ref());
        assert_eq!(
            reserve_procedure.rows_affected,
            reserve_effect.rows_affected
        );

        let query_procedure = registry
            .dispatch(query_contract.procedure_id, context())
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
            .dispatch(release_contract.procedure_id, context())
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
}
