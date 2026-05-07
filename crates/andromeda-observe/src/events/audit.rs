use crate::TraceId;
use andromeda_core::{
    AndromedaResult, PRINCIPAL_POLICY_EVIDENCE_VERSION, PrincipalPolicyEvidenceBinding,
    PrincipalPolicyVersion,
};

use super::{EventSchemaVersion, contains_sensitive_marker, non_empty_reason, observe_error};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceScope {
    Application,
    Administration,
    Cluster,
    BackupAgent,
    MonitoringAgent,
}

impl SurfaceScope {
    pub const fn permits_admin_operation(self) -> bool {
        !matches!(self, Self::Application)
    }

    pub const fn permits_permission(self, permission: Permission) -> bool {
        match self {
            Self::Application => {
                matches!(
                    permission,
                    Permission::ExecuteProcedure | Permission::ReadContract
                )
            }
            Self::BackupAgent => matches!(
                permission.family(),
                PermissionFamily::Recovery | PermissionFamily::Diagnostics
            ),
            Self::MonitoringAgent => matches!(permission.family(), PermissionFamily::Diagnostics),
            Self::Administration => matches!(
                permission.family(),
                PermissionFamily::Definition
                    | PermissionFamily::Diagnostics
                    | PermissionFamily::Security
                    | PermissionFamily::Recovery
            ),
            Self::Cluster => matches!(permission.family(), PermissionFamily::Cluster),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionFamily {
    Application,
    Definition,
    Diagnostics,
    Security,
    Recovery,
    Cluster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    ExecuteProcedure,
    ReadContract,
    CreateTable,
    CreateMap,
    CreateProcedure,
    ImportDefinitionBatch,
    DebugProcedure,
    ReadProcedureStore,
    InspectPlans,
    ManageSecurity,
    RotateCertificate,
    RevokeCertificateIdentity,
    Backup,
    Restore,
    ForensicStart,
    ClusterPromote,
    FenceNode,
    UpdateClusterManifest,
}

impl Permission {
    pub const fn family(self) -> PermissionFamily {
        match self {
            Self::ExecuteProcedure | Self::ReadContract => PermissionFamily::Application,
            Self::CreateTable
            | Self::CreateMap
            | Self::CreateProcedure
            | Self::ImportDefinitionBatch => PermissionFamily::Definition,
            Self::DebugProcedure | Self::ReadProcedureStore | Self::InspectPlans => {
                PermissionFamily::Diagnostics
            }
            Self::ManageSecurity | Self::RotateCertificate | Self::RevokeCertificateIdentity => {
                PermissionFamily::Security
            }
            Self::Backup | Self::Restore | Self::ForensicStart => PermissionFamily::Recovery,
            Self::ClusterPromote | Self::FenceNode | Self::UpdateClusterManifest => {
                PermissionFamily::Cluster
            }
        }
    }

    pub const fn is_admin_operation_permission(self) -> bool {
        matches!(
            self,
            Self::DebugProcedure
                | Self::ReadProcedureStore
                | Self::InspectPlans
                | Self::ManageSecurity
                | Self::RotateCertificate
                | Self::RevokeCertificateIdentity
                | Self::Backup
                | Self::Restore
                | Self::ForensicStart
                | Self::ClusterPromote
                | Self::FenceNode
                | Self::UpdateClusterManifest
        )
    }

    pub const fn authorizes_admin_operation(self, operation: AdminOperation) -> bool {
        matches!(
            (self, operation),
            (Self::DebugProcedure, AdminOperation::DebugProcedure)
                | (Self::ReadProcedureStore, AdminOperation::ReadProcedureStore,)
                | (Self::InspectPlans, AdminOperation::InspectPlans)
                | (Self::ManageSecurity, AdminOperation::ManageSecurity)
                | (Self::RotateCertificate, AdminOperation::RotateCertificate)
                | (
                    Self::RevokeCertificateIdentity,
                    AdminOperation::RevokeCertificateIdentity,
                )
                | (Self::Backup, AdminOperation::Backup)
                | (Self::Restore, AdminOperation::Restore)
                | (Self::ForensicStart, AdminOperation::ForensicStart)
                | (Self::ClusterPromote, AdminOperation::ClusterPromote)
                | (Self::FenceNode, AdminOperation::FenceNode)
                | (
                    Self::UpdateClusterManifest,
                    AdminOperation::UpdateClusterManifest,
                )
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdminOperation {
    DebugProcedure,
    ReadProcedureStore,
    InspectPlans,
    ManageSecurity,
    RotateCertificate,
    RevokeCertificateIdentity,
    Backup,
    Restore,
    ForensicStart,
    ClusterPromote,
    FenceNode,
    UpdateClusterManifest,
}

impl AdminOperation {
    pub const fn required_permission(self) -> Permission {
        match self {
            Self::DebugProcedure => Permission::DebugProcedure,
            Self::ReadProcedureStore => Permission::ReadProcedureStore,
            Self::InspectPlans => Permission::InspectPlans,
            Self::ManageSecurity => Permission::ManageSecurity,
            Self::RotateCertificate => Permission::RotateCertificate,
            Self::RevokeCertificateIdentity => Permission::RevokeCertificateIdentity,
            Self::Backup => Permission::Backup,
            Self::Restore => Permission::Restore,
            Self::ForensicStart => Permission::ForensicStart,
            Self::ClusterPromote => Permission::ClusterPromote,
            Self::FenceNode => Permission::FenceNode,
            Self::UpdateClusterManifest => Permission::UpdateClusterManifest,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateIdentity {
    pub fingerprint: String,
    pub subject: String,
    pub surface: SurfaceScope,
}

impl CertificateIdentity {
    pub fn new(
        fingerprint: impl Into<String>,
        subject: impl Into<String>,
        surface: SurfaceScope,
    ) -> AndromedaResult<Self> {
        Ok(Self {
            fingerprint: non_empty_evidence("certificate fingerprint", fingerprint)?,
            subject: non_empty_evidence("certificate subject", subject)?,
            surface,
        })
    }

    pub fn has_identity_evidence(&self) -> bool {
        !self.fingerprint.trim().is_empty() && !self.subject.trim().is_empty()
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        contains_sensitive_marker(&self.fingerprint) || contains_sensitive_marker(&self.subject)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserPrincipalKind {
    Human,
    Service,
    BreakGlass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPrincipal {
    pub principal_id: String,
    pub kind: UserPrincipalKind,
}

impl UserPrincipal {
    pub fn new(principal_id: impl Into<String>, kind: UserPrincipalKind) -> AndromedaResult<Self> {
        Ok(Self {
            principal_id: non_empty_evidence("principal id", principal_id)?,
            kind,
        })
    }

    pub fn has_identity_evidence(&self) -> bool {
        !self.principal_id.trim().is_empty()
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        contains_sensitive_marker(&self.principal_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SecurityPolicyVersionEvidence {
    pub policy_version: u64,
    pub policy_digest: String,
}

impl SecurityPolicyVersionEvidence {
    pub fn new(policy_version: u64, policy_digest: impl Into<String>) -> AndromedaResult<Self> {
        let evidence = Self {
            policy_version,
            policy_digest: non_empty_evidence("security policy digest", policy_digest)?,
        };
        if !evidence.has_version_evidence() {
            return Err(observe_error(
                "security policy version evidence requires non-zero version and canonical sha256 digest",
            ));
        }
        Ok(evidence)
    }

    pub fn try_bootstrap_v0() -> AndromedaResult<Self> {
        Self::from_core_policy_binding(&PrincipalPolicyEvidenceBinding::current()?)
    }

    /// Compatibility helper for tests and static bootstrap fixtures.
    ///
    /// Request-handling audit paths must prefer [`Self::try_bootstrap_v0`] so
    /// policy-evidence regressions return typed observability errors.
    pub fn bootstrap_v0() -> Self {
        match Self::try_bootstrap_v0() {
            Ok(evidence) => evidence,
            Err(_) => Self {
                policy_version: PRINCIPAL_POLICY_EVIDENCE_VERSION,
                policy_digest: PrincipalPolicyVersion::current().sha256_digest(),
            },
        }
    }

    pub fn from_core_policy_binding(
        binding: &PrincipalPolicyEvidenceBinding,
    ) -> AndromedaResult<Self> {
        Self::new(
            binding.policy_version(),
            binding.policy_digest().to_string(),
        )
    }

    pub fn from_core_policy_version(
        principal_policy_version: PrincipalPolicyVersion,
    ) -> AndromedaResult<Self> {
        Self::from_core_policy_binding(&principal_policy_version.evidence_binding()?)
    }

    pub fn matches_core_policy_binding(&self, binding: &PrincipalPolicyEvidenceBinding) -> bool {
        self.matches_core_policy_version_and_digest(binding)
    }

    pub fn matches_core_policy_version_and_digest(
        &self,
        binding: &PrincipalPolicyEvidenceBinding,
    ) -> bool {
        binding.matches_version_and_digest(self.policy_version, &self.policy_digest)
            && self.has_version_evidence()
    }

    pub fn matches_core_policy_version(
        &self,
        principal_policy_version: PrincipalPolicyVersion,
    ) -> bool {
        principal_policy_version
            .evidence_binding()
            .is_ok_and(|binding| self.matches_core_policy_binding(&binding))
    }

    pub fn has_version_evidence(&self) -> bool {
        PrincipalPolicyEvidenceBinding::has_version_evidence_parts(
            self.policy_version,
            &self.policy_digest,
        ) && !contains_sensitive_marker(&self.policy_digest)
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        contains_sensitive_marker(&self.policy_digest)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityAuditOutcome {
    Allowed,
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecurityAuditTrace {
    pub trace_id: TraceId,
    pub schema_version: EventSchemaVersion,
    pub surface: SurfaceScope,
    pub certificate: CertificateIdentity,
    pub principal: UserPrincipal,
    pub permission: Permission,
    pub outcome: SecurityAuditOutcome,
    pub policy_version: SecurityPolicyVersionEvidence,
    pub reason: String,
}

impl SecurityAuditTrace {
    pub fn new(
        trace_id: TraceId,
        surface: SurfaceScope,
        certificate: CertificateIdentity,
        principal: UserPrincipal,
        permission: Permission,
        outcome: SecurityAuditOutcome,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        Self::new_with_policy_version(
            trace_id,
            surface,
            certificate,
            principal,
            permission,
            outcome,
            SecurityPolicyVersionEvidence::try_bootstrap_v0()?,
            reason,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "Security audit construction keeps identity, permission, decision, policy version, and reason evidence explicit."
    )]
    pub fn new_with_policy_version(
        trace_id: TraceId,
        surface: SurfaceScope,
        certificate: CertificateIdentity,
        principal: UserPrincipal,
        permission: Permission,
        outcome: SecurityAuditOutcome,
        policy_version: SecurityPolicyVersionEvidence,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        if !policy_version.has_version_evidence() {
            return Err(observe_error(
                "security audit traces require security policy version evidence",
            ));
        }
        Ok(Self {
            trace_id,
            schema_version: EventSchemaVersion::V0,
            surface,
            certificate,
            principal,
            permission,
            outcome,
            policy_version,
            reason: non_empty_reason(reason)?,
        })
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_supported_schema_version(&self) -> bool {
        self.schema_version.is_v0()
    }

    pub fn has_identity_evidence(&self) -> bool {
        self.certificate.has_identity_evidence() && self.principal.has_identity_evidence()
    }

    pub const fn surface_matches_certificate(&self) -> bool {
        self.surface as u8 == self.certificate.surface as u8
    }

    pub const fn surface_permits_permission(&self) -> bool {
        self.surface.permits_permission(self.permission)
    }

    pub fn has_policy_version_evidence(&self) -> bool {
        self.policy_version.has_version_evidence()
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        self.certificate.contains_sensitive_evidence()
            || self.principal.contains_sensitive_evidence()
            || self.policy_version.contains_sensitive_evidence()
            || contains_sensitive_marker(&self.reason)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminOperationTrace {
    pub trace_id: TraceId,
    pub schema_version: EventSchemaVersion,
    pub surface: SurfaceScope,
    pub certificate: CertificateIdentity,
    pub principal: UserPrincipal,
    pub operation: AdminOperation,
    pub permission: Permission,
    pub accepted: bool,
    pub reason: String,
}

impl AdminOperationTrace {
    #[allow(
        clippy::too_many_arguments,
        reason = "Audit trace construction keeps certificate, principal, permission, and decision fields explicit."
    )]
    pub fn new(
        trace_id: TraceId,
        surface: SurfaceScope,
        certificate: CertificateIdentity,
        principal: UserPrincipal,
        operation: AdminOperation,
        permission: Permission,
        accepted: bool,
        reason: impl Into<String>,
    ) -> AndromedaResult<Self> {
        Ok(Self {
            trace_id,
            schema_version: EventSchemaVersion::V0,
            surface,
            certificate,
            principal,
            operation,
            permission,
            accepted,
            reason: non_empty_reason(reason)?,
        })
    }

    pub fn has_reason(&self) -> bool {
        !self.reason.trim().is_empty()
    }

    pub const fn has_supported_schema_version(&self) -> bool {
        self.schema_version.is_v0()
    }

    pub const fn surface_permits_operation(&self) -> bool {
        self.surface
            .permits_permission(self.operation.required_permission())
    }

    pub fn has_identity_evidence(&self) -> bool {
        self.certificate.has_identity_evidence() && self.principal.has_identity_evidence()
    }

    pub const fn surface_matches_certificate(&self) -> bool {
        self.surface as u8 == self.certificate.surface as u8
    }

    pub const fn permission_matches_operation(&self) -> bool {
        self.permission.authorizes_admin_operation(self.operation)
    }

    pub fn contains_sensitive_evidence(&self) -> bool {
        self.certificate.contains_sensitive_evidence()
            || self.principal.contains_sensitive_evidence()
            || contains_sensitive_marker(&self.reason)
    }
}

fn non_empty_evidence(label: &str, value: impl Into<String>) -> AndromedaResult<String> {
    let value = value.into();
    if value.trim().is_empty() {
        return Err(observe_error(format!(
            "observability {label} evidence requires a non-empty value",
        )));
    }

    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_core::PRINCIPAL_POLICY_EVIDENCE_VERSION;

    fn certificate() -> CertificateIdentity {
        CertificateIdentity::new(
            "sha256:observe-policy-test",
            "CN=observe-policy-test",
            SurfaceScope::Application,
        )
        .expect("test certificate evidence is explicit")
    }

    fn principal() -> UserPrincipal {
        UserPrincipal::new("user:observe-policy", UserPrincipalKind::Service)
            .expect("test principal evidence is explicit")
    }

    #[test]
    fn security_policy_evidence_converts_from_core_binding() {
        let core_policy_version = PrincipalPolicyVersion::test_vector(0x2a);
        let evidence = SecurityPolicyVersionEvidence::from_core_policy_version(core_policy_version)
            .expect("core policy version converts to audit policy evidence");

        assert_eq!(evidence.policy_version, PRINCIPAL_POLICY_EVIDENCE_VERSION);
        assert!(evidence.matches_core_policy_version(core_policy_version));
        assert!(!evidence.matches_core_policy_version(PrincipalPolicyVersion::test_vector(0x2b)));

        let wrong_version =
            PrincipalPolicyEvidenceBinding::from_principal_policy_version(2, core_policy_version)
                .expect("alternate policy evidence remains well-formed");
        assert!(!evidence.matches_core_policy_version_and_digest(&wrong_version));

        let wrong_digest = PrincipalPolicyVersion::test_vector(0x2b)
            .evidence_binding()
            .expect("alternate digest evidence is well-formed");
        assert!(!evidence.matches_core_policy_version_and_digest(&wrong_digest));
    }

    #[test]
    fn security_policy_evidence_rejects_version_only_or_digest_only() {
        let valid_digest =
            "sha256:1111111111111111111111111111111111111111111111111111111111111111";

        assert!(SecurityPolicyVersionEvidence::new(0, valid_digest).is_err());
        assert!(SecurityPolicyVersionEvidence::new(1, "").is_err());
        assert!(
            SecurityPolicyVersionEvidence::new(
                1,
                "sha256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            )
            .is_err(),
            "audit policy evidence uses the same canonical digest rule as core"
        );

        let version_only = SecurityPolicyVersionEvidence {
            policy_version: 1,
            policy_digest: String::new(),
        };
        let err = SecurityAuditTrace::new_with_policy_version(
            TraceId::new(901),
            SurfaceScope::Application,
            certificate(),
            principal(),
            Permission::ExecuteProcedure,
            SecurityAuditOutcome::Denied,
            version_only,
            "version-only policy evidence is not audit-ready",
        )
        .unwrap_err();
        assert!(err.message().contains("policy version evidence"));
    }
}
