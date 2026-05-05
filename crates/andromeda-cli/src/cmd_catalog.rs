//! Catalog administration commands.
//!
//! Provides CLI commands for:
//! - Listing procedures with IDs and contract hashes
//! - Clearing plan cache (all or specific procedures)
//! - Displaying procedure contract information

use crate::error::cli_error;
use andromeda_core::AndromedaResult;
use serde::Serialize;

/// Serializable procedure metadata.
#[derive(Debug, Clone, Serialize)]
pub struct ProcedureMetadata {
    pub procedure_id: u64,
    pub name: String,
    pub namespace: Option<String>,
    pub contract_hash: String,
    pub catalog_version: u64,
}

/// Serializable procedure contract.
#[derive(Debug, Clone, Serialize)]
pub struct ProcedureContractInfo {
    pub procedure_id: u64,
    pub name: String,
    pub contract_hash: String,
    pub input_columns: Vec<ColumnInfo>,
    pub output_columns: Vec<ColumnInfo>,
    pub isolation_level: String,
    pub access_mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub column_type: String,
    pub nullable: bool,
}

/// Serializable cache invalidation outcome.
#[derive(Debug, Clone, Serialize)]
pub struct CacheInvalidationOutcome {
    pub success: bool,
    pub entries_cleared: usize,
    pub message: String,
}

/// Parses and executes catalog subcommands.
pub fn run_catalog_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("list-procedures") => run_list_procedures(&args[1..]),
        Some("invalidate-cache") => run_invalidate_cache(&args[1..]),
        Some("show-contract") => run_show_contract(&args[1..]),
        Some("-h" | "--help" | "help") => {
            print_catalog_help();
            Ok(())
        }
        Some(cmd) => Err(cli_error(format!(
            "unknown catalog subcommand `{cmd}`; run `andromeda-cli catalog --help`"
        ))),
        None => {
            print_catalog_help();
            Ok(())
        }
    }
}

/// Lists procedures with IDs, names, and contract hashes.
fn run_list_procedures(args: &[String]) -> AndromedaResult<()> {
    let mut namespace: Option<String> = None;
    let mut json = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--namespace" => {
                i += 1;
                if i >= args.len() {
                    return Err(cli_error("--namespace requires a namespace argument"));
                }
                namespace = Some(args[i].clone());
            }
            "--json" => json = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!(
                    "unknown list-procedures option: {}",
                    opt
                )));
            }
            _ => {}
        }
        i += 1;
    }

    // MOCK: In a real implementation, this would query the catalog store.
    let procedures = vec![
        ProcedureMetadata {
            procedure_id: 1,
            name: "InventoryReserveStock".to_string(),
            namespace: Some("inventory".to_string()),
            contract_hash: "a1b2c3d4e5f6".to_string(),
            catalog_version: 1,
        },
        ProcedureMetadata {
            procedure_id: 2,
            name: "PaymentProcess".to_string(),
            namespace: Some("payments".to_string()),
            contract_hash: "f1e2d3c4b5a6".to_string(),
            catalog_version: 1,
        },
        ProcedureMetadata {
            procedure_id: 3,
            name: "UserAuthenticate".to_string(),
            namespace: Some("auth".to_string()),
            contract_hash: "7f8e9d0c1b2a".to_string(),
            catalog_version: 2,
        },
    ];

    let filtered: Vec<_> = if let Some(ns) = namespace {
        procedures
            .into_iter()
            .filter(|p| p.namespace.as_ref().map_or(false, |n| n == &ns))
            .collect()
    } else {
        procedures
    };

    if json {
        let json_str = serde_json::to_string_pretty(&filtered)
            .map_err(|e| cli_error(format!("failed to serialize procedure list: {}", e)))?;
        println!("{}", json_str);
    } else {
        print_procedures_human(&filtered);
    }

    Ok(())
}

/// Invalidates plan cache (all or specific procedure).
fn run_invalidate_cache(args: &[String]) -> AndromedaResult<()> {
    let mut procedure_id: Option<u64> = None;
    let mut json = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--procedure-id" => {
                i += 1;
                if i >= args.len() {
                    return Err(cli_error("--procedure-id requires a procedure ID argument"));
                }
                procedure_id = Some(
                    args[i]
                        .parse()
                        .map_err(|_| cli_error("procedure-id must be an unsigned integer"))?,
                );
            }
            "--json" => json = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(format!(
                    "unknown invalidate-cache option: {}",
                    opt
                )));
            }
            _ => {}
        }
        i += 1;
    }

    // MOCK: In a real implementation, this would invoke the plan cache invalidation.
    let entries_cleared = if procedure_id.is_some() { 1 } else { 42 };
    let scope = if procedure_id.is_some() {
        "procedure-specific"
    } else {
        "all procedures"
    };

    let outcome = CacheInvalidationOutcome {
        success: true,
        entries_cleared,
        message: format!("Cleared {} cache entries ({})", entries_cleared, scope),
    };

    if json {
        let json_str = serde_json::to_string_pretty(&outcome)
            .map_err(|e| cli_error(format!("failed to serialize invalidation outcome: {}", e)))?;
        println!("{}", json_str);
    } else {
        println!(
            "✓ Cache invalidated: {} entries cleared",
            outcome.entries_cleared
        );
        if let Some(pid) = procedure_id {
            println!("  (procedure {})", pid);
        } else {
            println!("  (all procedures)");
        }
    }

    Ok(())
}

