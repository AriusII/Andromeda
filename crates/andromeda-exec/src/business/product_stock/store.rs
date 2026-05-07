use andromeda_core::AndromedaResult;

use crate::LocalHeapRowInsertRedoTemplate;

use super::super::types::ReserveStockCommand;
use super::{
    evidence::{InventoryProductStockCommitEvidence, InventoryProductStockDurableRedoEvidence},
    intent::InventoryProductStockReservationIntent,
};

/// Storage-facing seam for `Inventory.ProductStock`.
///
/// Implementations must stage changes without making them visible in
/// `prepare_reserve_stock`, publish only after durable commit plus row redo
/// evidence, and discard staged state on any failure before commit.
///
/// The V0 runtime uses the heap-backed adapter by default. Any future storage
/// adapter must reconstruct visible ProductStock state from cold snapshot plus
/// durable WAL and must preserve the same prepare/publish/abort contract; it
/// must not become a separate application execution path.
pub trait InventoryProductStockStore {
    fn prepare_reserve_stock(
        &mut self,
        command: ReserveStockCommand,
    ) -> AndromedaResult<InventoryProductStockReservationIntent>;

    fn prepared_reserve_stock_redo_template(
        &self,
        intent: &InventoryProductStockReservationIntent,
    ) -> AndromedaResult<LocalHeapRowInsertRedoTemplate>;

    fn publish_committed_reserve_stock_with_redo(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        commit: InventoryProductStockCommitEvidence,
        redo: InventoryProductStockDurableRedoEvidence,
    ) -> AndromedaResult<()>;

    fn abort_prepared_reserve_stock(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        reason: &str,
    ) -> AndromedaResult<()>;
}
