mod output;
mod parsing;
mod preview;
mod types;

use crate::error::cli_error;
use andromeda_core::AndromedaResult;
use output::{
    print_cache_invalidation_human, print_cache_invalidation_json, print_catalog_help,
    print_contract_human, print_contract_json, print_manifest_human, print_manifest_json,
    print_procedures_human, print_procedures_json,
};
use parsing::{
    ManifestSelector, parse_invalidate_cache_options, parse_list_procedures_options,
    parse_resolve_manifest_options, parse_show_contract_options,
};
use preview::{preview_contract, preview_manifest, preview_procedures};
use types::CacheInvalidationOutcome;

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
    let options = parse_list_procedures_options(args)?;
    let procedures = preview_procedures(options.namespace.as_deref());

    if options.json_output {
        print_procedures_json(&procedures);
    } else {
        print_procedures_human(&procedures);
    }

    Ok(())
}

/// Invalidates plan cache (all or specific procedure).
fn run_invalidate_cache(args: &[String]) -> AndromedaResult<()> {
    let options = parse_invalidate_cache_options(args)?;
    let scope = if options.procedure_id.is_some() {
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

    if options.json_output {
        print_cache_invalidation_json(&outcome);
    } else {
        print_cache_invalidation_human(&outcome, options.procedure_id);
    }

    Ok(())
}

/// Displays procedure contract information (wire format).
fn run_show_contract(args: &[String]) -> AndromedaResult<()> {
    let options = parse_show_contract_options(args)?;
    let contract = preview_contract(options.procedure_id);

    if options.json_output {
        print_contract_json(&contract);
    } else {
        print_contract_human(&contract);
    }

    Ok(())
}

/// Resolves a procedure manifest preview.
fn run_resolve_manifest(args: &[String]) -> AndromedaResult<()> {
    let options = parse_resolve_manifest_options(args)?;
    let manifest = match options.selector {
        ManifestSelector::ProcedureId(procedure_id) => preview_manifest(Some(procedure_id), None),
        ManifestSelector::QualifiedName(qualified_name) => {
            preview_manifest(None, Some(qualified_name.as_str()))
        }
    };

    if options.json_output {
        print_manifest_json(&manifest);
    } else {
        print_manifest_human(&manifest);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic_json::json_string;

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
