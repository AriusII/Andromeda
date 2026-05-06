use std::path::{Path, PathBuf};

use super::{
    DurableAuditCompactionReport, DurableAuditReplayQuery, DurableAuditReplayRecord,
    DurableAuditReplayResult, DurableAuditReplayWindow, DurableAuditRetentionPolicy,
    DurableAuditSinkReport, DurableAuditSinkResult, DurableAuditWalSegmentArchiveProof,
    DurableAuditWalSink, PendingDurableAuditRecord, append_record, compact_records,
    compact_records_with_archive_proofs, open_sink, replay_records, replay_records_with_evidence,
};

#[derive(Debug, Clone)]
pub struct FileDurableAuditWalSink {
    path: PathBuf,
}

impl FileDurableAuditWalSink {
    pub fn open(path: impl AsRef<Path>) -> DurableAuditSinkResult<Self> {
        Ok(Self {
            path: open_sink(path)?,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Replay is forensic evidence reconstruction only.
    pub fn replay(
        &self,
        query: &DurableAuditReplayQuery,
    ) -> DurableAuditSinkResult<Vec<DurableAuditReplayRecord>> {
        replay_records(&self.path, query)
    }

    pub fn query(
        &self,
        query: &DurableAuditReplayQuery,
    ) -> DurableAuditSinkResult<Vec<DurableAuditReplayRecord>> {
        self.replay(query)
    }

    pub fn query_with_evidence(
        &self,
        query: &DurableAuditReplayQuery,
        window: DurableAuditReplayWindow,
    ) -> DurableAuditSinkResult<DurableAuditReplayResult> {
        replay_records_with_evidence(&self.path, query, window)
    }

    pub fn compact(
        &self,
        policy: &DurableAuditRetentionPolicy,
    ) -> DurableAuditSinkResult<DurableAuditCompactionReport> {
        compact_records(&self.path, policy)
    }

    pub fn compact_with_archive_proofs(
        &self,
        policy: &DurableAuditRetentionPolicy,
        archive_proofs: &[DurableAuditWalSegmentArchiveProof],
    ) -> DurableAuditSinkResult<DurableAuditCompactionReport> {
        compact_records_with_archive_proofs(&self.path, policy, archive_proofs)
    }
}

impl DurableAuditWalSink for FileDurableAuditWalSink {
    fn append_durable_audit_record(
        &mut self,
        record: PendingDurableAuditRecord,
    ) -> DurableAuditSinkResult<DurableAuditSinkReport> {
        append_record(&self.path, record)
    }
}
