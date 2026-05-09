use crate::args::{parse_inventory_recoverable_wal_path, parse_recovery_inspect_options};
use crate::audit::run_audit_command;
use crate::benchmark::run_benchmark_command;
use crate::cmd_backup::run_backup_command;
use crate::cmd_catalog::run_catalog_command;
use crate::cmd_protocol::run_protocol_smoke;
use crate::cmd_recovery::run_recovery_inspect;
use crate::cmd_restore::run_restore_command;
use crate::cmd_vertical::{print_help, run_inventory_demo, run_inventory_recoverable_demo};
use crate::error::cli_error;
use crate::hadr::run_hadr_command;
use andromeda_error::AndromedaResult;

/// Dispatches a CLI command based on the first argument.
pub fn dispatch_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("inventory-demo") => run_inventory_demo(),
        Some("vertical-v0" | "inventory-recoverable") => {
            run_inventory_recoverable_demo(parse_inventory_recoverable_wal_path(&args[1..])?)
        },
        Some("protocol-smoke") => {
            let detailed = args[1..].iter().any(|arg| arg == "--detail");
            let report = run_protocol_smoke(detailed)?;
            print!("{}", report);
            Ok(())
        },
        Some("recovery-inspect") => {
            let options = parse_recovery_inspect_options(&args[1..])?;
            run_recovery_inspect(options)
        },
        Some("hadr") => run_hadr_command(&args[1..]),
        Some("audit") => run_audit_command(&args[1..]),
        Some("benchmark") => run_benchmark_command(&args[1..]),
        Some("backup") => run_backup_command(&args[1..]),
        Some("restore") => run_restore_command(&args[1..]),
        Some("catalog") => run_catalog_command(&args[1..]),
        Some("-h" | "--help" | "help") | None => {
            print_help();
            Ok(())
        },
        Some(_) => Err(cli_error("unknown command; run `andromeda-cli --help`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command_returns_error() {
        let result = dispatch_command(&["unknown".to_string()]);
        assert!(result.is_err());
    }
}
