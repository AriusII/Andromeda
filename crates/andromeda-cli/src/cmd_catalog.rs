use crate::diagnostic_json::{JSON_FLAG, json_option_string, json_string};
use crate::error::cli_error;
use crate::parse::{next_option_value_rejecting_flag, parse_u64};
use andromeda_core::AndromedaResult;

const CATALOG_PREVIEW_MODE: &str = "contract_preview/mock_ephemeral";
const CATALOG_PREVIEW_RUNTIME: &str = "mock_ephemeral";
const CATALOG_PREVIEW_MESSAGE: &str =
    "mock catalog output: no CatalogServerRuntime durable source was queried";

/// Procedure metadata.
#[derive(Debug, Clone)]
pub struct ProcedureMetadata {
    pub procedure_id: u64,
    pub name: String,
    pub namespace: Option<String>,
    pub contract_hash: String,
    pub catalog_version: u64,
}

/// Procedure contract.
#[derive(Debug, Clone)]
pub struct ProcedureContractInfo {
    pub procedure_id: u64,
    pub name: String,
    pub contract_hash: String,
    pub input_columns: Vec<ColumnInfo>,
    pub output_columns: Vec<ColumnInfo>,
    pub isolation_level: String,
    pub access_mode: String,
}

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub column_type: String,
    pub nullable: bool,
}

/// Cache invalidation outcome.
#[derive(Debug, Clone)]
pub struct CacheInvalidationOutcome {
    pub success: bool,
    pub entries_cleared: usize,
    pub cache_invalidated: bool,
    pub message: String,
}

/// Procedure manifest preview.
#[derive(Debug, Clone)]
pub struct ProcedureManifestInfo {
    pub procedure_id: u64,
    pub qualified_name: String,
    pub catalog_version: u64,
    pub contract_hash: String,
    pub input_columns: Vec<ColumnInfo>,
    pub output_columns: Vec<ColumnInfo>,
    pub is_mutable: bool,
    pub min_compatible_version: u64,
}

/// Parses and executes catalog subcommands.
pub fn run_catalog_command(args: &[String]) -> AndromedaResult<()> {
    match args.first().map(String::as_str) {
        Some("list-procedures") => run_list_procedures(&args[1..]),
        Some("invalidate-cache") => run_invalidate_cache(&args[1..]),
        Some("show-contract") => run_show_contract(&args[1..]),
        Some("resolve-manifest") => run_resolve_manifest(&args[1..]),
        Some("-h" | "--help" | "help") => {
            print_catalog_help();
            Ok(())
        }
        Some(_) => Err(cli_error(
            "unknown catalog subcommand; run `andromeda-cli catalog --help`",
        )),
        None => {
            print_catalog_help();
            Ok(())
        }
    }
}

/// Lists procedures with IDs, names, and contract hashes.
fn run_list_procedures(args: &[String]) -> AndromedaResult<()> {
    let mut namespace: Option<String> = None;
    let mut json_output = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--namespace" => {
                namespace = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--namespace requires a namespace argument",
                    )?
                    .to_string(),
                );
            }
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown list-procedures option; supported options are --namespace and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected list-procedures argument; supported options are --namespace and --json",
                ));
            }
        }
        i += 1;
    }

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
            .filter(|p| p.namespace.as_ref() == Some(&ns))
            .collect()
    } else {
        procedures
    };

    if json_output {
        print_procedures_json(&filtered);
    } else {
        print_procedures_human(&filtered);
    }

    Ok(())
}

