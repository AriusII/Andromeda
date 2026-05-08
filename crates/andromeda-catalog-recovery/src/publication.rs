/// Confines this contract surface to catalog Administration/HA publication.
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

/// Runtime class for administrative catalog subscribers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogSubscriberKind {
    Administration,
    HadrReplica,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationReplayTerminalOutcome {
    Committed,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogPublicationSubscriptionReplayRecordKind {
    Publication,
    SubscriptionAcknowledgement,
    Terminal,
}
