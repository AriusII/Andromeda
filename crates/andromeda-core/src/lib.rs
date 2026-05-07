#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Core

Foundation types for Andromeda crates: identifiers, errors, clocks,
Andromeda type descriptors, principal identity, and conservative
hardware/resource contracts.

This crate is a compatibility facade over the first foundation split. Public
exports here remain stable while downstream crates migrate to narrower
dependencies.
"#]

mod principal;

/// Compatibility digest module for crates that still use
/// `andromeda_core::digest::*`.
pub mod digest {
    pub use andromeda_digest::{Sha256, sha256};
}

/// Compatibility exports for crates that still import hardware policy types via
/// `andromeda_core::policy::*`.
pub mod policy {
    pub use crate::{
        CpuCapabilityClass, CpuProfile, GpuExecutionPolicy, GpuProfile, HardwareArchitecture,
        HardwareProfile, PipelineClass, RamProfile, RamSectionBudget, RamSectionRole,
        ResourceBudget,
    };
}

pub use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
pub use andromeda_hardware::{
    CpuCapabilityClass, CpuProfile, GpuExecutionPolicy, GpuProfile, HardwareArchitecture,
    HardwareProfile, PipelineClass, RamProfile, RamSectionBudget, RamSectionRole, ResourceBudget,
};
pub use andromeda_time::{Clock, EngineTimestamp, ManualClock, SystemClock};
pub use andromeda_types::{
    AbsencePolicy, CatalogObjectId, CatalogVersion, ColumnDescriptor, ContractHash, DatabaseId,
    DecimalType, FloatMode, FloatType, InvocationId, NamespaceId, ProcedureId, RequestId,
    ScalarType, SessionId, TextEncoding, TextType, TimestampType, TransactionId, TypeDescriptor,
};

pub use principal::{
    CertificateFingerprint, CertificateIdentity, CertificateIdentityStatus,
    PRINCIPAL_POLICY_EVIDENCE_VERSION, Permission, PermissionSet, Principal,
    PrincipalAuthorizationDecision, PrincipalAuthorizationDenialReason,
    PrincipalAuthorizationEvaluationStage, PrincipalAuthorizationEvidence,
    PrincipalAuthorizationOutcome, PrincipalBinding, PrincipalId, PrincipalPolicyEvidenceBinding,
    PrincipalPolicyVersion, PrincipalRegistry, PrincipalRole, PrincipalStatus, SessionToken,
    SurfaceScope, UserPrincipal,
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

    #[test]
    fn foundation_facade_preserves_legacy_public_paths() {
        let _request_id = RequestId::new(1);
        let _timestamp = EngineTimestamp::from_unix_millis(0);
        let _descriptor = TypeDescriptor::required(ScalarType::Bool);
        let _digest = digest::sha256(b"andromeda");
        let _profile = HardwareProfile::conservative();
        let _error = AndromedaError::new(AndromedaErrorKind::Contract, "contract error");
    }
}
