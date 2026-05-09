use super::super::{Permission, PrincipalRole, SurfaceScope};
use super::security_error;
use andromeda_error::AndromedaResult;
use andromeda_types::{ContractHash, ProcedureId};

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
        let mut hasher = andromeda_digest::Sha256::new();
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

fn update_policy_segment(hasher: &mut andromeda_digest::Sha256, segment: &str) {
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

fn policy_roles() -> [PrincipalRole; 5] {
    [
        PrincipalRole::SuperAdmin,
        PrincipalRole::Admin,
        PrincipalRole::Operator,
        PrincipalRole::User,
        PrincipalRole::Guest,
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
