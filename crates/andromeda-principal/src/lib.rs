#![forbid(unsafe_code)]
#![doc = r#"
# Andromeda Principal

Low-level principal identity, session, permission-set, policy-evidence, and
registry primitives shared by IAM, admission, transport, audit, and the legacy
core facade.

This crate owns principal runtime primitives only. Stable permission and surface
vocabulary continues to come from `andromeda-security-contract`.
"#]

mod principal;

pub use principal::{
    CertificateFingerprint, CertificateIdentity, CertificateIdentityStatus,
    PRINCIPAL_POLICY_EVIDENCE_VERSION, Permission, PermissionSet, Principal,
    PrincipalAuthorizationDecision, PrincipalAuthorizationDenialReason,
    PrincipalAuthorizationEvaluationStage, PrincipalAuthorizationEvidence,
    PrincipalAuthorizationOutcome, PrincipalBinding, PrincipalId, PrincipalPolicyEvidenceBinding,
    PrincipalPolicyVersion, PrincipalRegistry, PrincipalRole, PrincipalStatus, SessionToken,
    SurfaceScope, UserPrincipal,
};
