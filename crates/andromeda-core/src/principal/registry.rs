use std::collections::BTreeMap;

use super::{
    CertificateIdentity, CertificateIdentityStatus, Permission, PermissionSet, Principal,
    PrincipalId, PrincipalStatus, SurfaceScope,
};
use crate::{AndromedaError, AndromedaErrorKind, AndromedaResult, ContractHash, ProcedureId};

/// Immutable registry row that binds one certificate identity to one principal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrincipalBinding {
    certificate: CertificateIdentity,
    principal: Principal,
    direct_permissions: PermissionSet,
}

impl PrincipalBinding {
    pub fn new(certificate: CertificateIdentity, principal: Principal) -> AndromedaResult<Self> {
        Self::new_with_direct_permissions(certificate, principal, PermissionSet::new())
    }

    pub fn new_with_direct_permissions(
        certificate: CertificateIdentity,
        principal: Principal,
        direct_permissions: PermissionSet,
    ) -> AndromedaResult<Self> {
        if !certificate.has_identity_evidence() {
            return Err(security_error(
                "principal binding requires certificate identity evidence",
            ));
        }
        if principal.id.is_zero()
            || principal.session_token.is_empty()
            || principal.cert_fingerprint.is_empty()
        {
            return Err(security_error(
                "principal binding requires principal identity evidence",
            ));
        }
        if principal.cert_fingerprint != *certificate.fingerprint() {
            return Err(security_error(
                "principal binding certificate fingerprint must match principal fingerprint",
            ));
        }
        if direct_permissions
            .iter()
            .any(|permission| !certificate.surface_scope().permits_permission(permission))
        {
            return Err(security_error(
                "principal binding direct permissions must be permitted by certificate surface scope",
            ));
        }

        Ok(Self {
            certificate,
            principal,
            direct_permissions,
        })
    }

    pub fn certificate(&self) -> &CertificateIdentity {
        &self.certificate
    }

    pub fn principal(&self) -> &Principal {
        &self.principal
    }

    pub fn direct_permissions(&self) -> &PermissionSet {
        &self.direct_permissions
    }

    pub fn role_grants(&self, required: &Permission) -> bool {
        self.certificate.is_active()
            && self.principal.is_active()
            && self
                .certificate
                .surface_scope()
                .permits_permission(required)
            && self.principal.has_permission(required)
    }

    pub fn direct_grants(&self, required: &Permission) -> bool {
        self.certificate.is_active()
            && self.principal.is_active()
            && self
                .certificate
                .surface_scope()
                .permits_permission(required)
            && self.direct_permissions.has_permission(required)
    }

    pub fn grants(&self, required: &Permission) -> bool {
        self.role_grants(required) || self.direct_grants(required)
    }

    fn with_certificate_status(&self, status: CertificateIdentityStatus) -> Self {
        Self {
            certificate: self.certificate.with_status(status),
            principal: self.principal.clone(),
            direct_permissions: self.direct_permissions.clone(),
        }
    }

    fn with_principal_status(&self, status: PrincipalStatus) -> Self {
        Self {
            certificate: self.certificate.clone(),
            principal: self.principal.with_status(status),
            direct_permissions: self.direct_permissions.clone(),
        }
    }
}

/// In-memory certificate-to-principal registry.
#[derive(Debug, Clone)]
pub struct PrincipalRegistry {
    bindings: BTreeMap<String, PrincipalBinding>,
    policy_version: PrincipalPolicyVersion,
}

