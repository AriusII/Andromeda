//! Multi-procedure handler contract tests.
//!
//! Validates the three concrete `ProcedureHandler` implementations:
//!
//! 1. `ReserveStockProcedureHandler` — existing write handler (regression).
//! 2. `InventoryQueryStockProcedureHandler` — new read-only handler (stock query).
//! 3. `InventoryReleaseStockProcedureHandler` — new write handler (stock release).
//!
//! ## Coverage areas
//!
//! - Handler construction: valid contract produces valid handler.
//! - Handler construction: wrong contract ID is rejected before execution.
//! - `execute()` returns a `LocalProcedure` that validates cleanly.
//! - `LocalProcedure` contracts match the handler contract.
//! - `result_metadata()` matches the effect (rows_returned, cardinality).
//! - `ProcedureRegistry` accepts all three handlers without ID conflicts.
//! - `ProcedureRegistry` rejects duplicate registration.
//! - Read-only guarantee: `QueryStock` handler produces empty mutation_payload.
//! - Write guarantee: `ReserveStock` and `ReleaseStock` have non-empty payload.
//! - Permission contract: required permissions are forwarded from the contract.
//! - `ProcedureRegistry::dispatch` validates permissions (privilege escalation blocked).

use andromeda_admission::InvocationContext;
use andromeda_error::AndromedaErrorKind;
use andromeda_execution::{ProcedureHandler, ProcedureRegistry};
use andromeda_inventory_demo::{
    INVENTORY_QUERY_STOCK_PROCEDURE_ID, INVENTORY_RELEASE_STOCK_PROCEDURE_ID,
    INVENTORY_RESERVE_STOCK_PROCEDURE_ID, InventoryQueryStockProcedureHandler,
    InventoryReleaseStockProcedureHandler, InventoryReserveStockExecutor, InventoryStock,
    QueryStockEffect, ReleaseStockEffect, ReserveStockCommand, ReserveStockEffect,
    ReserveStockProcedureHandler, inventory_query_stock_contract, inventory_release_stock_contract,
    inventory_reserve_stock_contract,
};
use andromeda_observability::TraceId;
use andromeda_srpl_ir::Cardinality;

// Helper constructors

fn reserve_stock_effect() -> ReserveStockEffect {
    InventoryReserveStockExecutor::reserve(
        ReserveStockCommand {
            product_id: 42,
            quantity: 10,
        },
        InventoryStock {
            product_id: 42,
            available_quantity: 100,
            version: 1,
        },
    )
    .expect("reserve stock effect must be producible")
}

fn query_stock_effect_found() -> QueryStockEffect {
    QueryStockEffect::found(InventoryStock {
        product_id: 42,
        available_quantity: 90,
        version: 2,
    })
    .expect("query stock found effect must be producible")
}

fn query_stock_effect_not_found() -> QueryStockEffect {
    QueryStockEffect::not_found()
}

fn release_stock_effect() -> ReleaseStockEffect {
    // Simulate restoring 10 units back to stock after a release.
    ReleaseStockEffect {
        previous_stock: InventoryStock {
            product_id: 42,
            available_quantity: 90,
            version: 2,
        },
        next_stock: InventoryStock {
            product_id: 42,
            available_quantity: 100,
            version: 3,
        },
        rows_affected: 2, // stock row + reservation row
    }
}

fn test_invocation_context(permissions: Vec<String>) -> InvocationContext {
    InvocationContext::new(TraceId::new(0x0001_0000), permissions)
}

fn inventory_registry_with_all_handlers() -> ProcedureRegistry {
    let mut registry = ProcedureRegistry::new();

    let reserve_contract = inventory_reserve_stock_contract().expect("fixture must be valid");
    let reserve_handler =
        ReserveStockProcedureHandler::new(&reserve_contract, &reserve_stock_effect())
            .expect("reserve handler fixture must be valid");
    registry
        .register(reserve_handler)
        .expect("reserve handler must register");

    let query_contract = inventory_query_stock_contract().expect("fixture must be valid");
    let query_handler =
        InventoryQueryStockProcedureHandler::new(&query_contract, &query_stock_effect_found())
            .expect("query handler fixture must be valid");
    registry
        .register(query_handler)
        .expect("query handler must register");

    let release_contract = inventory_release_stock_contract().expect("fixture must be valid");
    let release_handler =
        InventoryReleaseStockProcedureHandler::new(&release_contract, &release_stock_effect())
            .expect("release handler fixture must be valid");
    registry
        .register(release_handler)
        .expect("release handler must register");

    registry
}

