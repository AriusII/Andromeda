use andromeda_core::{
    AndromedaResult, GpuExecutionPolicy, GpuProfile, PipelineClass, ResourceBudget,
};

use crate::TraceId;

use super::{non_empty_reason, ProtocolEventScope};

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
    IoBudgetValidation,
    GpuPolicyDecision,
    TransactionTransition,
    ExecutionTransition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionTrace {
    pub trace_id: TraceId,
    pub decision: CriticalDecisionKind,
    pub reason: String,
}

impl DecisionTrace {
    pub fn has_explanation(&self) -> bool {
        !self.reason.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaLayoutDecisionTrace {
    pub trace_id: TraceId,
    pub scope: ProtocolEventScope,
    pub schema_id: Option<u64>,
    pub schema_version: Option<u64>,
    pub layout_id: Option<u64>,
    pub layout_version: Option<u64>,
    pub accepted: bool,
    pub reason: String,
}

impl SchemaLayoutDecisionTrace {
    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_schema_layout_evidence(&self) -> bool {
        self.schema_id.is_some()
            && self.schema_version.is_some()
            && self.layout_id.is_some()
            && self.layout_version.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoStorageTier {
    Ram,
    Hot,
    Cold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoPipelineStage {
    Ram,
    Hot,
    Cold,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoPlacementDecisionTrace {
    pub trace_id: TraceId,
    pub pipeline: PipelineClass,
    pub stage: IoPipelineStage,
    pub selected_tier: IoStorageTier,
    pub accepted: bool,
    pub reason: String,
}

impl IoPlacementDecisionTrace {
    pub fn accepted(
        trace_id: TraceId,
        pipeline: PipelineClass,
        stage: IoPipelineStage,
        selected_tier: IoStorageTier,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        Self::new(trace_id, pipeline, stage, selected_tier, true, reason)
    }

    pub fn rejected(
        trace_id: TraceId,
        pipeline: PipelineClass,
        stage: IoPipelineStage,
        selected_tier: IoStorageTier,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        Self::new(trace_id, pipeline, stage, selected_tier, false, reason)
    }

    pub fn new(
        trace_id: TraceId,
        pipeline: PipelineClass,
        stage: IoPipelineStage,
        selected_tier: IoStorageTier,
        accepted: bool,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        let reason = non_empty_reason(reason)?;
        Ok(Self {
            trace_id,
            pipeline,
            stage,
            selected_tier,
            accepted,
            reason,
        })
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IoBudgetDecisionTrace {
    pub trace_id: TraceId,
    pub pipeline: PipelineClass,
    pub stage: IoPipelineStage,
    pub budget: ResourceBudget,
    pub requested_memory_bytes: u64,
    pub requested_temp_bytes: u64,
    pub requested_streams: u32,
    pub accepted: bool,
    pub reason: String,
}

impl IoBudgetDecisionTrace {
    #[allow(
        clippy::too_many_arguments,
        reason = "Trace constructors keep every audited decision input explicit."
    )]
    pub fn from_budget_request(
        trace_id: TraceId,
        pipeline: PipelineClass,
        stage: IoPipelineStage,
        budget: ResourceBudget,
        requested_memory_bytes: u64,
        requested_temp_bytes: u64,
        requested_streams: u32,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        let reason = non_empty_reason(reason)?;
        let accepted = requested_memory_bytes <= budget.max_memory_bytes
            && requested_temp_bytes <= budget.max_temp_bytes
            && requested_streams <= budget.max_streams;

        Ok(Self {
            trace_id,
            pipeline,
            stage,
            budget,
            requested_memory_bytes,
            requested_temp_bytes,
            requested_streams,
            accepted,
            reason,
        })
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_budget_evidence(&self) -> bool {
        self.budget.max_memory_bytes != 0
            || self.budget.max_temp_bytes != 0
            || self.budget.max_streams != 0
    }

    pub const fn requested_within_budget(&self) -> bool {
        self.requested_memory_bytes <= self.budget.max_memory_bytes
            && self.requested_temp_bytes <= self.budget.max_temp_bytes
            && self.requested_streams <= self.budget.max_streams
    }

    pub const fn outcome_matches_budget(&self) -> bool {
        self.accepted == self.requested_within_budget()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuPolicyDecisionTrace {
    pub trace_id: TraceId,
    pub pipeline: PipelineClass,
    pub policy: GpuExecutionPolicy,
    pub gpu_declared_available: bool,
    pub accepted: bool,
    pub reason: String,
}

impl GpuPolicyDecisionTrace {
    pub fn from_policy(
        trace_id: TraceId,
        pipeline: PipelineClass,
        policy: GpuExecutionPolicy,
        gpu_declared_available: bool,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        let reason = non_empty_reason(reason)?;
        Ok(Self {
            trace_id,
            pipeline,
            policy,
            gpu_declared_available,
            accepted: gpu_declared_available && policy.permits_pipeline(pipeline),
            reason,
        })
    }

    pub fn from_profile(
        trace_id: TraceId,
        pipeline: PipelineClass,
        profile: GpuProfile,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        Self::from_policy(
            trace_id,
            pipeline,
            profile.execution_policy,
            profile.available,
            reason,
        )
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn permitted_by_policy(&self) -> bool {
        self.gpu_declared_available && self.policy.permits_pipeline(self.pipeline)
    }

    pub const fn outcome_matches_policy(&self) -> bool {
        self.accepted == self.permitted_by_policy()
    }
}
