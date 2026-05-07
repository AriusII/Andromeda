use super::support::*;

#[test]
fn reserve_stock_handler_exposes_existing_contract_and_local_shape() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let effect =
        InventoryReserveStockExecutor::reserve(reserve_command(42, 3), stock(42, 10, 7)).unwrap();
    let handler = ReserveStockProcedureHandler::new(&contract, &effect).unwrap();

    assert_eq!(handler.procedure_id(), contract.procedure_id);
    assert_eq!(handler.contract(), contract.as_ref());
    assert_eq!(handler.result_metadata().row_count_exact, Some(1));

    let procedure = handler.execute(context()).unwrap();
    procedure.validate().unwrap();
    assert_eq!(procedure.contract, contract.as_ref());
    assert_eq!(procedure.rows_affected, effect.rows_affected);
}

#[test]
fn reserve_stock_handler_registers_and_dispatches_without_runtime_integration() {
    let contract = inventory_reserve_stock_contract().unwrap();
    let effect =
        InventoryReserveStockExecutor::reserve(reserve_command(42, 1), stock(42, 5, 1)).unwrap();
    let handler = ReserveStockProcedureHandler::new(&contract, &effect).unwrap();
    let mut registry = ProcedureRegistry::new();
    registry.register(handler).unwrap();

    let procedure = registry
        .dispatch(contract.procedure_id, reserve_context())
        .unwrap();

    assert_eq!(procedure.contract, contract.as_ref());
    assert_eq!(procedure.rows_affected, effect.rows_affected);
}

#[test]
fn registry_registers_and_dispatches_reserve_stock_query_stock_and_release_stock() {
    let reserve_contract = inventory_reserve_stock_contract().unwrap();
    let reserve_effect =
        InventoryReserveStockExecutor::reserve(reserve_command(42, 1), stock(42, 5, 1)).unwrap();
    let reserve_handler =
        ReserveStockProcedureHandler::new(&reserve_contract, &reserve_effect).unwrap();
    let query_handler = QueryStockFakeProcedureHandler::new(stock(42, 4, 2));
    let query_contract = query_handler.contract();
    let release_handler = ReleaseStockFakeProcedureHandler::new(stock(42, 4, 2), 1);
    let release_contract = release_handler.contract();

    let mut registry = ProcedureRegistry::new();
    registry.register(reserve_handler).unwrap();
    registry.register(query_handler).unwrap();
    registry.register(release_handler).unwrap();

    assert_eq!(registry.len(), 3);
    assert!(registry.contains(reserve_contract.procedure_id));
    assert!(registry.contains(query_contract.procedure_id));
    assert!(registry.contains(release_contract.procedure_id));

    let reserve_procedure = registry
        .dispatch(reserve_contract.procedure_id, reserve_context())
        .unwrap();
    reserve_procedure.validate().unwrap();
    assert_eq!(reserve_procedure.contract, reserve_contract.as_ref());
    assert_eq!(
        reserve_procedure.rows_affected,
        reserve_effect.rows_affected
    );

    let query_procedure = registry
        .dispatch(query_contract.procedure_id, query_context())
        .unwrap();
    assert_inventory_result_shape(
        &query_procedure,
        query_contract,
        TEST_INVENTORY_QUERY_STOCK_PERMISSION,
        TEST_INVENTORY_QUERY_STOCK_STREAM_ID,
        TEST_INVENTORY_QUERY_STOCK_COLUMN_COUNT,
        0,
    );

    let release_procedure = registry
        .dispatch(release_contract.procedure_id, release_context())
        .unwrap();
    assert_inventory_result_shape(
        &release_procedure,
        release_contract,
        TEST_INVENTORY_RELEASE_STOCK_PERMISSION,
        TEST_INVENTORY_RELEASE_STOCK_STREAM_ID,
        TEST_INVENTORY_RELEASE_STOCK_COLUMN_COUNT,
        2,
    );
}
