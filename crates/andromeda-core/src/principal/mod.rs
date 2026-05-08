//! Principal identity, roles, and permissions for the IAM pipeline.

mod certificate;
mod certificate_identity;
mod contract;
mod id;
mod identity;
mod permission;
mod permission_set;
mod registry;
mod role;
mod session;
mod status;
mod surface_scope;

pub use certificate::CertificateFingerprint;
pub use certificate_identity::{CertificateIdentity, CertificateIdentityStatus};
pub use id::PrincipalId;
pub use identity::{Principal, UserPrincipal};
pub use permission::Permission;
pub use permission_set::PermissionSet;
pub use registry::{
    PRINCIPAL_POLICY_EVIDENCE_VERSION, PrincipalAuthorizationDecision,
    PrincipalAuthorizationDenialReason, PrincipalAuthorizationEvaluationStage,
    PrincipalAuthorizationEvidence, PrincipalAuthorizationOutcome, PrincipalBinding,
    PrincipalPolicyEvidenceBinding, PrincipalPolicyVersion, PrincipalRegistry,
};
pub use role::PrincipalRole;
pub use session::SessionToken;
pub use status::PrincipalStatus;
pub use surface_scope::SurfaceScope;

#[cfg(test)]
mod tests;
