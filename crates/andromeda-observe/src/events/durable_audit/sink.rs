use super::{DurableAuditSinkReport, DurableAuditSinkResult, PendingDurableAuditRecord};

pub trait DurableAuditWalSink {
    /// Returning success proves append + flush to the durable audit WAL.
    fn append_durable_audit_record(
        &mut self,
        record: PendingDurableAuditRecord,
    ) -> DurableAuditSinkResult<DurableAuditSinkReport>;
}
