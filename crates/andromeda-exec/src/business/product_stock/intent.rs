use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::super::{
    executor::InventoryReserveStockExecutor,
    types::{InventoryStock, ReserveStockCommand, ReserveStockEffect},
};

/// Prepared reservation against `Inventory.ProductStock`.
///
/// The intent is not visible by itself. A ProductStock adapter must publish it
/// only after receiving durable commit plus row redo evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryProductStockReservationIntent {
    pub command: ReserveStockCommand,
    pub observed_stock: InventoryStock,
    pub effect: ReserveStockEffect,
}

impl InventoryProductStockReservationIntent {
    pub fn from_observed_stock(
        command: ReserveStockCommand,
        observed_stock: InventoryStock,
    ) -> AndromedaResult<Self> {
        let effect = InventoryReserveStockExecutor::reserve(command, observed_stock)?;
        let intent = Self {
            command,
            observed_stock,
            effect,
        };
        intent.validate()?;
        Ok(intent)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.command.validate()?;
        self.observed_stock.validate()?;

        if self.command.product_id != self.observed_stock.product_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "ProductStock reservation command must target the observed stock product",
            ));
        }

        if self.effect.previous_stock != self.observed_stock {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "ProductStock reservation effect must preserve observed stock evidence",
            ));
        }

        if self.effect.result.product_id != self.command.product_id
            || self.effect.result.quantity != self.command.quantity
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "ProductStock reservation result must match the requested product and quantity",
            ));
        }

        if !self
            .effect
            .result_evidence()
            .proves_exact_result_and_remaining_stock()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "ProductStock reservation effect does not prove exact result evidence",
            ));
        }

        Ok(())
    }
}
