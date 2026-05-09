use crate::TraceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriticalDecisionKind {
    ContractValidation,
    AuthorizationDenial,
    PlanSelection,
    WalAppend,
    TransactionCommit,
    WalFlush,
    CommitVisible,
    RollbackDurable,
    MvccVisibility,
    RecoveryStartup,
    ManifestValidation,
    ManifestSwitch,
    CatalogMutation,
    FrameRejection,
    StreamRoleRejection,
    Backpressure,
    CompletionEmitted,
    ContractRejected,
    UnsupportedVersion,
    SchemaLayoutDecision,
    CorruptionBoundary,
    SecurityAuthorization,
    SecurityAudit,
    AdminOperation,
    ResourceGovernance,
    BusinessRuleDecision,
    IoPlacementDecision,
    PlacementAudit,
    IoBudgetValidation,
    GpuPolicyDecision,
    TransactionTransition,
    ExecutionTransition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CriticalDecisionTrace {
    pub trace_id: TraceId,
    pub decision: CriticalDecisionKind,
    pub reason: String,
}

impl CriticalDecisionTrace {
    pub fn has_explanation(&self) -> bool {
        !self.reason.trim().is_empty()
    }
}