impl PrincipalRegistry {
    pub fn new() -> Self {
        Self {
            bindings: BTreeMap::new(),
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
            bindings: BTreeMap::new(),
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
        let fingerprint = binding.certificate.fingerprint().as_str().to_string();
        if let Some(existing) = self.bindings.get(&fingerprint) {
            if existing == &binding {
                return Ok(());
            }
            if existing.principal.id != binding.principal.id {
                return Err(security_error(
                    "certificate fingerprint already bound to a different principal id",
                ));
            }
            return Err(security_error(
                "certificate fingerprint already has an immutable principal binding",
            ));
        }

        self.bindings.insert(fingerprint, binding);
        Ok(())
    }

    pub fn lookup(&self, fingerprint: &str) -> Option<&PrincipalBinding> {
        self.bindings.get(fingerprint.trim())
    }

    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub fn revoke_certificate(&mut self, fingerprint: &str) -> AndromedaResult<()> {
        self.set_certificate_status(fingerprint, CertificateIdentityStatus::Revoked)
    }

    pub fn disable_certificate(&mut self, fingerprint: &str) -> AndromedaResult<()> {
        self.set_certificate_status(fingerprint, CertificateIdentityStatus::Disabled)
    }

    pub fn disable_principal(&mut self, principal_id: PrincipalId) -> AndromedaResult<()> {
        let mut found = false;
        for binding in self.bindings.values_mut() {
            if binding.principal.id == principal_id {
                *binding = binding.with_principal_status(PrincipalStatus::Disabled);
                found = true;
            }
        }

        if found {
            Ok(())
        } else {
            Err(security_error("principal id not found"))
        }
    }

    pub fn authorize(
        &self,
        requested_scope: SurfaceScope,
        presented_fingerprint: &str,
        required_permission: &Permission,
    ) -> PrincipalAuthorizationDecision {
        let Some(binding) = self.lookup(presented_fingerprint) else {
            return PrincipalAuthorizationDecision::denied(
                AuthorizationDecisionSeed::unknown(
                    self.policy_version,
                    requested_scope,
                    presented_fingerprint,
                    required_permission.clone(),
                ),
                PrincipalAuthorizationDenialReason::UnknownCertificate,
            );
        };

        if !binding.certificate.is_active() {
            return PrincipalAuthorizationDecision::denied_from_binding(
                requested_scope,
                self.policy_version,
                binding,
                required_permission.clone(),
                certificate_status_denial_reason(binding.certificate.status()),
                AuthorizationCheckState::not_evaluated(),
            );
        }

        if binding.certificate.surface_scope() != requested_scope {
            return PrincipalAuthorizationDecision::denied_from_binding(
                requested_scope,
                self.policy_version,
                binding,
                required_permission.clone(),
                PrincipalAuthorizationDenialReason::SurfaceScopeMismatch,
                AuthorizationCheckState::not_evaluated(),
            );
        }

        if !binding.principal.is_active() {
            return PrincipalAuthorizationDecision::denied_from_binding(
                requested_scope,
                self.policy_version,
                binding,
                required_permission.clone(),
                PrincipalAuthorizationDenialReason::PrincipalDisabled,
                AuthorizationCheckState::not_evaluated(),
            );
        }

        if !requested_scope.permits_permission(required_permission) {
            return PrincipalAuthorizationDecision::denied_from_binding(
                requested_scope,
                self.policy_version,
                binding,
                required_permission.clone(),
                PrincipalAuthorizationDenialReason::SurfaceDoesNotPermitPermission,
                AuthorizationCheckState::surface_policy(false),
            );
        }

        let role_permission_granted = binding.role_grants(required_permission);
        let direct_permission_granted = binding.direct_grants(required_permission);
        if !(role_permission_granted || direct_permission_granted) {
            return PrincipalAuthorizationDecision::denied_from_binding(
                requested_scope,
                self.policy_version,
                binding,
                required_permission.clone(),
                PrincipalAuthorizationDenialReason::PrincipalMissingPermission,
                AuthorizationCheckState::permission_grants(
                    role_permission_granted,
                    direct_permission_granted,
                ),
            );
        }

        PrincipalAuthorizationDecision::allowed_from_binding(
            requested_scope,
            self.policy_version,
            binding,
            required_permission.clone(),
            AuthorizationCheckState::permission_grants(
                role_permission_granted,
                direct_permission_granted,
            ),
        )
    }

    fn set_certificate_status(
        &mut self,
        fingerprint: &str,
        status: CertificateIdentityStatus,
    ) -> AndromedaResult<()> {
        let key = fingerprint.trim();
        let binding = self
            .bindings
            .get(key)
            .cloned()
            .ok_or_else(|| security_error("certificate identity not found"))?;
        self.bindings
            .insert(key.to_string(), binding.with_certificate_status(status));
        Ok(())
    }
}

impl Default for PrincipalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Version proof for the principal authorization policy matrix.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PrincipalPolicyVersion([u8; ContractHash::LEN]);

pub const PRINCIPAL_POLICY_EVIDENCE_VERSION: u64 = 1;

impl PrincipalPolicyVersion {
    pub const LEN: usize = ContractHash::LEN;

