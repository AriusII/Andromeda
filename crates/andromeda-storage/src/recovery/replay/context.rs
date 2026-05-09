use std::collections::{BTreeMap, HashMap};

use andromeda_core::AndromedaResult;
use andromeda_recovery::{
    IndexRebuildRequiredEvidence, ManifestSwitchRecoveryTrace, RecoveryReplayTarget,
};

use crate::{DatabaseManifest, Lsn, PageId, PageSize};

use super::super::storage_error;
use super::heap_redo::HeapRedoPageState;
use super::result::{ReplayOutcome, ReplayResult};

/// Replay context for database recovery.
///
/// This context is passed to all replay handlers and carries the state needed
/// to apply WAL records to the database.
pub struct ReplayContext {
    // TECH-DEBT(recovery-replay-context): Replay state is intentionally minimal until
    // heap/index/catalog/MVCC redo handlers stop fail-stopping.
    /// Highest LSN replayed so far in this recovery session.
    pub last_replayed_lsn: Option<Lsn>,

    /// Count of successfully applied records.
    pub applied_count: usize,

    /// Count of skipped records.
    pub skipped_count: usize,

    /// Records that failed to apply.
    pub error_records: Vec<ReplayResult>,

    /// Index/B-Tree WAL records that were validated but cannot be replayed
    /// inline until the durable B-Tree format is promoted.
    pub index_rebuild_required: Vec<IndexRebuildRequiredEvidence>,

    /// Heap pages reconstructed by promoted HREDOV1 row redo records.
    ///
    /// This is still a recovery-local apply target; durable page-store writeback
    /// remains a separate promotion boundary.
    heap_redo_pages: BTreeMap<PageId, HeapRedoPageState>,

    /// Active manifest anchor after replay.
    pub active_manifest: Option<DatabaseManifest>,

    /// CRC catalog for persisted manifests keyed by manifest version.
    pub known_manifest_crc_by_version: HashMap<u64, u32>,

    /// Observable manifest-switch trace events from replay.
    pub manifest_switch_traces: Vec<ManifestSwitchRecoveryTrace>,

    /// Highest durable `CheckpointEnd` marker observed during this recovery pass.
    pub latest_checkpoint_end_lsn: Option<Lsn>,

    /// Whether manifest switch replay must prove a durable checkpoint marker.
    pub require_checkpoint_end_for_manifest_switch: bool,
}

impl ReplayContext {
    pub fn new() -> Self {
        Self {
            last_replayed_lsn: None,
            applied_count: 0,
            skipped_count: 0,
            error_records: Vec::new(),
            index_rebuild_required: Vec::new(),
            heap_redo_pages: BTreeMap::new(),
            active_manifest: None,
            known_manifest_crc_by_version: HashMap::new(),
            manifest_switch_traces: Vec::new(),
            latest_checkpoint_end_lsn: None,
            require_checkpoint_end_for_manifest_switch: false,
        }
    }

    pub fn record_result(&mut self, result: ReplayResult) {
        self.last_replayed_lsn = Some(result.lsn);

        match result.outcome {
            ReplayOutcome::Applied => self.applied_count += 1,
            ReplayOutcome::Skipped | ReplayOutcome::IndexRebuildRequired => {
                self.skipped_count += 1;
            },
            ReplayOutcome::NotYetImplemented | ReplayOutcome::Deprecated => {
                self.error_records.push(result);
            },
        }
    }

    pub fn record_index_rebuild_required(&mut self, evidence: IndexRebuildRequiredEvidence) {
        self.index_rebuild_required.push(evidence);
    }

    pub fn observe_checkpoint_end(&mut self, lsn: Lsn) {
        if self
            .latest_checkpoint_end_lsn
            .is_none_or(|current| lsn > current)
        {
            self.latest_checkpoint_end_lsn = Some(lsn);
        }
    }

    pub fn require_manifest_switch_checkpoint_evidence(&mut self) {
        self.require_checkpoint_end_for_manifest_switch = true;
    }

    pub fn heap_redo_page(&self, page_id: PageId) -> Option<&HeapRedoPageState> {
        self.heap_redo_pages.get(&page_id)
    }

    pub fn heap_redo_page_count(&self) -> usize {
        self.heap_redo_pages.len()
    }

    pub fn heap_redo_page_origin(&self, page_id: PageId) -> Option<&'static str> {
        self.heap_redo_pages
            .get(&page_id)
            .map(HeapRedoPageState::origin_label)
    }

    pub fn hydrate_heap_redo_page_from_snapshot(
        &mut self,
        page_id: PageId,
        page_size: PageSize,
        page_lsn: Lsn,
        live_slots: impl IntoIterator<Item = (u16, Vec<u8>)>,
    ) -> AndromedaResult<()> {
        if self.heap_redo_pages.contains_key(&page_id) {
            return Err(storage_error(format!(
                "heap redo page {} already has replay state; snapshot hydration must happen before WAL replay touches the page",
                page_id.get()
            )));
        }
        let page = HeapRedoPageState::snapshot_hydrated(page_id, page_size, page_lsn, live_slots)?;
        self.heap_redo_pages.insert(page_id, page);
        Ok(())
    }

    pub(super) fn heap_redo_page_mut_or_insert(
        &mut self,
        page_id: PageId,
        page_size: PageSize,
    ) -> AndromedaResult<&mut HeapRedoPageState> {
        let page = self
            .heap_redo_pages
            .entry(page_id)
            .or_insert_with(|| HeapRedoPageState::redo_created(page_id, page_size));
        if page.page_size() != page_size {
            return Err(storage_error(format!(
                "heap row redo page {} has size {:?}, payload declares {:?}",
                page_id.get(),
                page.page_size(),
                page_size
            )));
        }
        Ok(page)
    }

    pub fn has_errors(&self) -> bool {
        !self.error_records.is_empty()
    }
}

impl Default for ReplayContext {
    fn default() -> Self {
        Self::new()
    }
}

impl RecoveryReplayTarget for ReplayContext {
    fn applied_count(&self) -> usize {
        self.applied_count
    }

    fn skipped_count(&self) -> usize {
        self.skipped_count
    }

    fn error_records(&self) -> &[ReplayResult] {
        &self.error_records
    }

    fn index_rebuild_required(&self) -> &[IndexRebuildRequiredEvidence] {
        &self.index_rebuild_required
    }

    fn observe_checkpoint_end(&mut self, lsn: Lsn) {
        Self::observe_checkpoint_end(self, lsn);
    }

    fn require_manifest_switch_checkpoint_evidence(&mut self) {
        Self::require_manifest_switch_checkpoint_evidence(self);
    }
}
