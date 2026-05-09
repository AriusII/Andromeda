#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceScope {
    Application,
    Administration,
    Cluster,
    BackupAgent,
    MonitoringAgent,
}

impl SurfaceScope {
    pub const fn same_surface(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Application, Self::Application)
                | (Self::Administration, Self::Administration)
                | (Self::Cluster, Self::Cluster)
                | (Self::BackupAgent, Self::BackupAgent)
                | (Self::MonitoringAgent, Self::MonitoringAgent)
        )
    }

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
            },
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
            },
            Self::ManageSecurity | Self::RotateCertificate | Self::RevokeCertificateIdentity => {
                PermissionFamily::Security
            },
            Self::Backup | Self::Restore | Self::ForensicStart => PermissionFamily::Recovery,
            Self::ClusterPromote | Self::FenceNode | Self::UpdateClusterManifest => {
                PermissionFamily::Cluster
            },
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
