#![forbid(unsafe_code)]

use andromeda_catalog::inventory_reserve_stock_contract;
use andromeda_core::{AndromedaResult, HardwareProfile, InvocationId};
use andromeda_exec::{
    CompletionStatus, InvocationContext, InvocationRequest, LocalProcedure, LocalVerticalRuntime,
    ResultStreamMetadata,
};
use andromeda_observe::TraceId;
use andromeda_srpl::Cardinality;
use andromeda_storage::{InMemoryWal, Lsn};

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

fn main() -> AndromedaResult<()> {
    let args = std::env::args().collect::<Vec<String>>();
    if args.get(1).is_some_and(|arg| arg == "vertical") {
        return run_vertical_demo();
    }

    let profile = HardwareProfile::conservative();

    println!("Andromeda Rust workspace 0.1.0");
    println!("database engine status: foundation only");
    println!("hardware profile: {:?}", profile.architecture);
    println!("crates:");

    for crate_name in WORKSPACE_CRATES {
        println!("  - {crate_name}");
    }

    println!("run `andromeda-cli vertical` for the local Phase 1 prototype");
    Ok(())
}

fn run_vertical_demo() -> AndromedaResult<()> {
    let contract = inventory_reserve_stock_contract()?;
    let request = InvocationRequest {
        invocation_id: InvocationId::new(1),
        procedure: contract.as_ref(),
        expected_contract_hash: contract.contract_hash,
        catalog_version: contract.object.catalog_version,
        structured_parameters: Vec::new(),
    };
    let procedure = LocalProcedure {
        contract: contract.as_ref(),
        required_permissions: contract.required_permissions.clone(),
        result_metadata: ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(1),
            column_count: contract.result_streams[0].columns.len() as u32,
            cardinality: Cardinality::One,
        },
        mutation_payload: b"Inventory.ReserveStock demo mutation".to_vec(),
        rows_affected: 1,
    };
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
