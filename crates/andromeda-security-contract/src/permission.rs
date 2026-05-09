pub const FAMILY_ID_APPLICATION: &str = "application";
pub const FAMILY_ID_DEFINITION: &str = "definition";
pub const FAMILY_ID_DIAGNOSTICS: &str = "diagnostics";
pub const FAMILY_ID_SECURITY: &str = "security";
pub const FAMILY_ID_RECOVERY: &str = "recovery";
pub const FAMILY_ID_CLUSTER: &str = "cluster";

pub const PERMISSION_ID_EXECUTE_PROCEDURE: &str = "andromeda.execute_procedure";
pub const PERMISSION_ID_READ_CONTRACT: &str = "andromeda.read_contract";
pub const PERMISSION_ID_READ_CONTRACT_METADATA: &str = "andromeda.read_contract_metadata";

pub const PERMISSION_ID_CREATE_TABLE: &str = "andromeda.definition.create_table";
pub const PERMISSION_ID_CREATE_MAP: &str = "andromeda.definition.create_map";
pub const PERMISSION_ID_CREATE_PROCEDURE: &str = "andromeda.definition.create_procedure";
pub const PERMISSION_ID_IMPORT_DEFINITION_BATCH: &str =
    "andromeda.definition.import_definition_batch";

pub const PERMISSION_ID_DEBUG_PROCEDURE: &str = "andromeda.diagnostics.debug_procedure";
pub const PERMISSION_ID_READ_PROCEDURE_STORE: &str = "andromeda.diagnostics.read_procedure_store";
pub const PERMISSION_ID_INSPECT_PLANS: &str = "andromeda.diagnostics.inspect_plans";
pub const PERMISSION_ID_READ_AUDIT: &str = "andromeda.diagnostics.read_audit";

pub const PERMISSION_ID_MANAGE_SECURITY: &str = "andromeda.security.manage_security";
pub const PERMISSION_ID_ROTATE_CERTIFICATE: &str = "andromeda.security.rotate_certificate";
pub const PERMISSION_ID_REVOKE_CERTIFICATE_IDENTITY: &str =
    "andromeda.security.revoke_certificate_identity";

pub const PERMISSION_ID_BACKUP: &str = "andromeda.recovery.backup";
pub const PERMISSION_ID_RESTORE: &str = "andromeda.recovery.restore";
pub const PERMISSION_ID_FORENSIC_START: &str = "andromeda.recovery.forensic_start";

pub const PERMISSION_ID_CLUSTER_PROMOTE: &str = "andromeda.cluster.promote";
pub const PERMISSION_ID_CLUSTER_FENCE_NODE: &str = "andromeda.cluster.fence_node";
pub const PERMISSION_ID_CLUSTER_UPDATE_MANIFEST: &str = "andromeda.cluster.update_manifest";

/// Stable permission family used for policy matrices and generated manifests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionFamily {
    Application,
    Definition,
    Diagnostics,
    Security,
    Recovery,
    Cluster,
}

pub const ALL_PERMISSION_FAMILIES: [PermissionFamily; 6] = [
    PermissionFamily::Application,
    PermissionFamily::Definition,
    PermissionFamily::Diagnostics,
    PermissionFamily::Security,
    PermissionFamily::Recovery,
    PermissionFamily::Cluster,
];

impl PermissionFamily {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Application => FAMILY_ID_APPLICATION,
            Self::Definition => FAMILY_ID_DEFINITION,
            Self::Diagnostics => FAMILY_ID_DIAGNOSTICS,
            Self::Security => FAMILY_ID_SECURITY,
            Self::Recovery => FAMILY_ID_RECOVERY,
            Self::Cluster => FAMILY_ID_CLUSTER,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        ALL_PERMISSION_FAMILIES
            .into_iter()
            .find(|family| family.as_str() == id)
    }
}

impl core::fmt::Display for PermissionFamily {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical public security permission identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    ExecuteProcedure,
    ReadContract,
    ReadContractMetadata,
    CreateTable,
    CreateMap,
    CreateProcedure,
    ImportDefinitionBatch,
    DebugProcedure,
    ReadProcedureStore,
    InspectPlans,
    ReadAudit,
    ManageSecurity,
    RotateCertificate,
    RevokeCertificateIdentity,
    Backup,
    Restore,
    ForensicStart,
    ClusterPromote,
    ClusterFenceNode,
    ClusterUpdateManifest,
}

pub const ALL_PERMISSIONS: [Permission; 20] = [
    Permission::ExecuteProcedure,
    Permission::ReadContract,
    Permission::ReadContractMetadata,
    Permission::CreateTable,
    Permission::CreateMap,
    Permission::CreateProcedure,
    Permission::ImportDefinitionBatch,
    Permission::DebugProcedure,
    Permission::ReadProcedureStore,
    Permission::InspectPlans,
    Permission::ReadAudit,
    Permission::ManageSecurity,
    Permission::RotateCertificate,
    Permission::RevokeCertificateIdentity,
    Permission::Backup,
    Permission::Restore,
    Permission::ForensicStart,
    Permission::ClusterPromote,
    Permission::ClusterFenceNode,
    Permission::ClusterUpdateManifest,
];

