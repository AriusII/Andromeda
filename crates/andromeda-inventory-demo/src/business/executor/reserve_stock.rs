use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::super::constants::{
    INVENTORY_RESERVE_STOCK_RESERVATION_ROWS_AFFECTED, INVENTORY_RESERVE_STOCK_STOCK_ROWS_AFFECTED,
};
use super::super::types::{
    InventoryReserveStockRejectionEvidence, InventoryStock, ReservationResult, ReserveStockCommand,
    ReserveStockEffect,
};

pub struct InventoryReserveStockExecutor;

impl InventoryReserveStockExecutor {
    pub fn reserve(
        command: ReserveStockCommand,
        stock: InventoryStock,
    ) -> AndromedaResult<ReserveStockEffect> {
        command.validate()?;
        stock.validate()?;

        if command.product_id != stock.product_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "reserve stock command product must match inventory stock product",
            ));
        }

        if stock.available_quantity < command.quantity {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "insufficient inventory stock for reservation",
            ));
        }

        let next_version = stock.version.checked_add(1).ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Execution,
                "inventory stock version overflow during reservation",
            )
        })?;
        let remaining_quantity = stock.available_quantity - command.quantity;
        let next_stock = InventoryStock {
            product_id: stock.product_id,
            available_quantity: remaining_quantity,
            version: next_version,
        };
        let result = ReservationResult {
            product_id: command.product_id,
            quantity: command.quantity,
            remaining_quantity,
            reserved: true,
        };

        Ok(ReserveStockEffect {
            previous_stock: stock,
            next_stock,
            result,
            rows_affected: INVENTORY_RESERVE_STOCK_STOCK_ROWS_AFFECTED
                + INVENTORY_RESERVE_STOCK_RESERVATION_ROWS_AFFECTED,
        })
    }

    pub fn rejection_evidence(
        command: ReserveStockCommand,
        stock: InventoryStock,
        reason: impl Into<String>,
    ) -> InventoryReserveStockRejectionEvidence {
        InventoryReserveStockRejectionEvidence {
            product_id: command.product_id,
            requested_quantity: command.quantity,
            available_quantity: stock.available_quantity,
            reason: reason.into(),
        }
    }
}