/// Displays procedure contract information (wire format).
fn run_show_contract(args: &[String]) -> AndromedaResult<()> {
    if args.is_empty() {
        return Err(cli_error(
            "show-contract requires <procedure-id>; usage: `catalog show-contract <procedure-id>`",
        ));
    }

    let procedure_id: u64 = args[0]
        .parse()
        .map_err(|_| cli_error("procedure-id must be an unsigned integer"))?;

    let json = args.iter().any(|arg| arg == "--json");

    // MOCK: In a real implementation, this would query the catalog store for contract details.
    let contract = ProcedureContractInfo {
        procedure_id,
        name: "InventoryReserveStock".to_string(),
        contract_hash: "a1b2c3d4e5f6".to_string(),
        input_columns: vec![
            ColumnInfo {
                name: "product_id".to_string(),
                column_type: "INT64".to_string(),
                nullable: false,
            },
            ColumnInfo {
                name: "quantity".to_string(),
                column_type: "INT32".to_string(),
                nullable: false,
            },
        ],
        output_columns: vec![
            ColumnInfo {
                name: "remaining_quantity".to_string(),
                column_type: "INT32".to_string(),
                nullable: false,
            },
            ColumnInfo {
                name: "status".to_string(),
                column_type: "STRING".to_string(),
                nullable: false,
            },
        ],
        isolation_level: "Snapshot".to_string(),
        access_mode: "ReadWrite".to_string(),
    };

    if json {
        let json_str = serde_json::to_string_pretty(&contract)
            .map_err(|e| cli_error(format!("failed to serialize contract: {}", e)))?;
        println!("{}", json_str);
    } else {
        print_contract_human(&contract);
    }

    Ok(())
}

fn print_catalog_help() {
    println!("Andromeda catalog administration commands");
    println!();
    println!("USAGE: andromeda-cli catalog <SUBCOMMAND> [OPTIONS]");
    println!();
    println!("SUBCOMMANDS:");
    println!("  list-procedures         List procedures with IDs and contract hashes");
    println!("  invalidate-cache        Clear plan cache (all or specific procedure)");
    println!("  show-contract <id>      Display procedure contract information");
    println!();
    println!("OPTIONS:");
    println!("  --namespace <ns>        Filter procedures by namespace");
    println!("  --procedure-id <id>     Target specific procedure for cache invalidation");
    println!("  --json                  Output in JSON format (default: human-readable)");
    println!("  -h, --help              Show this help message");
}

fn print_procedures_human(procedures: &[ProcedureMetadata]) {
    println!("Procedures");
    println!("==========");
    println!(
        "{:<6} {:<30} {:<20} {:<15}",
        "ID", "Name", "Namespace", "Contract Hash"
    );
    println!("{}", "-".repeat(81));
    for proc in procedures {
        let ns = proc
            .namespace
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("(default)");
        println!(
            "{:<6} {:<30} {:<20} {:<15}",
            proc.procedure_id,
            proc.name,
            ns,
            &proc.contract_hash[..12]
        );
    }
}

fn print_contract_human(contract: &ProcedureContractInfo) {
    println!("Procedure Contract");
    println!("==================");
    println!("ID: {}", contract.procedure_id);
    println!("Name: {}", contract.name);
    println!("Contract Hash: {}", contract.contract_hash);
    println!();
    println!("Input Columns:");
    for col in &contract.input_columns {
        let nullable = if col.nullable { "NULL" } else { "NOT NULL" };
        println!("  - {}: {} ({})", col.name, col.column_type, nullable);
    }
    println!();
    println!("Output Columns:");
    for col in &contract.output_columns {
        let nullable = if col.nullable { "NULL" } else { "NOT NULL" };
        println!("  - {}: {} ({})", col.name, col.column_type, nullable);
    }
    println!();
    println!("Isolation Level: {}", contract.isolation_level);
    println!("Access Mode: {}", contract.access_mode);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_list_procedures_returns_ok() {
        let result = run_list_procedures(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn catalog_list_procedures_with_namespace() {
        let result = run_list_procedures(&["--namespace".to_string(), "inventory".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn catalog_list_procedures_with_json() {
        let result = run_list_procedures(&["--json".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn catalog_invalidate_cache_returns_ok() {
        let result = run_invalidate_cache(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn catalog_invalidate_cache_with_procedure_id() {
        let result = run_invalidate_cache(&["--procedure-id".to_string(), "1".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn catalog_invalidate_cache_rejects_invalid_id() {
        let result =
            run_invalidate_cache(&["--procedure-id".to_string(), "not_a_number".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn catalog_show_contract_requires_procedure_id() {
        let result = run_show_contract(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn catalog_show_contract_accepts_valid_id() {
        let result = run_show_contract(&["1".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn catalog_show_contract_rejects_invalid_id() {
        let result = run_show_contract(&["not_a_number".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn catalog_show_contract_with_json() {
        let result = run_show_contract(&["1".to_string(), "--json".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn catalog_command_unknown_subcommand() {
        let result = run_catalog_command(&["unknown".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn catalog_command_help() {
        let result = run_catalog_command(&["--help".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn catalog_command_no_args_shows_help() {
        let result = run_catalog_command(&[]);
        assert!(result.is_ok());
    }
}
