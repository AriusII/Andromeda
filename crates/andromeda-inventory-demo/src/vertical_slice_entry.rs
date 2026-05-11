mod events;
mod protocol;
mod result_frames;

use andromeda_admission::{InvocationContext, InvocationRequest};
use andromeda_catalog_store::CatalogSnapshot;
use andromeda_definition_batch::{
    DefinitionBatchDependencyGraphHash, DefinitionBatchId, DefinitionBatchSourceHash,
};
use andromeda_digest::sha256;
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_exec::{LocalVerticalRuntime, VerticalInvocationOutcome};
use andromeda_observe::{EventEmitter, EventSink};
use andromeda_procedure_contract::{ProcedureContract, ProcedureContractBinding};
use andromeda_rpc_protocol::FrameBytes;
use andromeda_srpl_catalog_binding::bind_executable_procedure_plan;
use andromeda_srpl_definition_batch::compile_narrow_procedure_signature;
use andromeda_srpl_ir::ExecutableProcedurePlan;
use andromeda_storage_page::{PageId, PageSize};
use andromeda_wal::{InvocationWal, Lsn};

use crate::{
    InventoryProductStockCommitEvidence, InventoryProductStockDurableRedoEvidence,
    InventoryProductStockStore, InventoryStock, ReserveStockEffect,
    business::HeapInventoryProductStockStore,
};

use events::{emit_v0_outcome_events, v0_pre_transaction_reject_from_error};
use result_frames::encode_v0_result_frames;

pub use events::emit_v0_inventory_reserve_stock_pre_transaction_refusal;
pub use protocol::{
    V0InventoryProtocolViolation, V0InventoryReserveStockRpcPayload, decode_v0_execute_frame,
    encode_inventory_reserve_stock_v0_execute_frame,
};

type CatalogPublicationReceipt = andromeda_catalog_store::CatalogPublicationReceipt<
    DefinitionBatchId,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
>;

const V0_PRODUCT_STOCK_HEAP_PAGE_ID: PageId = PageId::new(42_000);
const V0_PRODUCT_STOCK_HEAP_PAGE_SIZE: PageSize = PageSize::KiB16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V0InventoryRecoverableOutcome {
    pub srpl_plan: ExecutableProcedurePlan,
    pub effect: ReserveStockEffect,
    pub product_stock_commit: InventoryProductStockCommitEvidence,
    pub product_stock_redo: InventoryProductStockDurableRedoEvidence,
    pub vertical: VerticalInvocationOutcome,
    pub command_frame: FrameBytes,
    pub result_frames: Vec<FrameBytes>,
}

impl V0InventoryRecoverableOutcome {
    pub fn durable_lsn(&self) -> Option<Lsn> {
        self.vertical.completion.durable_lsn
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V0InventoryReserveStockExecutableProcedure {
    pub contract: ProcedureContract,
    pub binding: ProcedureContractBinding,
    pub srpl_source_digest: [u8; 32],
    pub srpl_plan: ExecutableProcedurePlan,
}

impl V0InventoryReserveStockExecutableProcedure {
    pub fn bind_from_srpl_source(
        srpl_source: &str,
        catalog: &CatalogSnapshot<CatalogPublicationReceipt>,
        contract: &ProcedureContract,
    ) -> AndromedaResult<Self> {
        let srpl_ir = compile_narrow_procedure_signature(srpl_source).map_err(|diagnostic| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!(
                    "V0 Inventory.ReserveStock SRPL compilation failed during {:?}: {}",
                    diagnostic.phase, diagnostic.message
                ),
            )
        })?;
        let srpl_plan = bind_executable_procedure_plan(&srpl_ir, catalog)?;
        let executable = Self {
            contract: contract.clone(),
            binding: contract.validated_binding()?,
            srpl_source_digest: sha256(srpl_source.as_bytes()),
            srpl_plan,
        };
        executable.validate()?;
        Ok(executable)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.contract.validate_binding(&self.binding)?;
        self.srpl_plan.validate()?;

        if self.srpl_source_digest == [0; 32] {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "V0 executable procedure requires non-zero SRPL source digest evidence",
            ));
        }

        if self.srpl_plan.evidence.procedure_contract != self.contract.as_ref() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "V0 executable procedure plan contract evidence does not match its contract",
            ));
        }

        if self.srpl_plan.evidence.catalog_version != self.binding.catalog_version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "V0 executable procedure plan catalog version does not match binding evidence",
            ));
        }

        if self.srpl_plan.evidence.procedure_object != self.contract.object {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "V0 executable procedure plan object evidence does not match its contract",
            ));
        }

        Ok(())
    }

    pub fn contract(&self) -> &ProcedureContract {
        &self.contract
    }
}

pub struct V0InventoryRecoverableRuntime<W> {
    local: LocalVerticalRuntime<W>,
}

