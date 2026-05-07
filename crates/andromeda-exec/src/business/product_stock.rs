use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_storage::{
    HeapPageInsert, Lsn, PageId, PageSize, ProductStockHeapInsert, ProductStockRow, WalRecordKind,
    write_ahead_log::HeapRowRedoPayloadV1,
};

use crate::{LocalHeapRowInsertRedoTemplate, LocalHeapRowRedoContractBinding};

use super::executor::InventoryReserveStockExecutor;
use super::helpers::{validate_business_mvcc_timestamp, validate_business_mvcc_transaction_id};
use super::types::{InventoryStock, ReserveStockCommand, ReserveStockEffect};

/// Commit evidence required before a ProductStock adapter can publish a
/// reservation as visible state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryProductStockCommitEvidence {
    pub transaction_id: TransactionId,
    pub durable_commit_lsn: Lsn,
}

impl InventoryProductStockCommitEvidence {
    pub fn new(transaction_id: TransactionId, durable_commit_lsn: Lsn) -> AndromedaResult<Self> {
        let evidence = Self {
            transaction_id,
            durable_commit_lsn,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    pub fn validate(self) -> AndromedaResult<()> {
        validate_business_mvcc_transaction_id(
            self.transaction_id,
            "ProductStock publication requires a non-zero transaction id",
        )?;
        validate_business_mvcc_timestamp(
            self.durable_commit_lsn.get(),
            "ProductStock publication requires a non-zero durable commit LSN",
        )
    }
}

/// Durable heap redo evidence for the ProductStock physical row version.
///
/// `redo_record_lsn` is the WAL record LSN of the HREDOV1 row redo payload.
/// `durable_commit_lsn` proves the transaction commit record was flushed after
/// that redo record, so the row can be made visible by the ProductStock adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryProductStockDurableRedoEvidence {
    pub transaction_id: TransactionId,
    pub redo_record_lsn: Lsn,
    pub durable_commit_lsn: Lsn,
    pub redo_binding: Option<LocalHeapRowRedoContractBinding>,
    pub redo_payload: HeapRowRedoPayloadV1,
}

impl InventoryProductStockDurableRedoEvidence {
    pub fn new(
        transaction_id: TransactionId,
        durable_commit_lsn: Lsn,
        redo_payload: HeapRowRedoPayloadV1,
    ) -> AndromedaResult<Self> {
        Self::new_with_optional_binding(transaction_id, durable_commit_lsn, None, redo_payload)
    }

    pub fn new_with_binding(
        transaction_id: TransactionId,
        durable_commit_lsn: Lsn,
        redo_binding: LocalHeapRowRedoContractBinding,
        redo_payload: HeapRowRedoPayloadV1,
    ) -> AndromedaResult<Self> {
        Self::new_with_optional_binding(
            transaction_id,
            durable_commit_lsn,
            Some(redo_binding),
            redo_payload,
        )
    }

    fn new_with_optional_binding(
        transaction_id: TransactionId,
        durable_commit_lsn: Lsn,
        redo_binding: Option<LocalHeapRowRedoContractBinding>,
        redo_payload: HeapRowRedoPayloadV1,
    ) -> AndromedaResult<Self> {
        let evidence = Self {
            transaction_id,
            redo_record_lsn: redo_payload.resulting_page_lsn(),
            durable_commit_lsn,
            redo_binding,
            redo_payload,
        };
        evidence.validate()?;
        Ok(evidence)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        InventoryProductStockCommitEvidence::new(self.transaction_id, self.durable_commit_lsn)?;
        validate_business_mvcc_timestamp(
            self.redo_record_lsn.get(),
            "ProductStock redo evidence requires a non-zero redo record LSN",
        )?;

        if self.redo_payload.wal_record_kind() != WalRecordKind::RowInsert {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "ProductStock durable redo evidence must be a heap row insert redo record",
            ));
        }

        if self.redo_payload.resulting_page_lsn() != self.redo_record_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "ProductStock durable redo evidence LSN must match the HREDOV1 resulting page LSN",
            ));
        }

        if let Some(binding) = self.redo_binding {
            binding.validate()?;
        }

        if self.durable_commit_lsn <= self.redo_record_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "ProductStock durable commit LSN must follow the row redo record LSN",
            ));
        }

        Ok(())
    }
}

