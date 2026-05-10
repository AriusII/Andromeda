#![forbid(unsafe_code)]

//! Narrow CLI adapter for the inventory vertical demo.

use andromeda_admission::{InvocationContext, InvocationRequest};
use andromeda_catalog::CatalogDefinitionBatchPlanning;
use andromeda_catalog_store::CatalogSnapshot;
use andromeda_definition_batch::{
    DefinitionBatchDependencyGraphHash, DefinitionBatchId, DefinitionBatchSourceHash,
};
use andromeda_error::AndromedaResult;
use andromeda_exec::LocalVerticalRuntime;
use andromeda_inventory_demo_core::{
    InventoryReserveStockExecutor as CoreInventoryReserveStockExecutor,
    InventoryStock as CoreInventoryStock, ReserveStockCommand as CoreReserveStockCommand,
    V0InventoryRecoverableRuntime, V0InventoryReserveStockRpcPayload,
    bind_inventory_reserve_stock_v0_pdf_executable_procedure,
    encode_inventory_reserve_stock_v0_execute_frame,
};
use andromeda_observability::TraceId;
use andromeda_result_stream::CompletionStatus;
use andromeda_types::{InvocationId, RequestId, SessionId};
use andromeda_wal::{FileWal, InMemoryWal, Lsn};
use std::path::{Path, PathBuf};

pub use andromeda_business_fixtures::inventory_catalog::{
    INVENTORY_DATABASE_ID, INVENTORY_NAMESPACE_ID, inventory_domain_definition_batch,
    inventory_reserve_stock_contract,
};
pub use andromeda_inventory_demo_core::{
    InventoryReserveStockExecutor, InventoryStock, ReserveStockCommand,
};

type CatalogPublicationReceipt = andromeda_catalog_store::CatalogPublicationReceipt<
    DefinitionBatchId,
    DefinitionBatchSourceHash,
    DefinitionBatchDependencyGraphHash,
>;

/// Printable trace details returned to the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliInventoryTraceReport {
    pub decision: String,
    pub reason: String,
}

/// Result of running the local inventory procedure demo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliInventoryDemoReport {
    pub procedure: String,
    pub status: CompletionStatus,
    pub rows_affected: Option<u64>,
    pub remaining_stock: i64,
    pub durable_wal_lsn: u64,
    pub durable_wal_records: usize,
    pub contract_trace: CliInventoryTraceReport,
    pub authorization_trace: Option<CliInventoryTraceReport>,
}

/// Result of running the recoverable inventory procedure demo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliInventoryRecoverableReport {
    pub procedure: String,
    pub status: CompletionStatus,
    pub rows_affected: Option<u64>,
    pub remaining_stock: i64,
    pub durable_wal_lsn: u64,
    pub wal_path: PathBuf,
    pub result_frame_count: usize,
}

/// Runs the local inventory demo and returns only the facts the CLI prints.
pub fn run_cli_inventory_demo() -> AndromedaResult<CliInventoryDemoReport> {
    let contract = inventory_reserve_stock_contract()?;
    let request = InvocationRequest {
        invocation_id: InvocationId::new(1),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
    let effect = CoreInventoryReserveStockExecutor::reserve(
        CoreReserveStockCommand {
            product_id: 42,
            quantity: 3,
        },
        CoreInventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 1,
        },
    )?;
    let procedure = effect.to_local_procedure(&contract)?;
    let mut runtime = LocalVerticalRuntime::new(InMemoryWal::new());
    let context = InvocationContext::new(TraceId::new(1), contract.required_permissions.clone());
    let outcome = runtime.execute_authorized(request, &procedure, &context)?;
    let durable_lsn = outcome
        .completion
        .durable_lsn()
        .unwrap_or_else(|| Lsn::new(0));

    debug_assert_eq!(outcome.completion.status(), CompletionStatus::Committed);

    Ok(CliInventoryDemoReport {
        procedure: contract.object.name.as_catalog_path(),
        status: outcome.completion.status(),
        rows_affected: outcome.completion.rows_affected(),
        remaining_stock: effect.result.remaining_quantity,
        durable_wal_lsn: durable_lsn.get(),
        durable_wal_records: runtime.wal().replay_durable().len(),
        contract_trace: CliInventoryTraceReport {
            decision: format!("{:?}", outcome.contract_trace.decision),
            reason: outcome.contract_trace.reason,
        },
        authorization_trace: outcome
            .authorization_trace
            .map(|trace| CliInventoryTraceReport {
                decision: format!("{:?}", trace.decision),
                reason: trace.reason,
            }),
    })
}

/// Runs the recoverable inventory demo and returns only the facts the CLI prints.
pub fn run_cli_inventory_recoverable(
    wal_path: &Path,
) -> AndromedaResult<CliInventoryRecoverableReport> {
    let contract = inventory_reserve_stock_contract()?;
    let catalog = inventory_catalog_snapshot()?;
    let procedure = bind_inventory_reserve_stock_v0_pdf_executable_procedure(&catalog, &contract)?;
    let request = InvocationRequest {
        invocation_id: InvocationId::new(2),
        procedure: contract.as_ref(),
        expected_binding: Some(contract.binding()),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
    std::fs::remove_file(wal_path).ok();
    let encoded_frame = encode_inventory_reserve_stock_v0_execute_frame(
        RequestId::new(2),
        SessionId::new(2),
        V0InventoryReserveStockRpcPayload::new(42, 3)?,
    )?;
    let mut runtime = V0InventoryRecoverableRuntime::new(FileWal::open(wal_path)?);
    let context = InvocationContext::new(TraceId::new(2), contract.required_permissions.clone());
    let outcome = runtime.execute_encoded_inventory_reserve_stock(
        &encoded_frame,
        &procedure,
        request,
        &context,
        CoreInventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 1,
        },
    )?;
    drop(runtime);

    debug_assert_eq!(
        outcome.vertical.completion.status(),
        CompletionStatus::Committed
    );

    Ok(CliInventoryRecoverableReport {
        procedure: contract.object.name.as_catalog_path(),
        status: outcome.vertical.completion.status(),
        rows_affected: outcome.vertical.completion.rows_affected(),
        remaining_stock: outcome.effect.result.remaining_quantity,
        durable_wal_lsn: outcome.durable_lsn().unwrap_or_else(|| Lsn::new(0)).get(),
        wal_path: wal_path.to_path_buf(),
        result_frame_count: outcome.result_frames.len(),
    })
}

fn inventory_catalog_snapshot() -> AndromedaResult<CatalogSnapshot<CatalogPublicationReceipt>> {
    let batch = inventory_domain_definition_batch()?;
    let plan = batch.dry_run()?;
    let mut snapshot = CatalogSnapshot::empty(
        INVENTORY_DATABASE_ID,
        INVENTORY_NAMESPACE_ID,
        batch.base_version,
    );
    snapshot.apply_mutation_plan(&plan.mutation_plan)?;
    Ok(snapshot)
}