    pub const fn new(bytes: [u8; Self::LEN]) -> Self {
        Self(bytes)
    }

    pub const fn zero() -> Self {
        Self([0; Self::LEN])
    }

    pub const fn test_vector(byte: u8) -> Self {
        Self([byte; Self::LEN])
    }

    pub const fn as_bytes(self) -> [u8; Self::LEN] {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.iter().all(|byte| *byte == 0)
    }

    pub fn sha256_digest(self) -> String {
        format!("sha256:{self}")
    }

    pub fn evidence_binding(self) -> AndromedaResult<PrincipalPolicyEvidenceBinding> {
        PrincipalPolicyEvidenceBinding::from_principal_policy_version(
            PRINCIPAL_POLICY_EVIDENCE_VERSION,
            self,
        )
    }

    pub fn current() -> Self {
        let mut hasher = crate::digest::Sha256::new();
        update_policy_segment(&mut hasher, "andromeda.core.principal.policy.v1");
        update_policy_segment(&mut hasher, "surface-policy");
        for surface in policy_surfaces() {
            update_policy_segment(&mut hasher, surface.as_str());
            for permission in policy_permission_probes() {
                update_policy_segment(&mut hasher, policy_permission_key(&permission));
                update_policy_segment(
                    &mut hasher,
                    if surface.permits_permission(&permission) {
                        "permit"
                    } else {
                        "deny"
                    },
                );
            }
        }

        update_policy_segment(&mut hasher, "role-policy");
        for role in policy_roles() {
            let grants = role.permissions();
            update_policy_segment(&mut hasher, role.as_str());
            for permission in policy_permission_probes() {
                update_policy_segment(&mut hasher, policy_permission_key(&permission));
                update_policy_segment(
                    &mut hasher,
                    if grants.has_permission(&permission) {
                        "grant"
                    } else {
                        "deny"
                    },
                );
            }
        }

        update_policy_segment(&mut hasher, "direct-permission-rule");
        update_policy_segment(
            &mut hasher,
            "certificate-surface-must-permit-direct-permission",
        );
        Self(hasher.finalize())
    }
}

/// Shared policy evidence shape consumed by authorization and durable audit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrincipalPolicyEvidenceBinding {
    policy_version: u64,
    policy_digest: String,
}

impl PrincipalPolicyEvidenceBinding {
    pub fn new(policy_version: u64, policy_digest: impl Into<String>) -> AndromedaResult<Self> {
        let binding = Self {
            policy_version,
            policy_digest: policy_digest.into(),
        };
        if !binding.has_version_evidence() {
            return Err(security_error(
                "principal policy evidence requires non-zero version and canonical sha256 digest",
            ));
        }
        Ok(binding)
    }

    pub fn current() -> AndromedaResult<Self> {
        PrincipalPolicyVersion::current().evidence_binding()
    }

    pub fn from_principal_policy_version(
        policy_version: u64,
        principal_policy_version: PrincipalPolicyVersion,
    ) -> AndromedaResult<Self> {
        if principal_policy_version.is_zero() {
            return Err(security_error(
                "principal policy evidence requires non-zero principal policy version",
            ));
        }
        Self::new(policy_version, principal_policy_version.sha256_digest())
    }

    pub const fn policy_version(&self) -> u64 {
        self.policy_version
    }

    pub fn policy_digest(&self) -> &str {
        self.policy_digest.as_str()
    }

