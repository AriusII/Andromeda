use andromeda_catalog::{ProcedureContract, ProcedureContractRef};
use andromeda_core::{AndromedaResult, ProcedureId};

use crate::{
    InvocationContext, LocalProcedure, QueryStockEffect, ReleaseStockEffect, ReserveStockEffect,
    ResultStreamMetadata,
};

use super::handler::ProcedureHandler;

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

/// Post-gate adapter for the V0 `Inventory.QueryStock` read-only Procedure.
///
/// Accepts a pre-computed [`QueryStockEffect`] and converts it through
/// `QueryStockEffect::to_local_procedure`. It does **not** perform admission,
/// authorization, transaction allocation, WAL dispatch, or production registry
/// integration.
///
/// The handler preserves the read-only guarantee: `mutation_payload` in the
/// resulting `LocalProcedure` is always empty and `rows_affected` is always 0.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryQueryStockProcedureHandler {
    contract: ProcedureContractRef,
    local_procedure: LocalProcedure,
}

impl InventoryQueryStockProcedureHandler {
    pub fn new(contract: &ProcedureContract, effect: &QueryStockEffect) -> AndromedaResult<Self> {
        let local_procedure = effect.to_local_procedure(contract)?;
        local_procedure.validate()?;
        Ok(Self {
            contract: contract.as_ref(),
            local_procedure,
        })
    }
}

impl ProcedureHandler for InventoryQueryStockProcedureHandler {
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

/// Post-gate adapter for the V0 `Inventory.ReleaseStock` write Procedure.
///
/// Accepts a pre-computed [`ReleaseStockEffect`] and converts it through
/// `ReleaseStockEffect::to_local_procedure`. It does **not** perform admission,
/// authorization, transaction allocation, WAL dispatch, or production registry
/// integration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryReleaseStockProcedureHandler {
    contract: ProcedureContractRef,
    local_procedure: LocalProcedure,
}

impl InventoryReleaseStockProcedureHandler {
    pub fn new(contract: &ProcedureContract, effect: &ReleaseStockEffect) -> AndromedaResult<Self> {
        let local_procedure = effect.to_local_procedure(contract)?;
        local_procedure.validate()?;
        Ok(Self {
            contract: contract.as_ref(),
            local_procedure,
        })
    }
}

impl ProcedureHandler for InventoryReleaseStockProcedureHandler {
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
