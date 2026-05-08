#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Maps

Runtime-free Map descriptor, grain, refresh, and staleness primitives.
"#]

mod descriptor;
mod error;
mod publication;

pub use descriptor::{MapDescriptor, MapGrain, MapId, MapRefreshMode, MapStalenessPolicy};
pub use error::{MapDescriptorError, MapDescriptorResult};
pub use publication::{
    MapPublicationCandidate, MapPublicationEvidence, MapPublicationRebuildEvidence,
    MapPublicationRecoveryEvidence, MapPublicationRollbackEvidence, MapPublicationState,
    MapPublicationSwitch, MapValidatedPublicationCandidate,
};