impl Permission {
    pub const fn canonical_id(self) -> &'static str {
        canonical_permission_id(self)
    }

    pub const fn family(self) -> PermissionFamily {
        match self {
            Self::ExecuteProcedure | Self::ReadContract | Self::ReadContractMetadata => {
                PermissionFamily::Application
            },
            Self::CreateTable
            | Self::CreateMap
            | Self::CreateProcedure
            | Self::ImportDefinitionBatch => PermissionFamily::Definition,
            Self::DebugProcedure
            | Self::ReadProcedureStore
            | Self::InspectPlans
            | Self::ReadAudit => PermissionFamily::Diagnostics,
            Self::ManageSecurity | Self::RotateCertificate | Self::RevokeCertificateIdentity => {
                PermissionFamily::Security
            },
            Self::Backup | Self::Restore | Self::ForensicStart => PermissionFamily::Recovery,
            Self::ClusterPromote | Self::ClusterFenceNode | Self::ClusterUpdateManifest => {
                PermissionFamily::Cluster
            },
        }
    }

    pub const fn descriptor(self) -> PermissionDescriptor {
        PermissionDescriptor {
            id: self.canonical_id(),
            family: self.family(),
        }
    }

    pub fn from_canonical_id(id: &str) -> Option<Self> {
        permission_from_canonical_id(id)
    }
}

impl core::fmt::Display for Permission {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.canonical_id())
    }
}

/// Runtime-free descriptor shape for generated manifests and audit metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PermissionDescriptor {
    pub id: &'static str,
    pub family: PermissionFamily,
}

pub const fn canonical_permission_id(permission: Permission) -> &'static str {
    match permission {
        Permission::ExecuteProcedure => PERMISSION_ID_EXECUTE_PROCEDURE,
        Permission::ReadContract => PERMISSION_ID_READ_CONTRACT,
        Permission::ReadContractMetadata => PERMISSION_ID_READ_CONTRACT_METADATA,
        Permission::CreateTable => PERMISSION_ID_CREATE_TABLE,
        Permission::CreateMap => PERMISSION_ID_CREATE_MAP,
        Permission::CreateProcedure => PERMISSION_ID_CREATE_PROCEDURE,
        Permission::ImportDefinitionBatch => PERMISSION_ID_IMPORT_DEFINITION_BATCH,
        Permission::DebugProcedure => PERMISSION_ID_DEBUG_PROCEDURE,
        Permission::ReadProcedureStore => PERMISSION_ID_READ_PROCEDURE_STORE,
        Permission::InspectPlans => PERMISSION_ID_INSPECT_PLANS,
        Permission::ReadAudit => PERMISSION_ID_READ_AUDIT,
        Permission::ManageSecurity => PERMISSION_ID_MANAGE_SECURITY,
        Permission::RotateCertificate => PERMISSION_ID_ROTATE_CERTIFICATE,
        Permission::RevokeCertificateIdentity => PERMISSION_ID_REVOKE_CERTIFICATE_IDENTITY,
        Permission::Backup => PERMISSION_ID_BACKUP,
        Permission::Restore => PERMISSION_ID_RESTORE,
        Permission::ForensicStart => PERMISSION_ID_FORENSIC_START,
        Permission::ClusterPromote => PERMISSION_ID_CLUSTER_PROMOTE,
        Permission::ClusterFenceNode => PERMISSION_ID_CLUSTER_FENCE_NODE,
        Permission::ClusterUpdateManifest => PERMISSION_ID_CLUSTER_UPDATE_MANIFEST,
    }
}

pub fn permission_from_canonical_id(id: &str) -> Option<Permission> {
    ALL_PERMISSIONS
        .into_iter()
        .find(|permission| permission.canonical_id() == id)
}

pub fn permission_family_for_id(id: &str) -> Option<PermissionFamily> {
    permission_from_canonical_id(id).map(Permission::family)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_permission_ids_are_canonical_and_roundtrip() {
        for permission in ALL_PERMISSIONS {
            let id = permission.canonical_id();
            assert_eq!(id, id.to_ascii_lowercase());
            assert!(id.starts_with("andromeda."));
            assert_eq!(Permission::from_canonical_id(id), Some(permission));
            assert_eq!(permission_family_for_id(id), Some(permission.family()));
        }
    }

    #[test]
    fn test_permission_descriptor_matches_mapping() {
        let descriptor = Permission::RotateCertificate.descriptor();

        assert_eq!(descriptor.id, PERMISSION_ID_ROTATE_CERTIFICATE);
        assert_eq!(descriptor.family, PermissionFamily::Security);
    }

    #[test]
    fn test_existing_manifest_permission_labels_are_preserved() {
        assert_eq!(
            Permission::ExecuteProcedure.canonical_id(),
            "andromeda.execute_procedure"
        );
        assert_eq!(
            Permission::ReadContract.canonical_id(),
            "andromeda.read_contract"
        );
        assert_eq!(
            Permission::ReadContractMetadata.canonical_id(),
            "andromeda.read_contract_metadata"
        );
    }

    #[test]
    fn test_unknown_permission_id_is_rejected() {
        assert_eq!(Permission::from_canonical_id("andromeda.security"), None);
        assert_eq!(Permission::from_canonical_id("ANDROMEDA.SECURITY"), None);
    }
}
