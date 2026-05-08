use crate::{EventEnvelope, TraceEvent};

use super::TraceQueryLsnRange;

pub(super) fn matches_lsn_range(envelope: &EventEnvelope, range: TraceQueryLsnRange) -> bool {
    if envelope
        .correlation
        .durable_lsn
        .is_some_and(|lsn| range.contains(lsn))
    {
        return true;
    }

    match &envelope.event {
        TraceEvent::Wal(trace) => range.contains(trace.durable_lsn),
        TraceEvent::WalEvent(trace) => {
            range.contains(trace.appended_lsn) || matches_optional_lsn(trace.durable_lsn, range)
        }
        TraceEvent::CommitVisible(trace) => range.contains(trace.durable_commit_lsn),
        TraceEvent::RollbackDurable(trace) => range.contains(trace.durable_rollback_lsn),
        TraceEvent::RecoveryStartup(trace) => {
            range.contains(trace.last_durable_lsn)
                || matches_optional_lsn(trace.corruption_boundary_lsn, range)
        }
        TraceEvent::Manifest(trace) => {
            range.contains(trace.base_checkpoint_lsn)
                || range.contains(trace.required_wal_start_lsn)
        }
        TraceEvent::CompletionEmitted(trace) => matches_optional_lsn(trace.durable_lsn, range),
        TraceEvent::CorruptionBoundary(trace) => range.contains(trace.boundary_lsn),
        _ => false,
    }
}

fn matches_optional_lsn(lsn: Option<u64>, range: TraceQueryLsnRange) -> bool {
    lsn.is_some_and(|lsn| range.contains(lsn))
}
