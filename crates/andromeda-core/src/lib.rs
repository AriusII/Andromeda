#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Core

Foundation types for Andromeda crates: identifiers, errors, clocks, SQL type
descriptors, principal identity, and conservative hardware/resource contracts.

The crate is intentionally dependency-light and forbids unsafe code. Public
exports here are cross-crate contracts; implementation details stay private to
their modules.
"#]

pub mod digest;
mod error;
mod ids;
mod time;
mod types;

mod principal;

/// Compatibility exports for crates that still import hardware policy types via
/// `andromeda_core::policy::*`.
pub mod policy {
    pub use crate::{
        CpuCapabilityClass, CpuProfile, GpuExecutionPolicy, GpuProfile, HardwareArchitecture,
        HardwareProfile, PipelineClass, RamProfile, RamSectionBudget, RamSectionRole,
        ResourceBudget,
    };
}

mod hardware_cpu;
mod hardware_gpu;
mod hardware_integration;
mod hardware_pipeline;
mod hardware_ram;

pub use error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
pub use ids::{
    CatalogObjectId, CatalogVersion, ContractHash, DatabaseId, InvocationId, NamespaceId,
    ProcedureId, RequestId, SessionId, TransactionId,
};

pub use principal::{
    CertificateFingerprint, Permission, PermissionSet, Principal, PrincipalId, PrincipalRole,
    SessionToken,
};

pub use hardware_cpu::{CpuCapabilityClass, CpuProfile, HardwareArchitecture};
pub use hardware_gpu::{GpuExecutionPolicy, GpuProfile};
pub use hardware_integration::{HardwareProfile, ResourceBudget};
pub use hardware_pipeline::PipelineClass;
pub use hardware_ram::{RamProfile, RamSectionBudget, RamSectionRole};
pub use time::{Clock, EngineTimestamp, ManualClock, SystemClock};
pub use types::{
    AbsencePolicy, ColumnDescriptor, DecimalType, FloatMode, FloatType, ScalarType, TextEncoding,
    TextType, TimestampType, TypeDescriptor,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_shim_reexports_legacy_hardware_policy_types() {
        let profile = policy::HardwareProfile::conservative();
        assert_eq!(
            profile.cpu.capability_class,
            policy::CpuCapabilityClass::Conservative
        );

        let gpu_policy = policy::GpuExecutionPolicy::OffCriticalPathOnly;
        assert!(!gpu_policy.permits_pipeline(policy::PipelineClass::Commit));
    }
}
