#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DurableAuditRetentionBoundary {
    WalSegment,
    CatalogVersion,
    SecurityPolicy,
    ForensicHold,
}