// ReserveStockProcedureHandler — regression coverage

#[test]
fn reserve_stock_handler_constructs_from_valid_contract_and_effect() {
    let contract = inventory_reserve_stock_contract().expect("fixture must be valid");
    let effect = reserve_stock_effect();

    let handler = ReserveStockProcedureHandler::new(&contract, &effect)
        .expect("handler construction must succeed for valid contract");

    assert_eq!(handler.procedure_id(), INVENTORY_RESERVE_STOCK_PROCEDURE_ID);
    assert_eq!(handler.contract(), contract.as_ref());
}

#[test]
fn reserve_stock_handler_execute_returns_valid_local_procedure() {
    let contract = inventory_reserve_stock_contract().expect("fixture must be valid");
    let effect = reserve_stock_effect();
    let handler = ReserveStockProcedureHandler::new(&contract, &effect).unwrap();

    let ctx = test_invocation_context(vec!["Inventory.ReserveStock.Execute".to_string()]);
    let procedure = handler.execute(ctx).expect("execute must succeed");

    procedure
        .validate()
        .expect("produced LocalProcedure must validate");
    assert_eq!(procedure.contract, contract.as_ref());
    assert!(
        !procedure.mutation_payload.is_empty(),
        "write handler must produce non-empty payload"
    );
    assert!(
        procedure.rows_affected > 0,
        "write handler must report rows_affected > 0"
    );
}

#[test]
fn reserve_stock_handler_result_metadata_matches_one_cardinality() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let effect = reserve_stock_effect();
    let handler = ReserveStockProcedureHandler::new(&contract, &effect).unwrap();
    let meta = handler.result_metadata();

    assert_eq!(meta.cardinality, Cardinality::One);
    assert_eq!(meta.row_count_exact, Some(1));
    assert_eq!(meta.row_count_max, Some(1));
}

// InventoryQueryStockProcedureHandler — stock found path

#[test]
fn query_stock_handler_constructs_from_valid_contract_and_found_effect() {
    let contract = inventory_query_stock_contract().expect("fixture must be valid");
    let effect = query_stock_effect_found();

    let handler = InventoryQueryStockProcedureHandler::new(&contract, &effect)
        .expect("handler construction must succeed for found effect");

    assert_eq!(handler.procedure_id(), INVENTORY_QUERY_STOCK_PROCEDURE_ID);
    assert_eq!(handler.contract(), contract.as_ref());
}

#[test]
fn query_stock_handler_execute_found_returns_read_only_local_procedure() {
    let contract = inventory_query_stock_contract().unwrap();
    let effect = query_stock_effect_found();
    let handler = InventoryQueryStockProcedureHandler::new(&contract, &effect).unwrap();

    let ctx = test_invocation_context(vec!["Inventory.QueryStock.Execute".to_string()]);
    let procedure = handler.execute(ctx).expect("execute must succeed");

    procedure.validate().expect("LocalProcedure must validate");
    assert_eq!(procedure.contract, contract.as_ref());

    // Read-only guarantee: no mutation payload, no rows affected.
    assert!(
        procedure.mutation_payload.is_empty(),
        "QueryStock must produce an empty mutation_payload (read-only)"
    );
    assert_eq!(
        procedure.rows_affected, 0,
        "QueryStock must report rows_affected = 0 (read-only)"
    );
}

#[test]
fn query_stock_handler_result_metadata_found_has_optional_one_with_one_row() {
    let contract = inventory_query_stock_contract().unwrap();
    let effect = query_stock_effect_found();
    let handler = InventoryQueryStockProcedureHandler::new(&contract, &effect).unwrap();
    let meta = handler.result_metadata();

    assert_eq!(meta.cardinality, Cardinality::OptionalOne);
    assert_eq!(meta.row_count_exact, Some(1), "found: exactly 1 row");
    assert_eq!(meta.row_count_max, Some(1));
    // 3 columns declared in the fixture: ProductId, AvailableQuantity, Version.
    assert_eq!(meta.column_count, 3);
}

// InventoryQueryStockProcedureHandler — stock not found path

#[test]
fn query_stock_handler_constructs_from_not_found_effect() {
    let contract = inventory_query_stock_contract().unwrap();
    let effect = query_stock_effect_not_found();

    let handler = InventoryQueryStockProcedureHandler::new(&contract, &effect)
        .expect("handler must accept a not-found effect");

    assert_eq!(handler.procedure_id(), INVENTORY_QUERY_STOCK_PROCEDURE_ID);
}

