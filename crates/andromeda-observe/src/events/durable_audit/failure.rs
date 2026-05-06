#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditFailureKind {
    ValidationRejected,
    WalAppendRejected,
    WalFlushRejected,
    CorruptionDetected,
    PermissionDenied,
    RetentionRejected,
}

impl DurableAuditFailureKind {
    /// Durable-audit failures are fail-closed by doctrine.
    pub const fn requires_fail_closed(self) -> bool {
        match self {
            Self::ValidationRejected
            | Self::WalAppendRejected
            | Self::WalFlushRejected
            | Self::CorruptionDetected
            | Self::PermissionDenied
            | Self::RetentionRejected => true,
        }
    }
}
