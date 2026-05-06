use std::path::{Path, PathBuf};

use super::{
    DurableAuditReplayQuery, DurableAuditReplayRecord, DurableAuditSinkReport,
    DurableAuditSinkResult, DurableAuditWalSink, PendingDurableAuditRecord, append_record,
    open_sink, replay_records,
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
}

impl DurableAuditWalSink for FileDurableAuditWalSink {
    fn append_durable_audit_record(
        &mut self,
        record: PendingDurableAuditRecord,
    ) -> DurableAuditSinkResult<DurableAuditSinkReport> {
        append_record(&self.path, record)
    }
}
