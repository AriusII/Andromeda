//! Vertical demo commands.

use andromeda_error::AndromedaResult;
use andromeda_hardware::HardwareProfile;
use andromeda_inventory_demo::{run_cli_inventory_demo, run_cli_inventory_recoverable};
use andromeda_wal::Lsn;
use std::path::PathBuf;

/// Runs the inventory procedure demo.
pub fn run_inventory_demo() -> AndromedaResult<()> {
    let outcome = run_cli_inventory_demo()?;

    println!("Andromeda inventory procedure demo");
    println!("procedure: {}", outcome.procedure);
    println!("status: {:?}", outcome.status);
    println!("rows affected: {:?}", outcome.rows_affected);
    println!("remaining stock: {}", outcome.remaining_stock);
    println!("durable WAL LSN: {}", outcome.durable_wal_lsn);
    println!("durable WAL records: {}", outcome.durable_wal_records);
    println!(
        "contract trace: {} ({})",
        outcome.contract_trace.decision, outcome.contract_trace.reason
    );
    if let Some(trace) = &outcome.authorization_trace {
        println!("authorization trace: {} ({})", trace.decision, trace.reason);
    }

    Ok(())
}

/// Runs the recoverable inventory procedure demo.
pub fn run_inventory_recoverable_demo(wal_path: PathBuf) -> AndromedaResult<()> {
    let outcome = run_cli_inventory_recoverable(&wal_path)?;

    let report = crate::cmd_recovery::recovery_report_for_path(&wal_path, Lsn::new(1))?;
    let replay_lsns = report
        .replay_lsns()
        .map(|lsn| lsn.get())
        .collect::<Vec<_>>();

    println!("Andromeda recoverable inventory procedure demo");
    println!("procedure: {}", outcome.procedure);
    println!("status: {:?}", outcome.status);
    println!("rows affected: {:?}", outcome.rows_affected);
    println!("remaining stock: {}", outcome.remaining_stock);
    println!("durable WAL LSN: {}", outcome.durable_wal_lsn);
    println!("WAL path: {}", outcome.wal_path.display());
    println!("recovery replay LSNs: {:?}", replay_lsns);
    println!("recovery boundary: {:?}", report.boundary_kind);
    println!("forensic required: {}", report.forensic_required);
    println!("result frames: {}", outcome.result_frame_count);
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
        "andromeda-transaction",
        "andromeda-mvcc",
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

    println!("run `andromeda-cli inventory-demo` for the inventory procedure demo");
    println!(
        "run `andromeda-cli inventory-recoverable [--wal <path>]` for the recoverable inventory procedure demo"
    );
    println!("run `andromeda-cli recovery-inspect <wal-path>` for a FileWal recovery report");
    println!(
        "run `andromeda-cli protocol-smoke [--detail]` for local protocol contract inspection"
    );
    println!();
    println!("ADMIN COMMANDS:");
    println!("run `andromeda-cli hadr [status|promote|demote|quorum]` for HADR administration");
    println!("run `andromeda-cli audit --help` for durable audit trace inspection");
    println!(
        "run `andromeda-cli benchmark [workloads|contract|run]` for bounded diagnostic benchmark orchestration"
    );
    println!("run `andromeda-cli backup [start|status|list]` for backup operations");
    println!("run `andromeda-cli restore <backup-id> [--pitr-lsn <lsn>]` for restore operations");
    println!(
        "run `andromeda-cli catalog [list-procedures|invalidate-cache|show-contract]` for catalog operations"
    );
}