    pub fn has_version_evidence(&self) -> bool {
        Self::has_version_evidence_parts(self.policy_version, self.policy_digest())
    }

    pub fn has_version_evidence_parts(policy_version: u64, policy_digest: &str) -> bool {
        policy_version != 0 && Self::is_canonical_sha256_digest(policy_digest)
    }

    pub fn matches_version_and_digest(&self, policy_version: u64, policy_digest: &str) -> bool {
        self.policy_version == policy_version
            && self.policy_digest == policy_digest
            && self.has_version_evidence()
            && Self::has_version_evidence_parts(policy_version, policy_digest)
    }

    pub fn matches_principal_policy_version(
        &self,
        principal_policy_version: PrincipalPolicyVersion,
    ) -> bool {
        self.policy_version == PRINCIPAL_POLICY_EVIDENCE_VERSION
            && !principal_policy_version.is_zero()
            && self.policy_digest == principal_policy_version.sha256_digest()
    }

    fn is_canonical_sha256_digest(value: &str) -> bool {
        value.strip_prefix("sha256:").is_some_and(|hex| {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
        })
    }
}

impl Default for PrincipalPolicyVersion {
    fn default() -> Self {
        Self::current()
    }
}

impl std::fmt::Debug for PrincipalPolicyVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PrincipalPolicyVersion({self})")
    }
}

impl std::fmt::Display for PrincipalPolicyVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Stable authorization outcome for core IAM decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrincipalAuthorizationOutcome {
    Allowed,
    Denied,
}

impl PrincipalAuthorizationOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
        }
    }
}

/// Machine-classifiable denial reason for IAM policy decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrincipalAuthorizationDenialReason {
    UnknownCertificate,
    CertificateDisabled,
    CertificateRevoked,
    SurfaceScopeMismatch,
    PrincipalDisabled,
    SurfaceDoesNotPermitPermission,
    PrincipalMissingPermission,
}

impl PrincipalAuthorizationDenialReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnknownCertificate => "unknown_certificate",
            Self::CertificateDisabled => "certificate_disabled",
            Self::CertificateRevoked => "certificate_revoked",
            Self::SurfaceScopeMismatch => "surface_scope_mismatch",
            Self::PrincipalDisabled => "principal_disabled",
            Self::SurfaceDoesNotPermitPermission => "surface_does_not_permit_permission",
            Self::PrincipalMissingPermission => "principal_missing_permission",
        }
    }

    pub const fn evaluation_stage(self) -> PrincipalAuthorizationEvaluationStage {
        match self {
            Self::UnknownCertificate => PrincipalAuthorizationEvaluationStage::CertificateLookup,
            Self::CertificateDisabled => PrincipalAuthorizationEvaluationStage::CertificateStatus,
            Self::CertificateRevoked => PrincipalAuthorizationEvaluationStage::CertificateStatus,
            Self::SurfaceScopeMismatch => PrincipalAuthorizationEvaluationStage::SurfaceScope,
            Self::PrincipalDisabled => PrincipalAuthorizationEvaluationStage::PrincipalStatus,
            Self::SurfaceDoesNotPermitPermission => {
                PrincipalAuthorizationEvaluationStage::SurfacePolicy
            }
            Self::PrincipalMissingPermission => PrincipalAuthorizationEvaluationStage::Permission,
        }
    }
}

impl std::fmt::Display for PrincipalAuthorizationDenialReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Stable audit stage reached by an IAM authorization decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrincipalAuthorizationEvaluationStage {
    CertificateLookup,
    CertificateStatus,
    SurfaceScope,
    PrincipalStatus,
    SurfacePolicy,
    Permission,
    Allowed,
}

impl PrincipalAuthorizationEvaluationStage {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CertificateLookup => "certificate_lookup",
            Self::CertificateStatus => "certificate_status",
            Self::SurfaceScope => "surface_scope",
            Self::PrincipalStatus => "principal_status",
            Self::SurfacePolicy => "surface_policy",
            Self::Permission => "permission",
            Self::Allowed => "allowed",
        }
    }
}