/// Invalidates plan cache (all or specific procedure).
fn run_invalidate_cache(args: &[String]) -> AndromedaResult<()> {
    let mut procedure_id: Option<u64> = None;
    let mut json_output = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--procedure-id" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--procedure-id requires a procedure ID argument",
                )?;
                procedure_id = Some(parse_u64(
                    value,
                    "procedure-id must be an unsigned integer",
                )?);
            }
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown invalidate-cache option; supported options are --procedure-id and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected invalidate-cache argument; supported options are --procedure-id and --json",
                ));
            }
        }
        i += 1;
    }

    let scope = if procedure_id.is_some() {
        "procedure-specific"
    } else {
        "all procedures"
    };

    let outcome = CacheInvalidationOutcome {
        success: true,
        entries_cleared: 0,
        cache_invalidated: false,
        message: format!(
            "Contract preview only: no CatalogServerRuntime durable source was queried; no CatalogServerRuntime plan cache was invalidated ({scope})"
        ),
    };

    if json_output {
        print_cache_invalidation_json(&outcome);
    } else if outcome.success {
        println!("{}", outcome.message);
        println!("Mode: {}", CATALOG_PREVIEW_MODE);
        println!("Runtime: {}", CATALOG_PREVIEW_RUNTIME);
        println!("Durable Catalog State: false");
        println!("Cache Invalidated: {}", outcome.cache_invalidated);
        println!("Entries Cleared: {}", outcome.entries_cleared);
        if let Some(pid) = procedure_id {
            println!("  (procedure {})", pid);
        } else {
            println!("  (all procedures)");
        }
    } else {
        println!("✗ Cache invalidation failed: {}", outcome.message);
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

    let procedure_id = parse_u64(&args[0], "procedure-id must be an unsigned integer")?;

    let mut json_output = false;
    for arg in &args[1..] {
        match arg.as_str() {
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown catalog show-contract option; supported option is --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected catalog show-contract argument; supported option is --json",
                ));
            }
        }
    }

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

    if json_output {
        print_contract_json(&contract);
    } else {
        print_contract_human(&contract);
    }

    Ok(())
}

/// Resolves a procedure manifest preview.
fn run_resolve_manifest(args: &[String]) -> AndromedaResult<()> {
    let mut procedure_id: Option<u64> = None;
    let mut qualified_name: Option<String> = None;
    let mut json_output = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--procedure-id" => {
                let value = next_option_value_rejecting_flag(
                    args,
                    &mut i,
                    "--procedure-id requires a procedure ID argument",
                )?;
                procedure_id = Some(parse_u64(
                    value,
                    "procedure-id must be an unsigned integer",
                )?);
            }
            "--qualified-name" => {
                qualified_name = Some(
                    next_option_value_rejecting_flag(
                        args,
                        &mut i,
                        "--qualified-name requires a Procedure qualified name argument",
                    )?
                    .to_string(),
                );
            }
            JSON_FLAG => json_output = true,
            opt if opt.starts_with("--") => {
                return Err(cli_error(
                    "unknown resolve-manifest option; supported options are --procedure-id, --qualified-name, and --json",
                ));
            }
            _ => {
                return Err(cli_error(
                    "unexpected resolve-manifest argument; supported options are --procedure-id, --qualified-name, and --json",
                ));
            }
        }
        i += 1;
    }

    if procedure_id.is_some() == qualified_name.is_some() {
        return Err(cli_error(
            "resolve-manifest requires exactly one selector: --procedure-id <id> or --qualified-name <name>",
        ));
    }

    let manifest = preview_manifest(procedure_id, qualified_name.as_deref());

    if json_output {
        print_manifest_json(&manifest);
    } else {
        print_manifest_human(&manifest);
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
    println!("  resolve-manifest        Resolve Procedure manifest metadata");
    println!();
    println!("OPTIONS:");
    println!("  --namespace <ns>        Filter procedures by namespace");
    println!("  --procedure-id <id>     Target specific procedure for cache invalidation");
    println!("  --qualified-name <name> Target Procedure by qualified name");
    println!("  --json                  Emit diagnostic machine-readable JSON output");
    println!("  -h, --help              Show this help message");
}

fn print_procedures_human(procedures: &[ProcedureMetadata]) {
    println!("Procedures (mock contract preview)");
    println!("===================================");
    println!("Mode: {}", CATALOG_PREVIEW_MODE);
    println!("Runtime: {}", CATALOG_PREVIEW_RUNTIME);
    println!("Durable Catalog State: false");
    println!("Catalog Runtime Queried: false");
    println!(
        "{:<6} {:<30} {:<20} {:<15} {:<15}",
        "ID", "Name", "Namespace", "Contract Hash", "Catalog Version"
    );
    println!("{}", "-".repeat(98));
    for proc in procedures {
        let ns = proc.namespace.as_deref().unwrap_or("(default)");
        println!(
            "{:<6} {:<30} {:<20} {:<15} {:<15}",
            proc.procedure_id,
            proc.name,
            ns,
            &proc.contract_hash[..12],
            proc.catalog_version
        );
    }
}

