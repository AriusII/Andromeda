#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Maps

Runtime-free Map descriptor, dependency, summarizability, refresh, and staleness primitives.
"#]

mod consistency;
mod dependency;
mod descriptor;
mod diagnostic;
mod error;
mod ownership;
mod publication;
mod refresh;
mod summarizability;

pub use consistency::{
    MapConsistencyPolicy, MapDeltaApplyOutcome, MapDeltaApplyState, MapDeltaLog,
    MapRefreshAdmissionDecision, MapRefreshAdmissionRequest,
};
pub use dependency::{MapDependency, MapDependencyGraph};
pub use descriptor::{MapDescriptor, MapGrain, MapId, MapRefreshMode, MapStalenessPolicy};
pub use diagnostic::{MapValidationDiagnostic, MapValidationReport};
pub use error::{MapDescriptorError, MapDescriptorResult};
pub use ownership::{MapEvidenceAuthority, MapOwnershipBoundary};
pub use publication::{
    MapPublicationCandidate, MapPublicationEvidence, MapPublicationRebuildEvidence,
    MapPublicationRecoveryEvidence, MapPublicationRollbackEvidence, MapPublicationState,
    MapPublicationSwitch, MapValidatedPublicationCandidate,
};
pub use refresh::MapRefreshPlan;
pub use summarizability::{
    MapMeasurePolicy, MapMeasureRollupPolicy, MapSummarizabilityMode, MapSummarizabilityPolicy,
};
