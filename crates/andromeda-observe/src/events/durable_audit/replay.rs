use std::path::Path;

use super::{
    DurableAuditReplayQuery, DurableAuditReplayRecord, DurableAuditReplayResult,
    DurableAuditReplayWindow, DurableAuditSinkResult,
    journal_format::{replay_durable_audit_journal, replay_durable_audit_journal_with_evidence},
};

pub(crate) fn replay_records(
    path: &Path,
    query: &DurableAuditReplayQuery,
) -> DurableAuditSinkResult<Vec<DurableAuditReplayRecord>> {
    replay_durable_audit_journal(path, query)
}

pub(crate) fn replay_records_with_evidence(
    path: &Path,
    query: &DurableAuditReplayQuery,
    window: DurableAuditReplayWindow,
) -> DurableAuditSinkResult<DurableAuditReplayResult> {
    replay_durable_audit_journal_with_evidence(path, query, window)
}
