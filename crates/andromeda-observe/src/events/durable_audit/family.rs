#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditEventFamily {
    SecurityDecision,
    AdminDecision,
    AdmissionDecision,
    CatalogDecision,
    HadrDecision,
    BackupDecision,
    RestoreDecision,
    ForensicDecision,
    RecoveryDecision,
    GenericAudit,
}

impl DurableAuditEventFamily {
    pub const ALL: [Self; 10] = [
        Self::SecurityDecision,
        Self::AdminDecision,
        Self::AdmissionDecision,
        Self::CatalogDecision,
        Self::HadrDecision,
        Self::BackupDecision,
        Self::RestoreDecision,
        Self::ForensicDecision,
        Self::RecoveryDecision,
        Self::GenericAudit,
    ];

    pub const fn requires_wal_before_visible_decision(self) -> bool {
        matches!(
            self,
            Self::SecurityDecision
                | Self::AdminDecision
                | Self::CatalogDecision
                | Self::HadrDecision
                | Self::BackupDecision
                | Self::RestoreDecision
                | Self::ForensicDecision
        )
    }
}