/// Prepared reservation against `Inventory.ProductStock`.
///
/// The intent is not visible by itself. A ProductStock adapter must publish it
/// only after receiving [`InventoryProductStockCommitEvidence`].
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

/// Storage-facing seam for `Inventory.ProductStock`.
///
/// Implementations must stage changes without making them visible in
/// `prepare_reserve_stock`, publish only after durable commit evidence, and
/// discard staged state on any failure before commit.
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

    fn publish_committed_reserve_stock(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        commit: InventoryProductStockCommitEvidence,
    ) -> AndromedaResult<()>;

    fn prepared_reserve_stock_redo_template(
        &self,
        intent: &InventoryProductStockReservationIntent,
    ) -> AndromedaResult<Option<LocalHeapRowInsertRedoTemplate>> {
        intent.validate()?;
        Ok(None)
    }

    fn publish_committed_reserve_stock_with_redo(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        commit: InventoryProductStockCommitEvidence,
        redo: InventoryProductStockDurableRedoEvidence,
    ) -> AndromedaResult<()> {
        commit.validate()?;
        redo.validate()?;
        if commit.transaction_id != redo.transaction_id
            || commit.durable_commit_lsn != redo.durable_commit_lsn
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                "ProductStock redo evidence must match commit evidence",
            ));
        }
        self.publish_committed_reserve_stock(intent, commit)
    }

    fn abort_prepared_reserve_stock(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        reason: &str,
    ) -> AndromedaResult<()>;
}

/// Heap-backed V0 seam for `Inventory.ProductStock`.
///
/// The constructor seeds one visible row as cold snapshot state. Runtime
/// publication remains gated by durable commit evidence and produces a HREDOV1
/// redo payload tied to the commit LSN before the new stock row becomes the
/// adapter-visible row.
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

    fn publish_committed_reserve_stock(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        commit: InventoryProductStockCommitEvidence,
    ) -> AndromedaResult<()> {
        commit.validate()?;
        intent.validate()?;

        if self.prepared.as_ref() != Some(intent) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "ProductStock heap publication requires the matching prepared reservation",
            ));
        }

        if commit.durable_commit_lsn <= self.page_lsn {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "ProductStock heap publication requires a durable commit LSN after the page LSN",
            ));
        }

        let insert = self.prepared_insert.take().ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "ProductStock heap publication requires a prepared heap redo insert",
            )
        })?;
        let redo_payload =
            insert.row_insert_redo_payload(self.page_lsn, commit.durable_commit_lsn)?;

        self.visible_slot_id = insert.slot_id();
        self.visible_version = intent.effect.next_stock.version;
        self.page_lsn = commit.durable_commit_lsn;
        self.prepared = None;
        self.published_commit = Some(commit);
        self.last_redo_payload = Some(redo_payload);
        self.last_committed_insert = Some(insert);
        Ok(())
    }

    fn prepared_reserve_stock_redo_template(
        &self,
        intent: &InventoryProductStockReservationIntent,
    ) -> AndromedaResult<Option<LocalHeapRowInsertRedoTemplate>> {
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

        Ok(Some(template))
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

        let template = self
            .prepared_reserve_stock_redo_template(intent)?
            .ok_or_else(|| {
                AndromedaError::new(
                    AndromedaErrorKind::Storage,
                    "ProductStock heap publication requires storage redo template evidence",
                )
            })?;
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

/// Explicit in-memory adapter used by focused tests and callers that need to
/// inject observed ProductStock evidence manually.
///
/// V0 default execution should prefer [`HeapInventoryProductStockStore`] so a
/// ProductStock success is bridged through the durable heap/source-of-truth
/// boundary instead of this observed-state double.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedInventoryProductStockStore {
    visible_stock: InventoryStock,
    prepared: Option<InventoryProductStockReservationIntent>,
    published_commit: Option<InventoryProductStockCommitEvidence>,
}

