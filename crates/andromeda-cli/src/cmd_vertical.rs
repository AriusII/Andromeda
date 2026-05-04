//! Vertical demo commands.

use andromeda_catalog::inventory_reserve_stock_contract;
use andromeda_core::{AndromedaResult, HardwareProfile, InvocationId, RequestId, SessionId};
use andromeda_exec::{
    encode_inventory_reserve_stock_v0_execute_frame, inventory_reserve_stock_v0_pdf_srpl_source,
    CompletionStatus, InventoryReserveStockExecutor, InventoryStock, InvocationContext,
    InvocationRequest, LocalVerticalRuntime, ReserveStockCommand, V0InventoryRecoverableRuntime,
    V0InventoryReserveStockRpcPayload,
};
use andromeda_observe::TraceId;
use andromeda_storage::{FileWal, InMemoryWal, Lsn};
use std::path::PathBuf;

/// Runs the Phase 1 vertical demo.
pub fn run_vertical_demo() -> AndromedaResult<()> {
    let contract = inventory_reserve_stock_contract()?;
    let request = InvocationRequest {
        invocation_id: InvocationId::new(1),
        procedure: contract.as_ref(),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
    let effect = InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 3,
        },
        InventoryStock {
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
        .durable_lsn
        .unwrap_or_else(|| Lsn::new(0));

    println!("Andromeda Phase 1 local vertical prototype");
    println!("procedure: {}", contract.object.name.as_catalog_path());
    println!("status: {:?}", outcome.completion.status);
    println!("rows affected: {:?}", outcome.completion.rows_affected);
    println!("remaining stock: {}", effect.result.remaining_quantity);
    println!("durable WAL LSN: {}", durable_lsn.get());
    println!(
        "durable WAL records: {}",
        runtime.wal().replay_durable().len()
    );
    println!(
        "contract trace: {:?} ({})",
        outcome.contract_trace.decision, outcome.contract_trace.reason
    );
    if let Some(trace) = &outcome.authorization_trace {
        println!(
            "authorization trace: {:?} ({})",
            trace.decision, trace.reason
        );
    }

    debug_assert_eq!(outcome.completion.status, CompletionStatus::Committed);
    Ok(())
}

/// Runs the recoverable V0 vertical demo.
pub fn run_vertical_v0_demo(wal_path: PathBuf) -> AndromedaResult<()> {
    let contract = inventory_reserve_stock_contract()?;
    let catalog = crate::proto_helpers::inventory_catalog_snapshot()?;
    let request = InvocationRequest {
        invocation_id: InvocationId::new(2),
        procedure: contract.as_ref(),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
    std::fs::remove_file(&wal_path).ok();
    let encoded_frame = encode_inventory_reserve_stock_v0_execute_frame(
        RequestId::new(2),
        SessionId::new(2),
        V0InventoryReserveStockRpcPayload::new(42, 3)?,
    )?;
    let mut runtime = V0InventoryRecoverableRuntime::new(FileWal::open(&wal_path)?);
    let context = InvocationContext::new(TraceId::new(2), contract.required_permissions.clone());
    let outcome = runtime.execute_encoded_inventory_reserve_stock(
        &encoded_frame,
        inventory_reserve_stock_v0_pdf_srpl_source(),
        &catalog,
        &contract,
        request,
        &context,
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 1,
        },
    )?;
    drop(runtime);

    let report = crate::cmd_recovery::recovery_report_for_path(&wal_path, Lsn::new(1))?;
    let replay_lsns = report
        .replay_lsns()
        .map(|lsn| lsn.get())
        .collect::<Vec<_>>();

    println!("Andromeda V0 recoverable vertical prototype");
    println!("procedure: {}", contract.object.name.as_catalog_path());
    println!("status: {:?}", outcome.vertical.completion.status);
    println!(
        "rows affected: {:?}",
        outcome.vertical.completion.rows_affected
    );
    println!(
        "remaining stock: {}",
        outcome.effect.result.remaining_quantity
    );
    println!(
        "durable WAL LSN: {}",
        outcome.durable_lsn().unwrap_or_else(|| Lsn::new(0)).get()
    );
    println!("WAL path: {}", wal_path.display());
    println!("recovery replay LSNs: {:?}", replay_lsns);
    println!("recovery boundary: {:?}", report.boundary_kind);
    println!("forensic required: {}", report.forensic_required);
    println!("result frames: {}", outcome.result_frames.len());

    debug_assert_eq!(
        outcome.vertical.completion.status,
        CompletionStatus::Committed
    );
    Ok(())
}

/// Prints the help message.
pub fn print_help() {
    let profile = HardwareProfile::conservative();

    const WORKSPACE_CRATES: &[&str] = &[
        "andromeda-core",
        "andromeda-proto",
        "andromeda-quic",
        "andromeda-catalog",
        "andromeda-srpl",
        "andromeda-tx",
        "andromeda-storage",
        "andromeda-exec",
        "andromeda-observe",
    ];

    println!("Andromeda Rust workspace 0.1.0");
    println!("database engine status: foundation only");
    println!("hardware profile: {:?}", profile.architecture);
    println!("crates:");

    for crate_name in WORKSPACE_CRATES {
        println!("  - {crate_name}");
    }

    println!("run `andromeda-cli vertical` for the local Phase 1 prototype");
    println!(
        "run `andromeda-cli vertical-v0 [--wal <path>]` for the recoverable V0 vertical prototype"
    );
    println!("run `andromeda-cli recovery-inspect <wal-path>` for a V0 FileWal recovery report");
    println!(
        "run `andromeda-cli protocol-smoke [--detail]` for local protocol contract inspection"
    );
}
