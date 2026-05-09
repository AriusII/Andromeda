use std::collections::BTreeMap;

use andromeda_error::AndromedaResult;
use andromeda_storage_heap::{HeapRowRedoOperation, HeapRowRedoPayloadV1, ProductStockRow};
use andromeda_storage_page::{PageId, PageSize};
use andromeda_wal::{Lsn, WalRecord, WalRecordKind};

use super::{ReplayContext, ReplayResult, recovery_error};

/// Recovered heap-page state produced by versioned heap redo replay.
///
/// This is intentionally recovery-owned state. It gives the redo layer a real
/// idempotent apply target before the durable page-store bridge is promoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeapRedoPageState {
    page_id: PageId,
    page_size: PageSize,
    page_lsn: Lsn,
    origin: HeapRedoPageOrigin,
    slots: BTreeMap<u16, HeapRedoSlotState>,
}

impl HeapRedoPageState {
    pub fn redo_created(page_id: PageId, page_size: PageSize) -> Self {
        Self {
            page_id,
            page_size,
            page_lsn: Lsn::ZERO,
            origin: HeapRedoPageOrigin::RedoCreated,
            slots: BTreeMap::new(),
        }
    }

    pub fn snapshot_hydrated(
        page_id: PageId,
        page_size: PageSize,
        page_lsn: Lsn,
        live_slots: impl IntoIterator<Item = (u16, Vec<u8>)>,
    ) -> AndromedaResult<Self> {
        if page_lsn.is_zero() {
            return Err(recovery_error(format!(
                "snapshot-hydrated heap page {} must carry a nonzero durable page LSN",
                page_id.get()
            )));
        }

        let mut slots = BTreeMap::new();
        for (slot_id, tuple) in live_slots {
            if tuple.is_empty() {
                return Err(recovery_error(format!(
                    "snapshot-hydrated heap page {} slot {} must carry durable tuple bytes",
                    page_id.get(),
                    slot_id
                )));
            }
            if slots
                .insert(
                    slot_id,
                    HeapRedoSlotState::Live {
                        tuple,
                        last_lsn: page_lsn,
                    },
                )
                .is_some()
            {
                return Err(recovery_error(format!(
                    "snapshot-hydrated heap page {} declares duplicate slot {}",
                    page_id.get(),
                    slot_id
                )));
            }
        }

        Ok(Self {
            page_id,
            page_size,
            page_lsn,
            origin: HeapRedoPageOrigin::SnapshotHydrated,
            slots,
        })
    }

    pub const fn page_id(&self) -> PageId {
        self.page_id
    }

    pub const fn page_size(&self) -> PageSize {
        self.page_size
    }

    pub const fn page_lsn(&self) -> Lsn {
        self.page_lsn
    }