impl std::fmt::Display for PrincipalAuthorizationEvaluationStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Audit-ready evidence emitted by core IAM decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrincipalAuthorizationEvidence {
    pub policy_version: PrincipalPolicyVersion,
    pub surface_scope: SurfaceScope,
    pub certificate_surface_scope: Option<SurfaceScope>,
    pub certificate_fingerprint: String,
    pub certificate_subject: String,
    pub certificate_status: Option<CertificateIdentityStatus>,
    pub principal_id: Option<PrincipalId>,
    pub principal_status: Option<PrincipalStatus>,
    pub required_permission: Permission,
    pub outcome: PrincipalAuthorizationOutcome,
    pub reason: String,
    pub evaluation_stage: PrincipalAuthorizationEvaluationStage,
    pub surface_policy_evaluated: bool,
    pub surface_policy_allowed: bool,
    pub role_permission_evaluated: bool,
    pub role_permission_granted: bool,
    pub direct_permission_evaluated: bool,
    pub direct_permission_granted: bool,
}

impl PrincipalAuthorizationEvidence {
    pub fn has_identity_evidence(&self) -> bool {
        !self.certificate_fingerprint.trim().is_empty()
            && !self.certificate_subject.trim().is_empty()
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub fn has_policy_version(&self) -> bool {
        !self.policy_version.is_zero()
    }

    pub fn policy_evidence_binding(&self) -> AndromedaResult<PrincipalPolicyEvidenceBinding> {
        self.policy_version.evidence_binding()
    }

    pub fn matches_policy_evidence_binding(
        &self,
        binding: &PrincipalPolicyEvidenceBinding,
    ) -> bool {
        self.matches_policy_version_and_digest(binding)
    }

    pub fn matches_policy_version_and_digest(
        &self,
        binding: &PrincipalPolicyEvidenceBinding,
    ) -> bool {
        self.policy_evidence_binding()
            .is_ok_and(|expected| expected == *binding)
    }

    pub fn reason_code(&self) -> &str {
        self.reason.as_str()
    }

    pub const fn evaluation_stage(&self) -> PrincipalAuthorizationEvaluationStage {
        self.evaluation_stage
    }

    pub fn is_audit_ready(&self) -> bool {
        self.has_identity_evidence() && self.has_reason() && self.policy_evidence_binding().is_ok()
    }
}

/// Result of a core IAM authorization decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrincipalAuthorizationDecision {
    pub outcome: PrincipalAuthorizationOutcome,
    pub principal_id: Option<PrincipalId>,
    pub required_permission: Permission,
    pub denial_reason: Option<PrincipalAuthorizationDenialReason>,
    pub evidence: PrincipalAuthorizationEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AuthorizationDecisionSeed {
    policy_version: PrincipalPolicyVersion,
    surface_scope: SurfaceScope,
    certificate_surface_scope: Option<SurfaceScope>,
    certificate_fingerprint: String,
    certificate_subject: String,
    certificate_status: Option<CertificateIdentityStatus>,
    principal_id: Option<PrincipalId>,
    principal_status: Option<PrincipalStatus>,
    required_permission: Permission,
    checks: AuthorizationCheckState,
}

impl AuthorizationDecisionSeed {
    fn from_binding(
        surface_scope: SurfaceScope,
        policy_version: PrincipalPolicyVersion,
        binding: &PrincipalBinding,
        required_permission: Permission,
        checks: AuthorizationCheckState,
    ) -> Self {
        Self {
            policy_version,
            surface_scope,
            certificate_surface_scope: Some(binding.certificate.surface_scope()),
            certificate_fingerprint: binding.certificate.fingerprint().as_str().to_string(),
            certificate_subject: binding.certificate.subject().to_string(),
            certificate_status: Some(binding.certificate.status()),
            principal_id: Some(binding.principal.id),
            principal_status: Some(binding.principal.status),
            required_permission,
            checks,
        }
    }