#[test]
fn query_stock_handler_execute_not_found_returns_empty_result() {
    let contract = inventory_query_stock_contract().unwrap();
    let effect = query_stock_effect_not_found();
    let handler = InventoryQueryStockProcedureHandler::new(&contract, &effect).unwrap();

    let ctx = test_invocation_context(vec!["Inventory.QueryStock.Execute".to_string()]);
    let procedure = handler.execute(ctx).expect("execute must succeed");

    procedure.validate().expect("LocalProcedure must validate");
    assert!(procedure.mutation_payload.is_empty());
    assert_eq!(procedure.rows_affected, 0);
}

#[test]
fn query_stock_handler_result_metadata_not_found_has_optional_one_with_zero_rows() {
    let contract = inventory_query_stock_contract().unwrap();
    let effect = query_stock_effect_not_found();
    let handler = InventoryQueryStockProcedureHandler::new(&contract, &effect).unwrap();
    let meta = handler.result_metadata();

    assert_eq!(meta.cardinality, Cardinality::OptionalOne);
    assert_eq!(meta.row_count_exact, Some(0), "not found: exactly 0 rows");
    assert_eq!(meta.row_count_max, Some(1));
}

// InventoryReleaseStockProcedureHandler

#[test]
fn release_stock_handler_constructs_from_valid_contract_and_effect() {
    let contract = inventory_release_stock_contract().expect("fixture must be valid");
    let effect = release_stock_effect();

    let handler = InventoryReleaseStockProcedureHandler::new(&contract, &effect)
        .expect("handler construction must succeed");

    assert_eq!(handler.procedure_id(), INVENTORY_RELEASE_STOCK_PROCEDURE_ID);
    assert_eq!(handler.contract(), contract.as_ref());
}

#[test]
fn release_stock_handler_execute_returns_valid_write_local_procedure() {
    let contract = inventory_release_stock_contract().unwrap();
    let effect = release_stock_effect();
    let handler = InventoryReleaseStockProcedureHandler::new(&contract, &effect).unwrap();

    let ctx = test_invocation_context(vec!["Inventory.ReleaseStock.Execute".to_string()]);
    let procedure = handler.execute(ctx).expect("execute must succeed");

    procedure.validate().expect("LocalProcedure must validate");
    assert_eq!(procedure.contract, contract.as_ref());
    assert!(
        !procedure.mutation_payload.is_empty(),
        "release is a write; payload must be non-empty"
    );
    assert!(
        procedure.rows_affected > 0,
        "release must report rows_affected > 0"
    );
}

#[test]
fn release_stock_handler_result_metadata_has_one_cardinality() {
    let contract = inventory_release_stock_contract().unwrap();
    let effect = release_stock_effect();
    let handler = InventoryReleaseStockProcedureHandler::new(&contract, &effect).unwrap();
    let meta = handler.result_metadata();

    assert_eq!(meta.cardinality, Cardinality::One);
    assert_eq!(meta.row_count_exact, Some(1));
    assert_eq!(meta.row_count_max, Some(1));
    // 1 column declared in the fixture: Released (bool).
    assert_eq!(meta.column_count, 1);
}

// Handler contract validation: wrong procedure id is rejected

#[test]
fn query_stock_handler_rejects_reserve_stock_contract() {
    let wrong_contract = inventory_reserve_stock_contract().unwrap();
    let effect = query_stock_effect_found();

    let result = InventoryQueryStockProcedureHandler::new(&wrong_contract, &effect);
    assert!(
        result.is_err(),
        "QueryStock handler must reject a ReserveStock contract"
    );
}

#[test]
fn release_stock_handler_rejects_reserve_stock_contract() {
    let wrong_contract = inventory_reserve_stock_contract().unwrap();
    let effect = release_stock_effect();

    let result = InventoryReleaseStockProcedureHandler::new(&wrong_contract, &effect);
    assert!(
        result.is_err(),
        "ReleaseStock handler must reject a ReserveStock contract"
    );
}

#[test]
fn reserve_stock_handler_rejects_query_stock_contract() {
    let wrong_contract = inventory_query_stock_contract().unwrap();
    let effect = reserve_stock_effect();

    let result = ReserveStockProcedureHandler::new(&wrong_contract, &effect);
    assert!(
        result.is_err(),
        "ReserveStock handler must reject a QueryStock contract"
    );
}

// ProcedureRegistry — multi-handler registration