    pub const fn origin_label(&self) -> &'static str {
        self.origin.label()
    }

    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }

    pub fn live_slot_count(&self) -> usize {
        self.slots
            .values()
            .filter(|slot| matches!(slot, HeapRedoSlotState::Live { .. }))
            .count()
    }

    pub fn read_tuple(&self, slot_id: u16) -> Option<&[u8]> {
        match self.slots.get(&slot_id) {
            Some(HeapRedoSlotState::Live { tuple, .. }) => Some(tuple),
            _ => None,
        }
    }

    pub fn read_tuple_with_lsn(&self, slot_id: u16) -> Option<(&[u8], Lsn)> {
        match self.slots.get(&slot_id) {
            Some(HeapRedoSlotState::Live { tuple, last_lsn }) => Some((tuple, *last_lsn)),
            _ => None,
        }
    }

    pub fn read_product_stock(&self, slot_id: u16) -> AndromedaResult<Option<ProductStockRow>> {
        self.read_tuple(slot_id)
            .map(ProductStockRow::decode)
            .transpose()
    }

    pub fn read_product_stock_recovery_row(
        &self,
        slot_id: u16,
    ) -> AndromedaResult<Option<(u16, Lsn, ProductStockRow)>> {
        self.read_tuple_with_lsn(slot_id)
            .map(|(tuple, source_lsn)| {
                ProductStockRow::decode(tuple).map(|row| (slot_id, source_lsn, row))
            })
            .transpose()
    }

    pub fn decode_product_stock_rows(&self) -> AndromedaResult<Vec<(u16, ProductStockRow)>> {
        Ok(self
            .decode_product_stock_recovery_rows()?
            .into_iter()
            .map(|(slot_id, _source_lsn, row)| (slot_id, row))
            .collect())
    }

    pub fn decode_product_stock_recovery_rows(
        &self,
    ) -> AndromedaResult<Vec<(u16, Lsn, ProductStockRow)>> {
        let mut rows = Vec::new();
        for (slot_id, slot) in &self.slots {
            if let HeapRedoSlotState::Live { tuple, last_lsn } = slot {
                rows.push((*slot_id, *last_lsn, ProductStockRow::decode(tuple)?));
            }
        }
        Ok(rows)
    }

    pub fn is_slot_deleted(&self, slot_id: u16) -> bool {
        matches!(
            self.slots.get(&slot_id),
            Some(HeapRedoSlotState::Deleted { .. })
        )
    }

    fn apply_insert(&mut self, payload: &HeapRowRedoPayloadV1) -> AndromedaResult<()> {
        self.validate_lsn_transition(payload)?;

        let slot_id = payload.after_slot_id;
        match self.slots.get(&slot_id) {
            Some(HeapRedoSlotState::Live { tuple, .. }) if tuple == &payload.tuple => {
                self.page_lsn = self.page_lsn.max(payload.resulting_page_lsn);
                Ok(())
            },
            Some(HeapRedoSlotState::Live { .. }) => Err(recovery_error(format!(
                "heap row insert conflict on page {} slot {}: slot already contains different tuple bytes",
                self.page_id.get(),
                slot_id
            ))),
            Some(HeapRedoSlotState::Deleted { .. }) => Err(recovery_error(format!(
                "heap row insert conflict on page {} slot {}: slot is already deleted",
                self.page_id.get(),
                slot_id
            ))),
            None => {
                self.slots.insert(
                    slot_id,
                    HeapRedoSlotState::Live {
                        tuple: payload.tuple.clone(),
                        last_lsn: payload.resulting_page_lsn,
                    },
                );
                self.page_lsn = payload.resulting_page_lsn;
                Ok(())
            },
        }
    }

    fn apply_delete(&mut self, payload: &HeapRowRedoPayloadV1) -> AndromedaResult<()> {
        self.validate_lsn_transition(payload)?;

        let slot_id = payload.before_slot_id;
        match self.slots.get_mut(&slot_id) {
            Some(slot) => match slot {
                HeapRedoSlotState::Deleted { .. } => {
                    self.page_lsn = self.page_lsn.max(payload.resulting_page_lsn);
                    Ok(())
                },
                HeapRedoSlotState::Live { tuple, .. } => {
                    let previous_tuple = Some(tuple.clone());
                    *slot = HeapRedoSlotState::Deleted {
                        previous_tuple,
                        last_lsn: payload.resulting_page_lsn,
                    };
                    self.page_lsn = payload.resulting_page_lsn;
                    Ok(())
                },
            },
            None => Err(recovery_error(format!(
                "heap row delete references unknown page {} slot {}",
                self.page_id.get(),
                slot_id
            ))),
        }
    }

    fn apply_update(&mut self, payload: &HeapRowRedoPayloadV1) -> AndromedaResult<()> {
        self.validate_lsn_transition(payload)?;

        let before_slot_id = payload.before_slot_id;
        let after_slot_id = payload.after_slot_id;

        if matches!(
            (
                self.slots.get(&before_slot_id),
                self.slots.get(&after_slot_id)
            ),
            (
                Some(HeapRedoSlotState::Deleted { .. }),
                Some(HeapRedoSlotState::Live { tuple, .. })
            ) if tuple == &payload.tuple
        ) {
            self.page_lsn = self.page_lsn.max(payload.resulting_page_lsn);
            return Ok(());
        }

        let previous_tuple = match self.slots.get(&before_slot_id) {
            Some(HeapRedoSlotState::Live { tuple, .. }) => tuple.clone(),
            Some(HeapRedoSlotState::Deleted { .. }) => {
                return Err(recovery_error(format!(
                    "heap row update conflict on page {} slot {}: old version is deleted but new version is not present with matching bytes",
                    self.page_id.get(),
                    before_slot_id
                )));
            },
            None => {
                return Err(recovery_error(format!(
                    "heap row update references unknown old page {} slot {}",
                    self.page_id.get(),
                    before_slot_id
                )));
            },
        };

        match self.slots.get(&after_slot_id) {
            Some(HeapRedoSlotState::Live { tuple, .. }) if tuple == &payload.tuple => {},
            Some(HeapRedoSlotState::Live { .. }) => {
                return Err(recovery_error(format!(
                    "heap row update conflict on page {} new slot {}: slot already contains different tuple bytes",
                    self.page_id.get(),
                    after_slot_id
                )));
            },
            Some(HeapRedoSlotState::Deleted { .. }) => {
                return Err(recovery_error(format!(
                    "heap row update conflict on page {} new slot {}: slot is already deleted",
                    self.page_id.get(),
                    after_slot_id
                )));
            },
            None => {
                self.slots.insert(
                    after_slot_id,
                    HeapRedoSlotState::Live {
                        tuple: payload.tuple.clone(),
                        last_lsn: payload.resulting_page_lsn,
                    },
                );
            },
        }

        self.slots.insert(
            before_slot_id,
            HeapRedoSlotState::Deleted {
                previous_tuple: Some(previous_tuple),
                last_lsn: payload.resulting_page_lsn,
            },
        );
        self.page_lsn = payload.resulting_page_lsn;
        Ok(())
    }

    fn validate_lsn_transition(&self, payload: &HeapRowRedoPayloadV1) -> AndromedaResult<()> {
        if payload.resulting_page_lsn.is_zero() {
            return Err(recovery_error(
                "heap row redo resulting page LSN must not be zero",
            ));
        }

        if self.page_lsn == payload.resulting_page_lsn {
            return Ok(());
        }
        if self.page_lsn > payload.resulting_page_lsn {
            return Err(recovery_error(format!(
                "heap row redo record for page {} is older than recovered page LSN: record={}, page={}",
                self.page_id.get(),
                payload.resulting_page_lsn.get(),
                self.page_lsn.get()
            )));
        }
        if payload.expected_previous_page_lsn.is_zero() {
            if !self.page_lsn.is_zero() {
                return Err(recovery_error(format!(
                    "heap row redo record for page {} omits expected previous page LSN but recovered page LSN is {}",
                    self.page_id.get(),
                    self.page_lsn.get()
                )));
            }
            return Ok(());
        }
        if self.page_lsn != payload.expected_previous_page_lsn {
            return Err(recovery_error(format!(
                "heap row redo expected previous page LSN {} but recovered page {} is at LSN {}",
                payload.expected_previous_page_lsn.get(),
                self.page_id.get(),
                self.page_lsn.get()
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeapRedoPageOrigin {
    /// The page was first materialized by a promoted heap redo insert record.
    RedoCreated,
    /// The page was supplied by cold snapshot hydration before WAL replay.
    SnapshotHydrated,
}

impl HeapRedoPageOrigin {
    const fn label(self) -> &'static str {
        match self {
            Self::RedoCreated => "redo-created",
            Self::SnapshotHydrated => "snapshot-hydrated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeapRedoSlotState {
    Live {
        tuple: Vec<u8>,
        last_lsn: Lsn,
    },
    Deleted {
        previous_tuple: Option<Vec<u8>>,
        last_lsn: Lsn,
    },
}

pub(super) fn replay_heap_row_record(
    ctx: &mut ReplayContext,
    record: &WalRecord,
    kind: WalRecordKind,
) -> AndromedaResult<ReplayResult> {
    if let Err(error) = record.validate() {
        return Ok(heap_row_redo_error(
            record,
            kind,
            format!("WAL record validation failed: {}", error.message()),
        ));
    }

    let payload = match HeapRowRedoPayloadV1::decode(record.payload(), kind) {
        Ok(payload) => payload,
        Err(error) => {
            return Ok(heap_row_redo_error(
                record,
                kind,
                error.message().to_string(),
            ));
        },
    };

    if payload.resulting_page_lsn != record.header.lsn {
        return Ok(heap_row_redo_error(
            record,
            kind,
            format!(
                "resulting_page_lsn {} must match WAL record LSN {}",
                payload.resulting_page_lsn.get(),
                record.header.lsn.get()
            ),
        ));
    }

    if !matches!(payload.operation, HeapRowRedoOperation::Insert)
        && ctx.heap_redo_page(payload.page_id).is_none()
    {
        return Ok(heap_row_redo_error(
            record,
            kind,
            format!(
                "{:?} requires an explicit heap page base state for page {}; \
                 the base must be snapshot-hydrated from cold system truth or redo-created by an earlier durable insert",
                payload.operation,
                payload.page_id.get()
            ),
        ));
    }

    let page = match ctx.heap_redo_page_mut_or_insert(payload.page_id, payload.page_size) {
        Ok(page) => page,
        Err(error) => return Ok(heap_row_redo_error(record, kind, error.message())),
    };
    let apply_result = match payload.operation {
        HeapRowRedoOperation::Insert => page.apply_insert(&payload),
        HeapRowRedoOperation::Delete => page.apply_delete(&payload),
        HeapRowRedoOperation::Update => page.apply_update(&payload),
    };

    match apply_result {
        Ok(()) => Ok(ReplayResult::applied(record.header.lsn, kind)),
        Err(error) => Ok(ReplayResult::error(
            record.header.lsn,
            kind,
            format!("{kind:?} heap row redo failed: {}", error.message()),
        )),
    }
}

fn heap_row_redo_error(
    record: &WalRecord,
    kind: WalRecordKind,
    message: impl Into<String>,
) -> ReplayResult {
    ReplayResult::error(
        record.header.lsn,
        kind,
        format!(
            "{kind:?} recovery handler is promoted only for durable HREDOV1 heap row redo payloads; \
             unversioned or malformed heap WAL payloads are not promoted and recovery must fail closed: {}",
            message.into()
        ),
    )
}
