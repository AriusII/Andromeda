use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_storage_heap::{
    HeapPageInsert, HeapRowRedoPayloadV1, LocalHeapRowInsertRedoTemplate,
    LocalHeapRowRedoContractBinding, ProductStockHeapInsert, ProductStockRow,
};
use andromeda_storage_page::{PageId, PageSize};
use andromeda_wal::Lsn;

use super::super::types::{InventoryStock, ReserveStockCommand};
use super::{
    evidence::{InventoryProductStockCommitEvidence, InventoryProductStockDurableRedoEvidence},
    intent::InventoryProductStockReservationIntent,
    store::InventoryProductStockStore,
};

/// Heap-backed V0 seam for `Inventory.ProductStock`.
///
/// The constructor seeds one visible row as cold snapshot state. Runtime
/// publication remains gated by durable commit evidence plus HREDOV1 row redo
/// evidence before the new stock row becomes the adapter-visible row.
#[derive(Debug)]
pub struct HeapInventoryProductStockStore {
    page: HeapPageInsert,
    visible_slot_id: u16,
    visible_version: u64,
    page_lsn: Lsn,
    prepared: Option<InventoryProductStockReservationIntent>,
    prepared_insert: Option<ProductStockHeapInsert>,
    published_commit: Option<InventoryProductStockCommitEvidence>,
    last_committed_insert: Option<ProductStockHeapInsert>,
    last_redo_payload: Option<HeapRowRedoPayloadV1>,
    redo_binding: Option<LocalHeapRowRedoContractBinding>,
}

impl HeapInventoryProductStockStore {
    pub fn from_cold_snapshot(
        page_id: PageId,
        page_size: PageSize,
        visible_stock: InventoryStock,
    ) -> AndromedaResult<Self> {
        visible_stock.validate()?;
        let mut page = HeapPageInsert::for_product_stock(page_id, page_size)?;
        let snapshot_row =
            ProductStockRow::new(visible_stock.product_id, visible_stock.available_quantity)?;
        let snapshot_insert = page.insert_product_stock(snapshot_row)?;

        Ok(Self {
            page,
            visible_slot_id: snapshot_insert.slot_id(),
            visible_version: visible_stock.version,
            page_lsn: Lsn::ZERO,
            prepared: None,
            prepared_insert: None,
            published_commit: None,
            last_committed_insert: None,
            last_redo_payload: None,
            redo_binding: None,
        })
    }

    pub fn with_redo_contract_binding(
        mut self,
        redo_binding: LocalHeapRowRedoContractBinding,
    ) -> AndromedaResult<Self> {
        redo_binding.validate()?;
        self.redo_binding = Some(redo_binding);
        Ok(self)
    }

    pub fn visible_stock(&self) -> AndromedaResult<InventoryStock> {
        let row = self.visible_product_stock_row()?;
        let stock = InventoryStock {
            product_id: row.product_id,
            available_quantity: row.quantity_on_hand,
            version: self.visible_version,
        };
        stock.validate()?;
        Ok(stock)
    }

    pub fn visible_product_stock_row(&self) -> AndromedaResult<ProductStockRow> {
        self.page.read_product_stock(self.visible_slot_id)
    }

    pub fn active_heap_slot_count(&self) -> usize {
        self.page.active_slot_count()
    }

    pub fn page_lsn(&self) -> Lsn {
        self.page_lsn
    }

    pub fn prepared_intent(&self) -> Option<&InventoryProductStockReservationIntent> {
        self.prepared.as_ref()
    }

    pub fn prepared_heap_insert(&self) -> Option<&ProductStockHeapInsert> {
        self.prepared_insert.as_ref()
    }

    pub fn published_commit(&self) -> Option<InventoryProductStockCommitEvidence> {
        self.published_commit
    }

    pub fn last_committed_insert(&self) -> Option<&ProductStockHeapInsert> {
        self.last_committed_insert.as_ref()
    }

    pub fn last_redo_payload(&self) -> Option<&HeapRowRedoPayloadV1> {
        self.last_redo_payload.as_ref()
    }

    pub const fn redo_contract_binding(&self) -> Option<LocalHeapRowRedoContractBinding> {
        self.redo_binding
    }
}

impl InventoryProductStockStore for HeapInventoryProductStockStore {
    fn prepare_reserve_stock(
        &mut self,
        command: ReserveStockCommand,
    ) -> AndromedaResult<InventoryProductStockReservationIntent> {
        if self.prepared.is_some() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "ProductStock heap adapter already has a prepared reservation",
            ));
        }

        let intent = InventoryProductStockReservationIntent::from_observed_stock(
            command,
            self.visible_stock()?,
        )?;
        let next_row = ProductStockRow::new(
            intent.effect.next_stock.product_id,
            intent.effect.next_stock.available_quantity,
        )?;
        let prepared_insert = self.page.insert_product_stock(next_row)?;
        self.prepared = Some(intent.clone());
        self.prepared_insert = Some(prepared_insert);
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
                "ProductStock heap redo template requires the matching prepared reservation",
            ));
        }

        let insert = self.prepared_insert.as_ref().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "ProductStock heap redo template requires a prepared heap insert",
            )
        })?;

        let template = match self.redo_binding {
            Some(binding) => LocalHeapRowInsertRedoTemplate::new_with_contract_binding(
                insert.page_id(),
                insert.page_size(),
                insert.slot_id(),
                self.page_lsn,
                binding,
                insert.tuple().to_vec(),
            )?,
            None => LocalHeapRowInsertRedoTemplate::new(
                insert.page_id(),
                insert.page_size(),
                insert.slot_id(),
                self.page_lsn,
                insert.tuple().to_vec(),
            )?,
        };

        Ok(template)
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
                "ProductStock heap publication requires the matching prepared reservation",
            ));
        }

        if commit.transaction_id != redo.transaction_id
            || commit.durable_commit_lsn != redo.durable_commit_lsn
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "ProductStock heap redo evidence must match durable commit evidence",
            ));
        }

        if redo.redo_record_lsn <= self.page_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "ProductStock heap redo record LSN must advance the page LSN",
            ));
        }

        if let Some(expected_binding) = self.redo_binding
            && redo.redo_binding != Some(expected_binding)
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "ProductStock heap redo evidence must bind the expected table object, procedure, catalog version, and contract hash",
            ));
        }

        let template = self.prepared_reserve_stock_redo_template(intent)?;
        if template.redo_binding() != redo.redo_binding {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "ProductStock heap redo evidence binding must match the prepared redo template",
            ));
        }
        let expected_payload = template.materialize_heap_redo_payload(redo.redo_record_lsn)?;
        if expected_payload != redo.redo_payload {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "ProductStock heap durable redo payload does not match prepared heap insert",
            ));
        }

        let insert = self.prepared_insert.take().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "ProductStock heap publication requires a prepared heap redo insert",
            )
        })?;

        self.visible_slot_id = insert.slot_id();
        self.visible_version = intent.effect.next_stock.version;
        self.page_lsn = redo.redo_record_lsn;
        self.prepared = None;
        self.published_commit = Some(commit);
        self.last_redo_payload = Some(redo.redo_payload);
        self.last_committed_insert = Some(insert);
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
                "ProductStock heap reservation abort requires a reason",
            ));
        }

        if self.prepared.as_ref() != Some(intent) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "ProductStock heap abort requires the matching prepared reservation",
            ));
        }

        if let Some(insert) = self.prepared_insert.take() {
            self.page.delete_tuple(insert.slot_id())?;
        }
        self.prepared = None;
        Ok(())
    }
}
