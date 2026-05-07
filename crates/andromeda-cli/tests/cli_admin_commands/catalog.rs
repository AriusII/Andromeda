use super::support::{
    assert_contains_all, assert_dispatch_error, assert_dispatch_success, assert_success, run_cli,
    run_cli_vec, stdout,
};

#[test]
fn catalog_list_procedures_command_executes() {
    assert_dispatch_success(["catalog", "list-procedures"]);
}

#[test]
fn catalog_list_procedures_with_namespace_filter() {
    assert_dispatch_success(["catalog", "list-procedures", "--namespace", "inventory"]);
}

#[test]
fn catalog_invalidate_cache_command_executes() {
    assert_dispatch_success(["catalog", "invalidate-cache"]);
}

#[test]
fn catalog_invalidate_cache_for_specific_procedure() {
    assert_dispatch_success(["catalog", "invalidate-cache", "--procedure-id", "1"]);
}

#[test]
fn catalog_show_contract_requires_procedure_id() {
    assert_dispatch_error(["catalog", "show-contract"]);
}

#[test]
fn catalog_show_contract_accepts_procedure_id() {
    assert_dispatch_success(["catalog", "show-contract", "1"]);
}

#[test]
fn catalog_resolve_manifest_requires_selector() {
    assert_dispatch_error(["catalog", "resolve-manifest"]);
}

#[test]
fn catalog_resolve_manifest_accepts_procedure_id() {
    assert_dispatch_success(["catalog", "resolve-manifest", "--procedure-id", "1"]);
}

#[test]
fn catalog_resolve_manifest_accepts_qualified_name() {
    assert_dispatch_success([
        "catalog",
        "resolve-manifest",
        "--qualified-name",
        "Inventory.ReserveStock",
    ]);
}

#[test]
fn catalog_help_command_executes() {
    assert_dispatch_success(["catalog", "--help"]);
}

#[test]
fn catalog_list_procedures_accepts_json_output() {
    assert_dispatch_success(["catalog", "list-procedures", "--json"]);
}

#[test]
fn catalog_show_contract_accepts_json_output() {
    assert_dispatch_success(["catalog", "show-contract", "1", "--json"]);
}

#[test]
fn catalog_show_contract_human_uses_native_nullability_terms() {
    let output = run_cli(["catalog", "show-contract", "1"]);
    assert_success(&output);
    let text = stdout(&output);
    assert_contains_all(
        &text,
        &[
            "Procedure Contract (mock contract preview)",
            "Input Columns:",
            "Output Columns:",
            "product_id: INT64 (required)",
            "remaining_quantity: INT32 (required)",
        ],
    );
    assert!(!text.contains("NOT NULL"));
    assert!(!text.contains("NULL"));
}

#[test]
fn catalog_preview_json_outputs_are_explicit() {
    for args in [
        vec!["catalog", "list-procedures", "--json"],
        vec!["catalog", "show-contract", "1", "--json"],
        vec!["catalog", "invalidate-cache", "--json"],
        vec![
            "catalog",
            "resolve-manifest",
            "--qualified-name",
            "Inventory.ReserveStock",
            "--json",
        ],
    ] {
        let output = run_cli_vec(args);
        assert_success(&output);
        let json = stdout(&output);
        assert_contains_all(
            &json,
            &[
                "\"contract_preview\":true",
                "\"mode\":\"contract_preview/mock_ephemeral\"",
                "\"runtime\":\"mock_ephemeral\"",
                "\"durable_catalog_state\":false",
                "\"catalog_runtime_queried\":false",
                "no CatalogServerRuntime durable source was queried",
            ],
        );
    }
}

#[test]
fn catalog_invalidate_cache_json_does_not_claim_runtime_mutation() {
    let output = run_cli([
        "catalog",
        "invalidate-cache",
        "--procedure-id",
        "1",
        "--json",
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.catalog.invalidate-cache.v1\"",
            "\"cache_invalidated\":false",
            "\"entries_cleared\":0",
            "no CatalogServerRuntime plan cache was invalidated",
        ],
    );
}

#[test]
fn catalog_resolve_manifest_json_is_preview_only() {
    let output = run_cli([
        "catalog",
        "resolve-manifest",
        "--procedure-id",
        "1",
        "--json",
    ]);
    assert_success(&output);
    let json = stdout(&output);
    assert_contains_all(
        &json,
        &[
            "\"schema\":\"andromeda.cli.catalog.resolve-manifest.v1\"",
            "\"resolution_status\":\"preview_resolved\"",
            "\"procedure_id\":1",
            "\"contract_preview\":true",
            "\"durable_catalog_state\":false",
        ],
    );
}
