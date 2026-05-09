use andromeda_types::TransactionId;
use andromeda_wal::{Lsn, WalRecordKind};

use crate::StartupMode;

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

/// Summary report produced at the end of a single WAL replay session.
///
/// All counters are derived exclusively from the durable WAL prefix and the
/// conceptual redo plan. RAM-only state must not be reflected here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalReplayReport {
    /// Recovery startup mode that governed this session.
    pub startup_mode: StartupMode,
    /// LSN from which redo started.
    pub replay_start_lsn: Lsn,
    /// Highest LSN where a handler returned `Applied`.
    pub replay_end_lsn: Option<Lsn>,
    /// Total plan records evaluated.
    pub total_records: usize,
    /// Records whose handler returned `Applied`.
    pub applied_count: usize,
    /// Records whose handler returned `Skipped`.
    pub handler_skipped_count: usize,
    /// Records whose redo decision was anything other than `Replay`.
    pub plan_skipped_count: usize,
    /// Records whose handler returned `NotYetImplemented`.
    pub not_yet_implemented_count: usize,
    /// Index/B-Tree records requiring a rebuild before access paths are trusted.
    pub index_rebuild_required_count: usize,
    /// Structured access-path rebuild evidence emitted by this replay session.
    pub index_rebuild_required: Vec<IndexRebuildRequiredEvidence>,
    /// Incomplete transactions identified and discarded by the plan.
    pub incomplete_transaction_count: usize,
    /// Transactions whose `TxCommit` record was found in durable WAL.
    pub committed_transaction_count: usize,
    /// `true` if any handler returned `NotYetImplemented` or `Deprecated`.
    pub has_replay_errors: bool,
}

impl WalReplayReport {
    /// Returns `true` when recovery can open without discarded transactions,
    /// replay errors, or pending access-path rebuilds.
    pub fn is_clean_recovery(&self) -> bool {
        self.incomplete_transaction_count == 0
            && !self.has_replay_errors
            && !self.requires_access_path_rebuild()
    }

    /// Returns `true` when incomplete transactions were discarded.
    pub fn has_discarded_transactions(&self) -> bool {
        self.incomplete_transaction_count > 0
    }

    /// Returns `true` when one or more access paths must be rebuilt.
    pub const fn requires_access_path_rebuild(&self) -> bool {
        self.index_rebuild_required_count > 0
    }

    /// Returns structured access-path rebuild evidence emitted by this replay.
    pub fn access_path_rebuild_evidence(&self) -> &[IndexRebuildRequiredEvidence] {
        &self.index_rebuild_required
    }

    /// Returns `true` when any redo records are queued for replay by the plan.
    pub fn has_replay_work(&self) -> bool {
        self.applied_count > 0
            || self.not_yet_implemented_count > 0
            || self.index_rebuild_required_count > 0
    }
}
