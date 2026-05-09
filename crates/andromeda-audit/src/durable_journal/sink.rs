use super::{DurableAuditAppendRecord, DurableAuditSinkReport, DurableAuditSinkResult};

pub trait DurableAuditWalSink {
    /// Returning success proves append + flush to the durable audit WAL.
    fn append_durable_audit_record<R>(
        &mut self,
        record: R,
    ) -> DurableAuditSinkResult<DurableAuditSinkReport>
    where
        R: Into<DurableAuditAppendRecord>;
}
