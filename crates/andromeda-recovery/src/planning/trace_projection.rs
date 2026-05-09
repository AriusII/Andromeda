use super::ConceptualRedoPlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryTraceProjection<TraceId = u128> {
    pub trace_id: TraceId,
    pub last_durable_lsn: u64,
    pub corruption_boundary_lsn: Option<u64>,
}

impl ConceptualRedoPlan {
    /// Project the plan into a trace payload without coupling recovery to the
    /// observer crate.
    pub fn trace_projection<TraceId>(&self, trace_id: TraceId) -> RecoveryTraceProjection<TraceId>
    where
        TraceId: Copy,
    {
        RecoveryTraceProjection {
            trace_id,
            last_durable_lsn: self.durable_lsn.get(),
            corruption_boundary_lsn: self.wal_scan_stop.map(|_| self.durable_lsn.get()),
        }
    }
}
