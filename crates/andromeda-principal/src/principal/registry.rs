mod authorization;
mod binding_store;
mod denial;
mod policy;

pub use authorization::{
    PrincipalAuthorizationDecision, PrincipalAuthorizationEvidence, PrincipalAuthorizationOutcome,
};
pub use binding_store::PrincipalBinding;
use binding_store::PrincipalBindingStore;
pub use denial::{PrincipalAuthorizationDenialReason, PrincipalAuthorizationEvaluationStage};
pub use policy::{
    PRINCIPAL_POLICY_EVIDENCE_VERSION, PrincipalPolicyEvidenceBinding, PrincipalPolicyVersion,
};

use super::{CertificateIdentityStatus, Permission, PrincipalId, SurfaceScope};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// In-memory certificate-to-principal registry.
#[derive(Debug, Clone)]
pub struct PrincipalRegistry {
    bindings: PrincipalBindingStore,
    policy_version: PrincipalPolicyVersion,
}

impl PrincipalRegistry {
    pub fn new() -> Self {
        Self {
            bindings: PrincipalBindingStore::new(),
            policy_version: PrincipalPolicyVersion::current(),
        }
    }

    pub fn new_with_policy_version(
        policy_version: PrincipalPolicyVersion,
    ) -> AndromedaResult<Self> {
        if policy_version.is_zero() {
            return Err(security_error(
                "principal registry policy version must not be zero",
            ));
        }

        Ok(Self {
            bindings: PrincipalBindingStore::new(),
            policy_version,
        })
    }

    pub const fn policy_version(&self) -> PrincipalPolicyVersion {
        self.policy_version
    }

    pub fn policy_evidence_binding(&self) -> AndromedaResult<PrincipalPolicyEvidenceBinding> {
        self.policy_version.evidence_binding()
    }

    pub fn register(&mut self, binding: PrincipalBinding) -> AndromedaResult<()> {
        self.bindings.register(binding)
    }

    pub fn lookup(&self, fingerprint: &str) -> Option<&PrincipalBinding> {
        self.bindings.lookup(fingerprint)
    }

    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub fn revoke_certificate(&mut self, fingerprint: &str) -> AndromedaResult<()> {
        self.bindings
            .set_certificate_status(fingerprint, CertificateIdentityStatus::Revoked)
    }

    pub fn disable_certificate(&mut self, fingerprint: &str) -> AndromedaResult<()> {
        self.bindings
            .set_certificate_status(fingerprint, CertificateIdentityStatus::Disabled)
    }

    pub fn disable_principal(&mut self, principal_id: PrincipalId) -> AndromedaResult<()> {
        self.bindings.disable_principal(principal_id)
    }

    pub fn authorize(
        &self,
        requested_scope: SurfaceScope,
        presented_fingerprint: &str,
        required_permission: &Permission,
    ) -> PrincipalAuthorizationDecision {
        authorization::authorize(
            self.policy_version,
            requested_scope,
            presented_fingerprint,
            required_permission,
            self.lookup(presented_fingerprint),
        )
    }
}

impl Default for PrincipalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn security_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}
