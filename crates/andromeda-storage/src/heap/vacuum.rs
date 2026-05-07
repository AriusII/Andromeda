use super::{HEAP_PAGE_V1_PAYLOAD_OFFSET, HeapPage};

/// Heap physical vacuum mode.
///
/// This is a planning contract only; callers still need MVCC and WAL safety.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapVacuumMode {
    /// Preserve `(page_id, slot_id)` stability while compacting payload bytes.
    OnlineStableRowIds,
    /// Offline rewrite may remap RowIds under an external remap protocol.
    OfflineRewriteAllowRowIdRemap,
}

/// Read-only physical vacuum plan.
///
/// MVCC visibility proof is external to HeapPageV1.
/// WAL redo is required before durable physical rewrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeapVacuumPlan {
    pub mode: HeapVacuumMode,
    pub candidate_deleted_slots: Vec<u16>,
    pub live_slots_rewritten: Vec<u16>,
    pub bytes_reclaimable: u32,
    pub preserves_row_ids: bool,
    pub row_id_remap_allowed: bool,
    pub requires_mvcc_gc_proof: bool,
    pub requires_wal_redo: bool,
    pub durable_format_change: bool,
}

impl HeapVacuumPlan {
    pub fn is_empty(&self) -> bool {
        self.candidate_deleted_slots.is_empty()
            && self.live_slots_rewritten.is_empty()
            && self.bytes_reclaimable == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeapVacuumReport {
    pub mode: HeapVacuumMode,
    pub applied: bool,
    pub slots_deleted_considered: usize,
    pub live_slots_rewritten: usize,
    pub bytes_reclaimed: u32,
    pub row_ids_preserved: bool,
    pub wal_record_lsn_required: bool,
}

impl HeapPage {
    pub fn vacuum_plan(&self, mode: HeapVacuumMode) -> HeapVacuumPlan {
        let mut candidate_deleted_slots = Vec::new();
        let mut live_slots_rewritten = Vec::new();
        let mut bytes_reclaimable = 0u32;
        let mut next_compacted_offset = HEAP_PAGE_V1_PAYLOAD_OFFSET as u16;

        for (slot_id, entry) in self.slot_directory.iter().enumerate() {
            if entry.is_deleted() {
                candidate_deleted_slots.push(slot_id as u16);
                bytes_reclaimable = bytes_reclaimable.saturating_add(u32::from(entry.length));
                continue;
            }

            if entry.offset != next_compacted_offset {
                live_slots_rewritten.push(slot_id as u16);
            }
            next_compacted_offset = next_compacted_offset.saturating_add(entry.length);
        }

        let has_work = !candidate_deleted_slots.is_empty() || !live_slots_rewritten.is_empty();
        let preserves_row_ids = matches!(mode, HeapVacuumMode::OnlineStableRowIds);

        HeapVacuumPlan {
            mode,
            candidate_deleted_slots,
            live_slots_rewritten,
            bytes_reclaimable,
            preserves_row_ids,
            row_id_remap_allowed: matches!(mode, HeapVacuumMode::OfflineRewriteAllowRowIdRemap),
            requires_mvcc_gc_proof: has_work,
            requires_wal_redo: has_work,
            durable_format_change: false,
        }
    }
}
