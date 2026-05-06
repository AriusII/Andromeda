use std::collections::BTreeMap;

use andromeda_core::AndromedaResult;

use crate::{
    Lsn, PageId, PageSize, WalRecord, WalRecordKind,
    write_ahead_log::{HeapRowRedoOperation, HeapRowRedoPayloadV1},
};

use super::{ReplayContext, ReplayResult};

/// Recovered heap-page state produced by versioned heap redo replay.
///
/// This is intentionally recovery-owned state. It gives the redo layer a real
/// idempotent apply target before the durable page-store bridge is promoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeapRedoPageState {
    page_id: PageId,
    page_size: PageSize,
    page_lsn: Lsn,
    slots: BTreeMap<u16, HeapRedoSlotState>,
}

impl HeapRedoPageState {
    pub fn new(page_id: PageId, page_size: PageSize) -> Self {
        Self {
            page_id,
            page_size,
            page_lsn: Lsn::ZERO,
            slots: BTreeMap::new(),
        }
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

    pub fn is_slot_deleted(&self, slot_id: u16) -> bool {
        matches!(
            self.slots.get(&slot_id),
            Some(HeapRedoSlotState::Deleted { .. })
        )
    }

    fn apply_insert(&mut self, payload: &HeapRowRedoPayloadV1) -> Result<(), String> {
        self.validate_lsn_transition(payload)?;

        let slot_id = payload.after_slot_id;
        match self.slots.get(&slot_id) {
            Some(HeapRedoSlotState::Live { tuple, .. }) if tuple == &payload.tuple => {
                self.page_lsn = self.page_lsn.max(payload.resulting_page_lsn);
                Ok(())
            }
            Some(HeapRedoSlotState::Live { .. }) => Err(format!(
                "heap row insert conflict on page {} slot {}: slot already contains different tuple bytes",
                self.page_id.get(),
                slot_id
            )),
            Some(HeapRedoSlotState::Deleted { .. }) => Err(format!(
                "heap row insert conflict on page {} slot {}: slot is already deleted",
                self.page_id.get(),
                slot_id
            )),
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
            }
        }
    }

    fn apply_delete(&mut self, payload: &HeapRowRedoPayloadV1) -> Result<(), String> {
        self.validate_lsn_transition(payload)?;

        let slot_id = payload.before_slot_id;
        match self.slots.get_mut(&slot_id) {
            Some(slot) => match slot {
                HeapRedoSlotState::Deleted { .. } => {
                    self.page_lsn = self.page_lsn.max(payload.resulting_page_lsn);
                    Ok(())
                }
                HeapRedoSlotState::Live { tuple, .. } => {
                    let previous_tuple = Some(tuple.clone());
                    *slot = HeapRedoSlotState::Deleted {
                        previous_tuple,
                        last_lsn: payload.resulting_page_lsn,
                    };
                    self.page_lsn = payload.resulting_page_lsn;
                    Ok(())
                }
            },
            None => Err(format!(
                "heap row delete references unknown page {} slot {}",
                self.page_id.get(),
                slot_id
            )),
        }
    }

    fn apply_update(&mut self, payload: &HeapRowRedoPayloadV1) -> Result<(), String> {
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
                return Err(format!(
                    "heap row update conflict on page {} slot {}: old version is deleted but new version is not present with matching bytes",
                    self.page_id.get(),
                    before_slot_id
                ));
            }
            None => {
                return Err(format!(
                    "heap row update references unknown old page {} slot {}",
                    self.page_id.get(),
                    before_slot_id
                ));
            }
        };

        match self.slots.get(&after_slot_id) {
            Some(HeapRedoSlotState::Live { tuple, .. }) if tuple == &payload.tuple => {}
            Some(HeapRedoSlotState::Live { .. }) => {
                return Err(format!(
                    "heap row update conflict on page {} new slot {}: slot already contains different tuple bytes",
                    self.page_id.get(),
                    after_slot_id
                ));
            }
            Some(HeapRedoSlotState::Deleted { .. }) => {
                return Err(format!(
                    "heap row update conflict on page {} new slot {}: slot is already deleted",
                    self.page_id.get(),
                    after_slot_id
                ));
            }
            None => {
                self.slots.insert(
                    after_slot_id,
                    HeapRedoSlotState::Live {
                        tuple: payload.tuple.clone(),
                        last_lsn: payload.resulting_page_lsn,
                    },
                );
            }
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

    fn validate_lsn_transition(&self, payload: &HeapRowRedoPayloadV1) -> Result<(), String> {
        if payload.resulting_page_lsn.is_zero() {
            return Err("heap row redo resulting page LSN must not be zero".to_string());
        }

        if self.page_lsn == payload.resulting_page_lsn {
            return Ok(());
        }
        if self.page_lsn > payload.resulting_page_lsn {
            return Err(format!(
                "heap row redo record for page {} is older than recovered page LSN: record={}, page={}",
                self.page_id.get(),
                payload.resulting_page_lsn.get(),
                self.page_lsn.get()
            ));
        }
        if payload.expected_previous_page_lsn.is_zero() {
            if !self.page_lsn.is_zero() {
                return Err(format!(
                    "heap row redo record for page {} omits expected previous page LSN but recovered page LSN is {}",
                    self.page_id.get(),
                    self.page_lsn.get()
                ));
            }
            return Ok(());
        }
        if self.page_lsn != payload.expected_previous_page_lsn {
            return Err(format!(
                "heap row redo expected previous page LSN {} but recovered page {} is at LSN {}",
                payload.expected_previous_page_lsn.get(),
                self.page_id.get(),
                self.page_lsn.get()
            ));
        }
        Ok(())
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
    let payload = match HeapRowRedoPayloadV1::decode(record.payload(), kind) {
        Ok(payload) => payload,
        Err(error) => {
            return Ok(heap_row_redo_error(
                record,
                kind,
                error.message().to_string(),
            ));
        }
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

    let page = match ctx.heap_redo_page_mut_or_insert(payload.page_id, payload.page_size) {
        Ok(page) => page,
        Err(message) => return Ok(heap_row_redo_error(record, kind, message)),
    };
    let apply_result = match payload.operation {
        HeapRowRedoOperation::Insert => page.apply_insert(&payload),
        HeapRowRedoOperation::Delete => page.apply_delete(&payload),
        HeapRowRedoOperation::Update => page.apply_update(&payload),
    };

    match apply_result {
        Ok(()) => Ok(ReplayResult::applied(record.header.lsn, kind)),
        Err(message) => Ok(ReplayResult::error(
            record.header.lsn,
            kind,
            format!("{kind:?} heap row redo failed: {message}"),
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
