use andromeda_error::AndromedaResult;
use andromeda_wal::{Lsn, WalRecord};

use super::{IndexRebuildRequiredEvidence, ReplayResult};

/// Minimal replay target state required by the generic redo driver.
///
/// Concrete storage/page/heap/index/catalog apply state stays in the owner
/// crate. The driver only observes counters and durable boundary evidence.
pub trait RecoveryReplayTarget {
    fn applied_count(&self) -> usize;
    fn skipped_count(&self) -> usize;
    fn error_records(&self) -> &[ReplayResult];
    fn index_rebuild_required(&self) -> &[IndexRebuildRequiredEvidence];
    fn observe_checkpoint_end(&mut self, lsn: Lsn);
    fn require_manifest_switch_checkpoint_evidence(&mut self);
}

/// Concrete replay adapter for owner-specific WAL handlers.
pub trait RecoveryWalReplayAdapter<Target: RecoveryReplayTarget> {
    fn replay_record(&mut self, target: &mut Target, record: &WalRecord) -> AndromedaResult<()>;

    fn is_explicit_deferred_replay_error(&self, _target: &Target, _record: &WalRecord) -> bool {
        false
    }
}
