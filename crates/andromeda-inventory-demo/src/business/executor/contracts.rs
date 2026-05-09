use crate::{
    INVENTORY_QUERY_STOCK_OBJECT_ID, INVENTORY_QUERY_STOCK_PROCEDURE_ID,
    INVENTORY_RELEASE_STOCK_OBJECT_ID, INVENTORY_RELEASE_STOCK_PROCEDURE_ID,
    INVENTORY_RESERVE_STOCK_OBJECT_ID, INVENTORY_RESERVE_STOCK_PROCEDURE_ID,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::ProcedureContract;

pub(in crate::business) fn validate_inventory_reserve_stock_contract(
    contract: &ProcedureContract,
) -> AndromedaResult<()> {
    contract.validate_canonical_hash()?;

    if contract.procedure_id != INVENTORY_RESERVE_STOCK_PROCEDURE_ID {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.ReserveStock executable requires the reserve stock procedure id",
        ));
    }

    if contract.object.object_id != INVENTORY_RESERVE_STOCK_OBJECT_ID {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.ReserveStock executable requires the reserve stock catalog object id",
        ));
    }

    if contract.result_streams.len() != 1 || !contract.result_streams[0].row_count_exact_required {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.ReserveStock executable requires one exact result stream",
        ));
    }

    Ok(())
}

/// Validate a contract presented to `Inventory.QueryStock` handlers.
///
/// Checks procedure id, object id, and that a result stream is declared.
/// `row_count_exact_required` is intentionally not enforced here because the
/// `QueryStock` contract declares `OptionalOne` cardinality (0 or 1 rows),
/// meaning the count is not fixed at contract definition time.
pub(in crate::business) fn validate_inventory_query_stock_contract(
    contract: &ProcedureContract,
) -> AndromedaResult<()> {
    contract.validate_canonical_hash()?;

    if contract.procedure_id != INVENTORY_QUERY_STOCK_PROCEDURE_ID {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.QueryStock executable requires the query stock procedure id",
        ));
    }

    if contract.object.object_id != INVENTORY_QUERY_STOCK_OBJECT_ID {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.QueryStock executable requires the query stock catalog object id",
        ));
    }

    if contract.result_streams.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.QueryStock executable requires a declared result stream",
        ));
    }

    Ok(())
}

/// Validate a contract presented to `Inventory.ReleaseStock` handlers.
pub(in crate::business) fn validate_inventory_release_stock_contract(
    contract: &ProcedureContract,
) -> AndromedaResult<()> {
    contract.validate_canonical_hash()?;

    if contract.procedure_id != INVENTORY_RELEASE_STOCK_PROCEDURE_ID {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.ReleaseStock executable requires the release stock procedure id",
        ));
    }

    if contract.object.object_id != INVENTORY_RELEASE_STOCK_OBJECT_ID {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.ReleaseStock executable requires the release stock catalog object id",
        ));
    }

    if contract.result_streams.len() != 1 || !contract.result_streams[0].row_count_exact_required {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "Inventory.ReleaseStock executable requires one exact result stream",
        ));
    }

    Ok(())
}
