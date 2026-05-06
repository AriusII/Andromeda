#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurableAuditWalEvidence {
    pub record_lsn: u64,
    pub durable_lsn: u64,
    pub checksum: u64,
}

impl DurableAuditWalEvidence {
    /// Evidence is accepted only after append + flush.
    pub const fn proves_durable(self) -> bool {
        self.record_lsn != 0 && self.durable_lsn >= self.record_lsn && self.checksum != 0
    }
}
