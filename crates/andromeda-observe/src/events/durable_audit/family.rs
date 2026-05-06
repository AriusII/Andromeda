#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditEventFamily {
    SecurityDecision,
    AdminDecision,
    AdmissionDecision,
    CatalogDecision,
    RecoveryDecision,
    GenericAudit,
}

impl DurableAuditEventFamily {
    pub const fn requires_wal_before_visible_decision(self) -> bool {
        matches!(
            self,
            Self::SecurityDecision | Self::AdminDecision | Self::CatalogDecision
        )
    }
}
