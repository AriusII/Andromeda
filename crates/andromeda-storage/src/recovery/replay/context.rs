use std::collections::{BTreeMap, HashMap};

use andromeda_core::TransactionId;

use crate::{DatabaseManifest, Lsn, PageId, PageSize, WalRecordKind};

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
        }
    }

    pub fn record_result(&mut self, result: ReplayResult) {
        self.last_replayed_lsn = Some(result.lsn);

        match result.outcome {
            ReplayOutcome::Applied => self.applied_count += 1,
            ReplayOutcome::Skipped | ReplayOutcome::IndexRebuildRequired => {
                self.skipped_count += 1;
            }
            ReplayOutcome::NotYetImplemented | ReplayOutcome::Deprecated => {
                self.error_records.push(result);
            }
        }
    }

    pub fn record_index_rebuild_required(&mut self, evidence: IndexRebuildRequiredEvidence) {
        self.index_rebuild_required.push(evidence);
    }

    pub fn heap_redo_page(&self, page_id: PageId) -> Option<&HeapRedoPageState> {
        self.heap_redo_pages.get(&page_id)
    }

    pub fn heap_redo_page_count(&self) -> usize {
        self.heap_redo_pages.len()
    }

    pub(super) fn heap_redo_page_mut_or_insert(
        &mut self,
        page_id: PageId,
        page_size: PageSize,
    ) -> Result<&mut HeapRedoPageState, String> {
        let page = self
            .heap_redo_pages
            .entry(page_id)
            .or_insert_with(|| HeapRedoPageState::new(page_id, page_size));
        if page.page_size() != page_size {
            return Err(format!(
                "heap row redo page {} has size {:?}, payload declares {:?}",
                page_id.get(),
                page.page_size(),
                page_size
            ));
        }
        Ok(page)
    }

    pub fn has_errors(&self) -> bool {
        !self.error_records.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRebuildRequiredEvidence {
    pub lsn: Lsn,
    pub kind: WalRecordKind,
    pub transaction_id: Option<TransactionId>,
    pub index_id: u64,
    pub key_format_major: u32,
    pub key_format_minor: u32,
    pub codec_version: u8,
    pub max_key_size: u16,
    pub payload_len: usize,
    pub payload_checksum: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestSwitchRecoveryTrace {
    ManifestSwitchApplied {
        lsn: Lsn,
        manifest_version: u64,
        snapshot_id: u64,
        base_checkpoint_lsn: Lsn,
        required_wal_start_lsn: Lsn,
    },
    ManifestSwitchValidationFailed {
        lsn: Lsn,
        manifest_version: u64,
        reason: &'static str,
    },
}

impl Default for ReplayContext {
    fn default() -> Self {
        Self::new()
    }
}
