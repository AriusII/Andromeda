use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_exec::LocalHeapRowInsertRedoTemplate;

use super::super::types::{InventoryStock, ReserveStockCommand};
use super::{
    evidence::{InventoryProductStockCommitEvidence, InventoryProductStockDurableRedoEvidence},
    intent::InventoryProductStockReservationIntent,
    store::InventoryProductStockStore,
};

/// Explicit in-memory adapter used by focused tests and callers that need to
/// inject observed ProductStock evidence manually.
///
/// V0 default execution should prefer [`super::heap_store::HeapInventoryProductStockStore`]
/// so a ProductStock success is bridged through the durable heap/source-of-truth
/// boundary instead of this observed-state double.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedInventoryProductStockStore {
    visible_stock: InventoryStock,
    prepared: Option<InventoryProductStockReservationIntent>,
    published_commit: Option<InventoryProductStockCommitEvidence>,
    published_redo: Option<InventoryProductStockDurableRedoEvidence>,
}

impl ObservedInventoryProductStockStore {
    pub fn new(visible_stock: InventoryStock) -> AndromedaResult<Self> {
        visible_stock.validate()?;
        Ok(Self {
            visible_stock,
            prepared: None,
            published_commit: None,
            published_redo: None,
        })
    }

    pub fn visible_stock(&self) -> InventoryStock {
        self.visible_stock
    }

    pub fn prepared_intent(&self) -> Option<&InventoryProductStockReservationIntent> {
        self.prepared.as_ref()
    }

    pub fn published_commit(&self) -> Option<InventoryProductStockCommitEvidence> {
        self.published_commit
    }

    pub fn published_redo(&self) -> Option<&InventoryProductStockDurableRedoEvidence> {
        self.published_redo.as_ref()
    }
}

impl InventoryProductStockStore for ObservedInventoryProductStockStore {
    fn prepare_reserve_stock(
        &mut self,
        command: ReserveStockCommand,
    ) -> AndromedaResult<InventoryProductStockReservationIntent> {
        if self.prepared.is_some() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "ProductStock adapter already has a prepared reservation",
            ));
        }

        let intent = InventoryProductStockReservationIntent::from_observed_stock(
            command,
            self.visible_stock,
        )?;
        self.prepared = Some(intent.clone());
        Ok(intent)
    }

    fn prepared_reserve_stock_redo_template(
        &self,
        intent: &InventoryProductStockReservationIntent,
    ) -> AndromedaResult<LocalHeapRowInsertRedoTemplate> {
        intent.validate()?;
        if self.prepared.as_ref() != Some(intent) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "ProductStock redo template requires the matching prepared reservation",
            ));
        }

        Err(AndromedaError::new(
            AndromedaErrorKind::Storage,
            "observed ProductStock adapter does not materialize heap row redo templates; use heap-backed ProductStock store for durable row publication",
        ))
    }

    fn publish_committed_reserve_stock_with_redo(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        commit: InventoryProductStockCommitEvidence,
        redo: InventoryProductStockDurableRedoEvidence,
    ) -> AndromedaResult<()> {
        commit.validate()?;
        redo.validate()?;
        intent.validate()?;

        if self.prepared.as_ref() != Some(intent) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "ProductStock publication requires the matching prepared reservation",
            ));
        }

        if commit.transaction_id != redo.transaction_id
            || commit.durable_commit_lsn != redo.durable_commit_lsn
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "ProductStock redo evidence must match durable commit evidence",
            ));
        }

        self.visible_stock = intent.effect.next_stock;
        self.prepared = None;
        self.published_commit = Some(commit);
        self.published_redo = Some(redo);
        Ok(())
    }

    fn abort_prepared_reserve_stock(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        reason: &str,
    ) -> AndromedaResult<()> {
        if reason.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "ProductStock reservation abort requires a reason",
            ));
        }

        if self.prepared.as_ref() != Some(intent) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "ProductStock abort requires the matching prepared reservation",
            ));
        }

        self.prepared = None;
        Ok(())
    }
}