impl<W> V0InventoryRecoverableRuntime<W>
where
    W: InvocationWal,
{
    pub fn new(wal: W) -> Self {
        Self {
            local: LocalVerticalRuntime::new(wal),
        }
    }

    /// Wrap an externally constructed [`LocalVerticalRuntime`] so callers can
    /// inject a recovery-seeded local runtime before the V0 path begins
    /// serving traffic.
    pub fn from_local_runtime(local: LocalVerticalRuntime<W>) -> Self {
        Self { local }
    }

    pub fn wal(&self) -> &W {
        self.local.wal()
    }

    pub fn wal_mut(&mut self) -> &mut W {
        self.local.wal_mut()
    }

    pub fn into_inner(self) -> LocalVerticalRuntime<W> {
        self.local
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "V0 vertical slice keeps each contractual input explicit at the RPC boundary."
    )]
    pub fn execute_encoded_inventory_reserve_stock(
        &mut self,
        encoded_execute_frame: &[u8],
        procedure: &V0InventoryReserveStockExecutableProcedure,
        request: InvocationRequest,
        context: &InvocationContext,
        observed_stock: InventoryStock,
    ) -> AndromedaResult<V0InventoryRecoverableOutcome> {
        let mut product_stock = v0_product_stock_store_from_cold_snapshot(observed_stock)?;
        self.execute_encoded_inventory_reserve_stock_with_product_stock(
            encoded_execute_frame,
            procedure,
            request,
            context,
            &mut product_stock,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "V0 vertical slice keeps each contractual input explicit at the RPC boundary."
    )]
    pub fn execute_encoded_inventory_reserve_stock_with_product_stock<P>(
        &mut self,
        encoded_execute_frame: &[u8],
        procedure: &V0InventoryReserveStockExecutableProcedure,
        request: InvocationRequest,
        context: &InvocationContext,
        product_stock: &mut P,
    ) -> AndromedaResult<V0InventoryRecoverableOutcome>
    where
        P: InventoryProductStockStore,
    {
        procedure.validate()?;
        let contract = procedure.contract();
        let command_frame = decode_v0_execute_frame(encoded_execute_frame)?;
        let command =
            V0InventoryReserveStockRpcPayload::decode(&command_frame.payload)?.to_command();

        validate_v0_reserve_stock_before_product_stock_prepare(&request, context, contract)?;

        let prepared = product_stock.prepare_reserve_stock(command)?;
        let product_stock_redo_template =
            product_stock.prepared_reserve_stock_redo_template(&prepared)?;
        let effect = prepared.effect.clone();
        let mut local_procedure = effect.to_local_procedure(contract)?;
        local_procedure.mutation_payload = product_stock_redo_template.encode_template()?;
        let vertical = match self
            .local
            .execute_internal_authorized(request, &local_procedure, context)
        {
            Ok(vertical) => vertical,
            Err(error) => {
                product_stock.abort_prepared_reserve_stock(&prepared, error.message())?;
                return Err(error);
            },
        };
        let durable_lsn = vertical.completion.durable_lsn.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "V0 ProductStock publication requires durable commit LSN evidence",
            )
        })?;
        let product_stock_commit =
            InventoryProductStockCommitEvidence::new(vertical.transaction_id, durable_lsn)?;
        let wal_evidence = vertical.wal_evidence.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                "V0 ProductStock publication requires local WAL evidence for storage redo",
            )
        })?;
        let redo_record_lsn = wal_evidence.mutation_lsn.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Storage,
                "V0 ProductStock publication requires a durable row redo WAL record LSN",
            )
        })?;
        let redo_payload =
            product_stock_redo_template.materialize_heap_redo_payload(redo_record_lsn)?;
        let product_stock_redo = InventoryProductStockDurableRedoEvidence::new(
            vertical.transaction_id,
            durable_lsn,
            redo_payload,
        )?;
        product_stock.publish_committed_reserve_stock_with_redo(
            &prepared,
            product_stock_commit,
            product_stock_redo.clone(),
        )?;
        let tx_id = vertical
            .completion
            .transaction_state
            .map(|_| vertical.transaction_id);
        let result_frames = encode_v0_result_frames(
            command_frame.header.request_id,
            command_frame.header.session_id,
            tx_id,
            &effect,
            &vertical,
        )?;

        Ok(V0InventoryRecoverableOutcome {
            srpl_plan: procedure.srpl_plan.clone(),
            effect,
            product_stock_commit,
            product_stock_redo,
            vertical,
            command_frame,
            result_frames,
        })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Observed V0 path keeps transport, catalog, contract, request, and stock evidence separate."
    )]
    pub fn execute_encoded_inventory_reserve_stock_observed(
        &mut self,
        encoded_execute_frame: &[u8],
        procedure: &V0InventoryReserveStockExecutableProcedure,
        request: InvocationRequest,
        context: &InvocationContext,
        observed_stock: InventoryStock,
        sink: &mut impl EventSink,
    ) -> AndromedaResult<V0InventoryRecoverableOutcome> {
        let mut emitter = EventEmitter::new(sink);
        self.execute_encoded_inventory_reserve_stock_observed_with_emitter(
            encoded_execute_frame,
            procedure,
            request,
            context,
            observed_stock,
            &mut emitter,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Observed V0 path keeps transport, catalog, contract, request, stock adapter, and sink evidence separate."
    )]
    pub fn execute_encoded_inventory_reserve_stock_observed_with_product_stock<P>(
        &mut self,
        encoded_execute_frame: &[u8],
        procedure: &V0InventoryReserveStockExecutableProcedure,
        request: InvocationRequest,
        context: &InvocationContext,
        product_stock: &mut P,
        sink: &mut impl EventSink,
    ) -> AndromedaResult<V0InventoryRecoverableOutcome>
    where
        P: InventoryProductStockStore,
    {
        let mut emitter = EventEmitter::new(sink);
        self.execute_encoded_inventory_reserve_stock_observed_with_product_stock_and_emitter(
            encoded_execute_frame,
            procedure,
            request,
            context,
            product_stock,
            &mut emitter,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Observed V0 path keeps transport, catalog, contract, request, and emitter evidence separate."
    )]
    pub fn execute_encoded_inventory_reserve_stock_observed_with_emitter<S: EventSink>(
        &mut self,
        encoded_execute_frame: &[u8],
        procedure: &V0InventoryReserveStockExecutableProcedure,
        request: InvocationRequest,
        context: &InvocationContext,
        observed_stock: InventoryStock,
        emitter: &mut EventEmitter<S>,
    ) -> AndromedaResult<V0InventoryRecoverableOutcome> {
        let mut product_stock = v0_product_stock_store_from_cold_snapshot(observed_stock)?;
        self.execute_encoded_inventory_reserve_stock_observed_with_product_stock_and_emitter(
            encoded_execute_frame,
            procedure,
            request,
            context,
            &mut product_stock,
            emitter,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Observed V0 path keeps transport, catalog, contract, request, stock adapter, and emitter evidence separate."
    )]
    pub fn execute_encoded_inventory_reserve_stock_observed_with_product_stock_and_emitter<
        P,
        S: EventSink,
    >(
        &mut self,
        encoded_execute_frame: &[u8],
        procedure: &V0InventoryReserveStockExecutableProcedure,
        request: InvocationRequest,
        context: &InvocationContext,
        product_stock: &mut P,
        emitter: &mut EventEmitter<S>,
    ) -> AndromedaResult<V0InventoryRecoverableOutcome>
    where
        P: InventoryProductStockStore,
    {
        procedure.validate()?;
        let contract = procedure.contract();
        let rejection_request = request.clone();
        let rejection_context = context.clone();
        let outcome = match self.execute_encoded_inventory_reserve_stock_with_product_stock(
            encoded_execute_frame,
            procedure,
            request,
            context,
            product_stock,
        ) {
            Ok(outcome) => outcome,
            Err(error) => {
                if let Some(reject) = v0_pre_transaction_reject_from_error(&error) {
                    let command_frame = decode_v0_execute_frame(encoded_execute_frame)?;
                    emit_v0_inventory_reserve_stock_pre_transaction_refusal(
                        &rejection_request,
                        &rejection_context,
                        command_frame.header.request_id,
                        command_frame.header.session_id,
                        contract,
                        &reject,
                        emitter,
                    )?;
                }
                return Err(error);
            },
        };

        emit_v0_outcome_events(&outcome, emitter)?;
        Ok(outcome)
    }
}