#[test]
fn procedure_registry_accepts_all_three_handlers_without_conflict() {
    let registry = inventory_registry_with_all_handlers();

    assert_eq!(registry.len(), 3);
    assert!(registry.contains(INVENTORY_RESERVE_STOCK_PROCEDURE_ID));
    assert!(registry.contains(INVENTORY_QUERY_STOCK_PROCEDURE_ID));
    assert!(registry.contains(INVENTORY_RELEASE_STOCK_PROCEDURE_ID));
}

#[test]
fn procedure_registry_rejects_duplicate_registration() {
    let mut registry = ProcedureRegistry::new();

    let contract = inventory_reserve_stock_contract().unwrap();
    let handler1 = ReserveStockProcedureHandler::new(&contract, &reserve_stock_effect()).unwrap();
    let handler2 = ReserveStockProcedureHandler::new(&contract, &reserve_stock_effect()).unwrap();

    registry
        .register(handler1)
        .expect("first registration must succeed");
    let result = registry.register(handler2);
    assert!(
        result.is_err(),
        "duplicate ProcedureId registration must be rejected"
    );
}

#[test]
fn procedure_registry_dispatches_query_stock_as_read_only_procedure() {
    let registry = inventory_registry_with_all_handlers();
    let ctx = test_invocation_context(vec!["Inventory.QueryStock.Execute".to_string()]);

    let procedure = registry
        .dispatch(INVENTORY_QUERY_STOCK_PROCEDURE_ID, ctx)
        .expect("cataloged QueryStock handler must dispatch");

    procedure
        .validate()
        .expect("dispatched QueryStock Procedure must validate");
    assert!(
        procedure.mutation_payload.is_empty(),
        "read-only QueryStock dispatch must not produce mutation payload"
    );
    assert_eq!(
        procedure.rows_affected, 0,
        "read-only QueryStock dispatch must not report writes"
    );
    assert_eq!(
        procedure.result_metadata.cardinality,
        Cardinality::OptionalOne
    );
}

#[test]
fn procedure_registry_dispatch_denies_permission_outside_handler_contract() {
    let registry = inventory_registry_with_all_handlers();
    let ctx = test_invocation_context(vec![
        "Inventory.QueryStock.Execute".to_string(),
        "Inventory.ReleaseStock.Execute".to_string(),
    ]);

    let err = registry
        .dispatch(INVENTORY_QUERY_STOCK_PROCEDURE_ID, ctx)
        .expect_err("dispatch permissions must stay within the handler contract");

    assert_eq!(err.kind(), AndromedaErrorKind::Security);
    assert!(err.message().contains("Inventory.ReleaseStock.Execute"));
    assert!(err.message().contains("handler contract scope"));
}

// Query effect construction validation

#[test]
fn query_stock_effect_found_validates_positive_product_id() {
    let bad_stock = InventoryStock {
        product_id: -1, // invalid
        available_quantity: 10,
        version: 1,
    };
    let result = QueryStockEffect::found(bad_stock);
    assert!(
        result.is_err(),
        "QueryStockEffect::found must reject non-positive product_id"
    );
}

#[test]
fn release_stock_effect_mutation_payload_is_deterministic() {
    let effect = release_stock_effect();
    let payload1 = effect.mutation_payload();
    let payload2 = effect.mutation_payload();
    assert_eq!(payload1, payload2, "mutation_payload must be deterministic");
    assert!(
        payload1.starts_with(b"andromeda.business.inventory.release-stock.v1"),
        "payload must carry domain tag"
    );
}

#[test]
fn release_stock_command_validates_positive_fields() {
    use andromeda_inventory_demo::ReleaseStockCommand;

    let zero_product = ReleaseStockCommand {
        product_id: 0,
        quantity: 10,
    };
    assert!(zero_product.validate().is_err(), "product_id=0 must fail");

    let zero_quantity = ReleaseStockCommand {
        product_id: 1,
        quantity: 0,
    };
    assert!(zero_quantity.validate().is_err(), "quantity=0 must fail");

    let valid = ReleaseStockCommand {
        product_id: 1,
        quantity: 5,
    };
    assert!(valid.validate().is_ok(), "valid command must pass");
}

#[test]
fn query_stock_command_validates_positive_product_id() {
    use andromeda_inventory_demo::QueryStockCommand;

    let zero = QueryStockCommand { product_id: 0 };
    assert!(zero.validate().is_err(), "product_id=0 must fail");

    let negative = QueryStockCommand { product_id: -5 };
    assert!(
        negative.validate().is_err(),
        "negative product_id must fail"
    );

    let valid = QueryStockCommand { product_id: 1 };
    assert!(valid.validate().is_ok(), "valid command must pass");
}