fn print_contract_human(contract: &ProcedureContractInfo) {
    println!("Procedure Contract (mock contract preview)");
    println!("==========================================");
    println!("Mode: {}", CATALOG_PREVIEW_MODE);
    println!("Runtime: {}", CATALOG_PREVIEW_RUNTIME);
    println!("Durable Catalog State: false");
    println!("Catalog Runtime Queried: false");
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

fn print_manifest_human(manifest: &ProcedureManifestInfo) {
    println!("Procedure Manifest (mock contract preview)");
    println!("==========================================");
    println!("Mode: {}", CATALOG_PREVIEW_MODE);
    println!("Runtime: {}", CATALOG_PREVIEW_RUNTIME);
    println!("Durable Catalog State: false");
    println!("Catalog Runtime Queried: false");
    println!("Resolution Status: preview_resolved");
    println!("ID: {}", manifest.procedure_id);
    println!("Qualified Name: {}", manifest.qualified_name);
    println!("Catalog Version: {}", manifest.catalog_version);
    println!(
        "Minimum Compatible Version: {}",
        manifest.min_compatible_version
    );
    println!("Contract Hash: {}", manifest.contract_hash);
    println!("Mutable: {}", manifest.is_mutable);
    println!();
    println!("Input Columns:");
    for col in &manifest.input_columns {
        let nullable = if col.nullable { "optional" } else { "required" };
        println!("  - {}: {} ({})", col.name, col.column_type, nullable);
    }
    println!();
    println!("Output Columns:");
    for col in &manifest.output_columns {
        let nullable = if col.nullable { "optional" } else { "required" };
        println!("  - {}: {} ({})", col.name, col.column_type, nullable);
    }
}

fn print_procedures_json(procedures: &[ProcedureMetadata]) {
    let entries = procedures
        .iter()
        .map(|proc| {
            format!(
                "{{\"procedure_id\":{},\"name\":{},\"namespace\":{},\"contract_hash\":{},\"catalog_version\":{}}}",
                proc.procedure_id,
                json_string(&proc.name),
                json_option_string(proc.namespace.as_deref()),
                json_string(&proc.contract_hash),
                proc.catalog_version,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    println!(
        "{{\"schema\":\"andromeda.cli.catalog.list-procedures.v1\",\"contract_preview\":true,\"mode\":{},\"runtime\":{},\"durable_catalog_state\":false,\"catalog_runtime_queried\":false,\"procedures\":[{}],\"message\":{}}}",
        json_string(CATALOG_PREVIEW_MODE),
        json_string(CATALOG_PREVIEW_RUNTIME),
        entries,
        json_string(CATALOG_PREVIEW_MESSAGE),
    );
}

fn print_cache_invalidation_json(outcome: &CacheInvalidationOutcome) {
    println!(
        "{{\"schema\":\"andromeda.cli.catalog.invalidate-cache.v1\",\"contract_preview\":true,\"mode\":{},\"runtime\":{},\"durable_catalog_state\":false,\"catalog_runtime_queried\":false,\"success\":{},\"cache_invalidated\":{},\"entries_cleared\":{},\"message\":{}}}",
        json_string(CATALOG_PREVIEW_MODE),
        json_string(CATALOG_PREVIEW_RUNTIME),
        outcome.success,
        outcome.cache_invalidated,
        outcome.entries_cleared,
        json_string(&outcome.message),
    );
}

fn print_contract_json(contract: &ProcedureContractInfo) {
    println!(
        "{{\"schema\":\"andromeda.cli.catalog.show-contract.v1\",\"contract_preview\":true,\"mode\":{},\"runtime\":{},\"durable_catalog_state\":false,\"catalog_runtime_queried\":false,\"procedure_id\":{},\"name\":{},\"contract_hash\":{},\"input_columns\":{},\"output_columns\":{},\"isolation_level\":{},\"access_mode\":{},\"message\":{}}}",
        json_string(CATALOG_PREVIEW_MODE),
        json_string(CATALOG_PREVIEW_RUNTIME),
        contract.procedure_id,
        json_string(&contract.name),
        json_string(&contract.contract_hash),
        columns_json(&contract.input_columns),
        columns_json(&contract.output_columns),
        json_string(&contract.isolation_level),
        json_string(&contract.access_mode),
        json_string(CATALOG_PREVIEW_MESSAGE),
    );
}

fn print_manifest_json(manifest: &ProcedureManifestInfo) {
    println!(
        "{{\"schema\":\"andromeda.cli.catalog.resolve-manifest.v1\",\"contract_preview\":true,\"mode\":{},\"runtime\":{},\"durable_catalog_state\":false,\"catalog_runtime_queried\":false,\"resolution_status\":\"preview_resolved\",\"procedure_id\":{},\"qualified_name\":{},\"catalog_version\":{},\"min_compatible_version\":{},\"contract_hash\":{},\"input_columns\":{},\"output_columns\":{},\"is_mutable\":{},\"message\":{}}}",
        json_string(CATALOG_PREVIEW_MODE),
        json_string(CATALOG_PREVIEW_RUNTIME),
        manifest.procedure_id,
        json_string(&manifest.qualified_name),
        manifest.catalog_version,
        manifest.min_compatible_version,
        json_string(&manifest.contract_hash),
        columns_json(&manifest.input_columns),
        columns_json(&manifest.output_columns),
        manifest.is_mutable,
        json_string(CATALOG_PREVIEW_MESSAGE),
    );
}

fn columns_json(columns: &[ColumnInfo]) -> String {
    let entries = columns
        .iter()
        .map(|col| {
            format!(
                "{{\"name\":{},\"column_type\":{},\"nullable\":{}}}",
                json_string(&col.name),
                json_string(&col.column_type),
                col.nullable,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{}]", entries)
}

fn preview_manifest(
    procedure_id: Option<u64>,
    qualified_name: Option<&str>,
) -> ProcedureManifestInfo {
    ProcedureManifestInfo {
        procedure_id: procedure_id.unwrap_or(1),
        qualified_name: qualified_name
            .unwrap_or("InventoryReserveStock")
            .to_string(),
        catalog_version: 1,
        min_compatible_version: 1,
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
        output_columns: vec![ColumnInfo {
            name: "status".to_string(),
            column_type: "STRING".to_string(),
            nullable: false,
        }],
        is_mutable: true,
    }
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
    fn catalog_list_procedures_rejects_flag_as_namespace() {
        let result = run_list_procedures(&["--namespace".to_string(), "--json".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn catalog_list_procedures_rejects_unexpected_argument() {
        let result = run_list_procedures(&["inventory".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn catalog_list_procedures_accepts_json_output() {
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
    fn catalog_invalidate_cache_rejects_unexpected_argument() {
        let result = run_invalidate_cache(&["extra".to_string()]);
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
    fn catalog_show_contract_accepts_json_output() {
        let result = run_show_contract(&["1".to_string(), "--json".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn catalog_resolve_manifest_requires_exactly_one_selector() {
        let result = run_resolve_manifest(&[]);
        assert!(result.is_err());

        let result = run_resolve_manifest(&[
            "--procedure-id".to_string(),
            "1".to_string(),
            "--qualified-name".to_string(),
            "Inventory.ReserveStock".to_string(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn catalog_resolve_manifest_accepts_procedure_id() {
        let result = run_resolve_manifest(&["--procedure-id".to_string(), "1".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn catalog_resolve_manifest_accepts_qualified_name_json_output() {
        let result = run_resolve_manifest(&[
            "--qualified-name".to_string(),
            "Inventory.ReserveStock".to_string(),
            "--json".to_string(),
        ]);
        assert!(result.is_ok());
    }

    #[test]
    fn catalog_json_string_escapes_diagnostic_fields() {
        assert_eq!(json_string("namespace\nname"), "\"namespace\\nname\"");
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
