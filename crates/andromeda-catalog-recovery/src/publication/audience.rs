#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationAudience {
    AdministrationHaOnly,
}

/// Categorical reason for a catalog publication trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationReasonCode {
    DefinitionBatchCommitted,
    RecoveryReplayRestored,
    HadrCatchupReplay,
}