impl ObservedInventoryProductStockStore {
    pub fn new(visible_stock: InventoryStock) -> AndromedaResult<Self> {
        visible_stock.validate()?;
        Ok(Self {
            visible_stock,
            prepared: None,
            published_commit: None,
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

    fn publish_committed_reserve_stock(
        &mut self,
        intent: &InventoryProductStockReservationIntent,
        commit: InventoryProductStockCommitEvidence,
    ) -> AndromedaResult<()> {
        commit.validate()?;
        intent.validate()?;

        if self.prepared.as_ref() != Some(intent) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "ProductStock publication requires the matching prepared reservation",
            ));
        }

        self.visible_stock = intent.effect.next_stock;
        self.prepared = None;
        self.published_commit = Some(commit);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn stock() -> InventoryStock {
        InventoryStock {
            product_id: 42,
            available_quantity: 10,
            version: 7,
        }
    }

    fn command() -> ReserveStockCommand {
        ReserveStockCommand {
            product_id: 42,
            quantity: 3,
        }
    }

    #[test]
    fn heap_store_keeps_reservation_invisible_until_durable_commit_evidence() {
        let mut store = HeapInventoryProductStockStore::from_cold_snapshot(
            PageId::new(31_001),
            PageSize::KiB16,
            stock(),
        )
        .unwrap();

        let intent = store.prepare_reserve_stock(command()).unwrap();

        assert_eq!(store.visible_stock().unwrap(), stock());
        assert_eq!(
            store.visible_product_stock_row().unwrap(),
            ProductStockRow::new(42, 10).unwrap()
        );
        assert_eq!(store.active_heap_slot_count(), 2);
        assert_eq!(store.prepared_intent(), Some(&intent));
        let prepared_insert = store.prepared_heap_insert().unwrap();
        assert_eq!(prepared_insert.slot_id(), 1);
        assert_eq!(prepared_insert.row(), ProductStockRow::new(42, 7).unwrap());
        assert!(store.last_committed_insert().is_none());
        assert!(store.last_redo_payload().is_none());
        let redo_template = store
            .prepared_reserve_stock_redo_template(&intent)
            .unwrap()
            .unwrap();
        let wal_redo = redo_template
            .materialize_heap_redo_payload(Lsn::new(8))
            .unwrap();
        assert_eq!(wal_redo.after_slot_id(), prepared_insert.slot_id());
        assert_eq!(wal_redo.expected_previous_page_lsn(), Lsn::ZERO);
        assert_eq!(wal_redo.resulting_page_lsn(), Lsn::new(8));
        assert_eq!(
            ProductStockRow::decode(wal_redo.tuple()).unwrap(),
            prepared_insert.row()
        );

        let commit =
            InventoryProductStockCommitEvidence::new(TransactionId::new(11), Lsn::new(9)).unwrap();
        store
            .publish_committed_reserve_stock(&intent, commit)
            .unwrap();

        assert_eq!(store.visible_stock().unwrap(), intent.effect.next_stock);
        assert_eq!(
            store.visible_product_stock_row().unwrap(),
            ProductStockRow::new(42, 7).unwrap()
        );
        assert_eq!(store.active_heap_slot_count(), 2);
        assert!(store.prepared_intent().is_none());
        assert_eq!(store.published_commit(), Some(commit));
        assert_eq!(store.page_lsn(), Lsn::new(9));

        let insert = store.last_committed_insert().unwrap();
        assert_eq!(insert.slot_id(), 1);
        assert_eq!(insert.row(), ProductStockRow::new(42, 7).unwrap());

        let redo = store.last_redo_payload().unwrap();
        assert_eq!(redo.expected_previous_page_lsn(), Lsn::ZERO);
        assert_eq!(redo.resulting_page_lsn(), Lsn::new(9));
        assert_eq!(redo.after_slot_id(), insert.slot_id());
        assert_eq!(ProductStockRow::decode(redo.tuple()).unwrap(), insert.row());
    }

    #[test]
    fn heap_store_abort_discards_prepared_state_without_heap_publication() {
        let mut store = HeapInventoryProductStockStore::from_cold_snapshot(
            PageId::new(31_002),
            PageSize::KiB16,
            stock(),
        )
        .unwrap();

        let intent = store.prepare_reserve_stock(command()).unwrap();

        store
            .abort_prepared_reserve_stock(&intent, "pre-commit WAL failure")
            .unwrap();

        assert_eq!(store.visible_stock().unwrap(), stock());
        assert_eq!(
            store.visible_product_stock_row().unwrap(),
            ProductStockRow::new(42, 10).unwrap()
        );
        assert_eq!(store.active_heap_slot_count(), 1);
        assert!(store.prepared_intent().is_none());
        assert!(store.prepared_heap_insert().is_none());
        assert!(store.published_commit().is_none());
        assert!(store.last_committed_insert().is_none());
        assert!(store.last_redo_payload().is_none());
        assert_eq!(store.page_lsn(), Lsn::ZERO);
    }

    #[test]
    fn heap_store_rejects_non_advancing_durable_lsn_without_visibility_change() {
        let mut store = HeapInventoryProductStockStore::from_cold_snapshot(
            PageId::new(31_003),
            PageSize::KiB16,
            stock(),
        )
        .unwrap();

        let first = store.prepare_reserve_stock(command()).unwrap();
        let first_commit =
            InventoryProductStockCommitEvidence::new(TransactionId::new(11), Lsn::new(9)).unwrap();
        store
            .publish_committed_reserve_stock(&first, first_commit)
            .unwrap();

        let second = store
            .prepare_reserve_stock(ReserveStockCommand {
                product_id: 42,
                quantity: 2,
            })
            .unwrap();
        let err = store
            .publish_committed_reserve_stock(
                &second,
                InventoryProductStockCommitEvidence::new(TransactionId::new(12), Lsn::new(9))
                    .unwrap(),
            )
            .unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
        assert!(err.message().contains("after the page LSN"));
        assert_eq!(store.visible_stock().unwrap(), first.effect.next_stock);
        assert_eq!(store.active_heap_slot_count(), 3);
        assert_eq!(store.prepared_intent(), Some(&second));
        assert!(store.prepared_heap_insert().is_some());
        assert_eq!(store.published_commit(), Some(first_commit));
        assert_eq!(store.page_lsn(), Lsn::new(9));
    }

    #[test]
    fn observed_store_keeps_reservation_invisible_until_durable_commit_evidence() {
        let mut store = ObservedInventoryProductStockStore::new(stock()).unwrap();

        let intent = store.prepare_reserve_stock(command()).unwrap();

        assert_eq!(store.visible_stock(), stock());
        assert_eq!(store.prepared_intent(), Some(&intent));
        assert!(store.published_commit().is_none());

        let commit =
            InventoryProductStockCommitEvidence::new(TransactionId::new(11), Lsn::new(3)).unwrap();
        store
            .publish_committed_reserve_stock(&intent, commit)
            .unwrap();

        assert_eq!(store.visible_stock(), intent.effect.next_stock);
        assert!(store.prepared_intent().is_none());
        assert_eq!(store.published_commit(), Some(commit));
    }

    #[test]
    fn observed_store_rejects_publication_without_durable_lsn() {
        let mut store = ObservedInventoryProductStockStore::new(stock()).unwrap();
        let intent = store.prepare_reserve_stock(command()).unwrap();

        let err = store
            .publish_committed_reserve_stock(
                &intent,
                InventoryProductStockCommitEvidence {
                    transaction_id: TransactionId::new(11),
                    durable_commit_lsn: Lsn::ZERO,
                },
            )
            .unwrap_err();

        assert_eq!(err.kind(), AndromedaErrorKind::Transaction);
        assert_eq!(store.visible_stock(), stock());
        assert_eq!(store.prepared_intent(), Some(&intent));
    }

    #[test]
    fn observed_store_abort_discards_prepared_state_without_visibility_change() {
        let mut store = ObservedInventoryProductStockStore::new(stock()).unwrap();
        let intent = store.prepare_reserve_stock(command()).unwrap();

        store
            .abort_prepared_reserve_stock(&intent, "pre-commit dispatcher rejection")
            .unwrap();

        assert_eq!(store.visible_stock(), stock());
        assert!(store.prepared_intent().is_none());
        assert!(store.published_commit().is_none());
    }
}