fn v0_product_stock_store_from_cold_snapshot(
    observed_stock: InventoryStock,
) -> AndromedaResult<HeapInventoryProductStockStore> {
    HeapInventoryProductStockStore::from_cold_snapshot(
        V0_PRODUCT_STOCK_HEAP_PAGE_ID,
        V0_PRODUCT_STOCK_HEAP_PAGE_SIZE,
        observed_stock,
    )
}

pub fn inventory_reserve_stock_v0_pdf_srpl_source() -> &'static str {
    "procedure Inventory.ReserveStock accepts (ProductId i64, Quantity i64) returns Reservation one (Reserved bool) begin ensure Inventory.ProductStock Stock where ProductId = Stock.ProductId and Stock.AvailableQuantity >= Quantity else fail InsufficientStock; update Inventory.ProductStock set AvailableQuantity = Stock.AvailableQuantity - Quantity where ProductId = Stock.ProductId affected rows 1; return Reservation (Reserved); end;"
}

pub fn bind_inventory_reserve_stock_v0_pdf_executable_procedure(
    catalog: &CatalogSnapshot<CatalogPublicationReceipt>,
    contract: &ProcedureContract,
) -> AndromedaResult<V0InventoryReserveStockExecutableProcedure> {
    V0InventoryReserveStockExecutableProcedure::bind_from_srpl_source(
        inventory_reserve_stock_v0_pdf_srpl_source(),
        catalog,
        contract,
    )
}

fn validate_v0_reserve_stock_before_product_stock_prepare(
    request: &InvocationRequest,
    context: &InvocationContext,
    contract: &ProcedureContract,
) -> AndromedaResult<()> {
    request
        .validate_admission(context.trace_id)
        .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;
    request
        .validate_before_transaction(contract.binding(), context.trace_id)
        .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;
    context
        .authorize(&contract.required_permissions)
        .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Security, reject.reason))?;
    if !request.structured_parameters.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "V0 Inventory.ReserveStock fixed RPC payload path rejects separate StructuredObject parameters before ProductStock preparation",
        ));
    }
    Ok(())
}