    fn unknown(
        policy_version: PrincipalPolicyVersion,
        surface_scope: SurfaceScope,
        presented_fingerprint: &str,
        required_permission: Permission,
    ) -> Self {
        Self {
            policy_version,
            surface_scope,
            certificate_surface_scope: None,
            certificate_fingerprint: fingerprint_for_audit(presented_fingerprint),
            certificate_subject: "subject:unknown".to_string(),
            certificate_status: None,
            principal_id: None,
            principal_status: None,
            required_permission,
            checks: AuthorizationCheckState::not_evaluated(),
        }
    }

    fn evidence(
        &self,
        outcome: PrincipalAuthorizationOutcome,
        reason: &str,
        evaluation_stage: PrincipalAuthorizationEvaluationStage,
    ) -> PrincipalAuthorizationEvidence {
        PrincipalAuthorizationEvidence {
            policy_version: self.policy_version,
            surface_scope: self.surface_scope,
            certificate_surface_scope: self.certificate_surface_scope,
            certificate_fingerprint: self.certificate_fingerprint.clone(),
            certificate_subject: self.certificate_subject.clone(),
            certificate_status: self.certificate_status,
            principal_id: self.principal_id,
            principal_status: self.principal_status,
            required_permission: self.required_permission.clone(),
            outcome,
            reason: reason.to_string(),
            evaluation_stage,
            surface_policy_evaluated: self.checks.surface_policy_evaluated,
            surface_policy_allowed: self.checks.surface_policy_allowed,
            role_permission_evaluated: self.checks.role_permission_evaluated,
            role_permission_granted: self.checks.role_permission_granted,
            direct_permission_evaluated: self.checks.direct_permission_evaluated,
            direct_permission_granted: self.checks.direct_permission_granted,
        }
    }
}

impl PrincipalAuthorizationDecision {
    fn allowed_from_binding(
        surface_scope: SurfaceScope,
        policy_version: PrincipalPolicyVersion,
        binding: &PrincipalBinding,
        required_permission: Permission,
        checks: AuthorizationCheckState,
    ) -> Self {
        let seed = AuthorizationDecisionSeed::from_binding(
            surface_scope,
            policy_version,
            binding,
            required_permission,
            checks,
        );
        Self {
            outcome: PrincipalAuthorizationOutcome::Allowed,
            principal_id: Some(binding.principal.id),
            required_permission: seed.required_permission.clone(),
            denial_reason: None,
            evidence: seed.evidence(
                PrincipalAuthorizationOutcome::Allowed,
                "allowed",
                PrincipalAuthorizationEvaluationStage::Allowed,
            ),
        }
    }

    fn denied_from_binding(
        surface_scope: SurfaceScope,
        policy_version: PrincipalPolicyVersion,
        binding: &PrincipalBinding,
        required_permission: Permission,
        reason: PrincipalAuthorizationDenialReason,
        checks: AuthorizationCheckState,
    ) -> Self {
        Self::denied(
            AuthorizationDecisionSeed::from_binding(
                surface_scope,
                policy_version,
                binding,
                required_permission,
                checks,
            ),
            reason,
        )
    }

    fn denied(seed: AuthorizationDecisionSeed, reason: PrincipalAuthorizationDenialReason) -> Self {
        Self {
            outcome: PrincipalAuthorizationOutcome::Denied,
            principal_id: seed.principal_id,
            required_permission: seed.required_permission.clone(),
            denial_reason: Some(reason),
            evidence: seed.evidence(
                PrincipalAuthorizationOutcome::Denied,
                reason.as_str(),
                reason.evaluation_stage(),
            ),
        }
    }

    pub const fn is_allowed(&self) -> bool {
        matches!(self.outcome, PrincipalAuthorizationOutcome::Allowed)
    }

    pub const fn is_denied(&self) -> bool {
        matches!(self.outcome, PrincipalAuthorizationOutcome::Denied)
    }
}

fn security_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}

