use crate::diagnostic_json::{json_option_string, json_string};

use super::types::{
    CATALOG_PREVIEW_MESSAGE, CATALOG_PREVIEW_MODE, CATALOG_PREVIEW_RUNTIME,
    CacheInvalidationOutcome, ColumnInfo, ProcedureContractInfo, ProcedureManifestInfo,
    ProcedureMetadata,
};

pub(super) fn print_catalog_help() {
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

pub(super) fn print_procedures_human(procedures: &[ProcedureMetadata]) {
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
    for procedure in procedures {
        let namespace = procedure.namespace.as_deref().unwrap_or("(default)");
        println!(
            "{:<6} {:<30} {:<20} {:<15} {:<15}",
            procedure.procedure_id,
            procedure.name,
            namespace,
            &procedure.contract_hash[..12],
            procedure.catalog_version
        );
    }
}

pub(super) fn print_cache_invalidation_human(
    outcome: &CacheInvalidationOutcome,
    procedure_id: Option<u64>,
) {
    if outcome.success {
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
}

pub(super) fn print_contract_human(contract: &ProcedureContractInfo) {
    println!("Procedure Contract (mock contract preview)");
    println!("==========================================");
    println!("Mode: {}", CATALOG_PREVIEW_MODE);
    println!("Runtime: {}", CATALOG_PREVIEW_RUNTIME);
    println!("Durable Catalog State: false");
    println!("Catalog Runtime Queried: false");
    println!("ID: {}", contract.procedure_id);
    println!("Name: {}", contract.name);
    println!("Contract Hash: {}", contract.contract_hash);
    print_column_section("Input Columns", &contract.input_columns);
    print_column_section("Output Columns", &contract.output_columns);
    println!();
    println!("Isolation Level: {}", contract.isolation_level);
    println!("Access Mode: {}", contract.access_mode);
}

pub(super) fn print_manifest_human(manifest: &ProcedureManifestInfo) {
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
    print_column_section("Input Columns", &manifest.input_columns);
    print_column_section("Output Columns", &manifest.output_columns);
}

pub(super) fn print_procedures_json(procedures: &[ProcedureMetadata]) {
    let procedures_json = json_array(procedures, |procedure| {
        format!(
            "{{\"procedure_id\":{},\"name\":{},\"namespace\":{},\"contract_hash\":{},\"catalog_version\":{}}}",
            procedure.procedure_id,
            json_string(&procedure.name),
            json_option_string(procedure.namespace.as_deref()),
            json_string(&procedure.contract_hash),
            procedure.catalog_version,
        )
    });
    println!(
        "{{\"schema\":\"andromeda.cli.catalog.list-procedures.v1\",\"contract_preview\":true,\"mode\":{},\"runtime\":{},\"durable_catalog_state\":false,\"catalog_runtime_queried\":false,\"procedures\":{},\"message\":{}}}",
        json_string(CATALOG_PREVIEW_MODE),
        json_string(CATALOG_PREVIEW_RUNTIME),
        procedures_json,
        json_string(CATALOG_PREVIEW_MESSAGE),
    );
}

pub(super) fn print_cache_invalidation_json(outcome: &CacheInvalidationOutcome) {
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

pub(super) fn print_contract_json(contract: &ProcedureContractInfo) {
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

pub(super) fn print_manifest_json(manifest: &ProcedureManifestInfo) {
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
    json_array(columns, |column| {
        format!(
            "{{\"name\":{},\"column_type\":{},\"nullable\":{}}}",
            json_string(&column.name),
            json_string(&column.column_type),
            column.nullable,
        )
    })
}

fn print_column_section(title: &str, columns: &[ColumnInfo]) {
    println!();
    println!("{title}:");
    for column in columns {
        println!(
            "  - {}: {} ({})",
            column.name,
            column.column_type,
            column_nullability(column)
        );
    }
}

fn column_nullability(column: &ColumnInfo) -> &'static str {
    if column.nullable {
        "optional"
    } else {
        "required"
    }
}

fn json_array<T>(items: &[T], render: impl FnMut(&T) -> String) -> String {
    let entries = items.iter().map(render).collect::<Vec<_>>().join(",");
    format!("[{}]", entries)
}