fn certificate_status_denial_reason(
    status: CertificateIdentityStatus,
) -> PrincipalAuthorizationDenialReason {
    match status {
        CertificateIdentityStatus::Active => PrincipalAuthorizationDenialReason::CertificateRevoked,
        CertificateIdentityStatus::Disabled => {
            PrincipalAuthorizationDenialReason::CertificateDisabled
        }
        CertificateIdentityStatus::Revoked => {
            PrincipalAuthorizationDenialReason::CertificateRevoked
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AuthorizationCheckState {
    surface_policy_evaluated: bool,
    surface_policy_allowed: bool,
    role_permission_evaluated: bool,
    role_permission_granted: bool,
    direct_permission_evaluated: bool,
    direct_permission_granted: bool,
}

impl AuthorizationCheckState {
    const fn not_evaluated() -> Self {
        Self {
            surface_policy_evaluated: false,
            surface_policy_allowed: false,
            role_permission_evaluated: false,
            role_permission_granted: false,
            direct_permission_evaluated: false,
            direct_permission_granted: false,
        }
    }

    const fn surface_policy(surface_policy_allowed: bool) -> Self {
        Self {
            surface_policy_evaluated: true,
            surface_policy_allowed,
            role_permission_evaluated: false,
            role_permission_granted: false,
            direct_permission_evaluated: false,
            direct_permission_granted: false,
        }
    }

    const fn permission_grants(
        role_permission_granted: bool,
        direct_permission_granted: bool,
    ) -> Self {
        Self {
            surface_policy_evaluated: true,
            surface_policy_allowed: true,
            role_permission_evaluated: true,
            role_permission_granted,
            direct_permission_evaluated: true,
            direct_permission_granted,
        }
    }
}

fn fingerprint_for_audit(presented: &str) -> String {
    let trimmed = presented.trim();
    if trimmed.is_empty() {
        "fingerprint:empty".to_string()
    } else {
        format!(
            "fingerprint:{}",
            trimmed.chars().take(64).collect::<String>()
        )
    }
}

fn update_policy_segment(hasher: &mut crate::digest::Sha256, segment: &str) {
    let bytes = segment.as_bytes();
    hasher.update(&(bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

fn policy_surfaces() -> [SurfaceScope; 5] {
    [
        SurfaceScope::Application,
        SurfaceScope::Administration,
        SurfaceScope::Cluster,
        SurfaceScope::BackupAgent,
        SurfaceScope::MonitoringAgent,
    ]
}

fn policy_roles() -> [super::PrincipalRole; 5] {
    [
        super::PrincipalRole::SuperAdmin,
        super::PrincipalRole::Admin,
        super::PrincipalRole::Operator,
        super::PrincipalRole::User,
        super::PrincipalRole::Guest,
    ]
}

fn policy_permission_probes() -> [Permission; 11] {
    [
        Permission::ExecuteProcedure(ProcedureId::new(1)),
        Permission::ReadContractMetadata,
        Permission::AdminRoleManagement,
        Permission::AdminCatalogPublish,
        Permission::AdminShutdown,
        Permission::AdminRecovery,
        Permission::AuditRead,
        Permission::AdminCertificateRotate,
        Permission::ClusterPromote,
        Permission::ClusterFenceNode,
        Permission::ClusterManifestUpdate,
    ]
}

fn policy_permission_key(permission: &Permission) -> &'static str {
    match permission {
        Permission::ExecuteProcedure(_) => "execute_procedure:any",
        Permission::ReadContractMetadata => "read_contract_metadata",
        Permission::AdminRoleManagement => "admin_role_management",
        Permission::AdminCatalogPublish => "admin_catalog_publish",
        Permission::AdminShutdown => "admin_shutdown",
        Permission::AdminRecovery => "admin_recovery",
        Permission::AuditRead => "audit_read",
        Permission::AdminCertificateRotate => "admin_certificate_rotate",
        Permission::ClusterPromote => "cluster_promote",
        Permission::ClusterFenceNode => "cluster_fence_node",
        Permission::ClusterManifestUpdate => "cluster_manifest_update",
    }
}
