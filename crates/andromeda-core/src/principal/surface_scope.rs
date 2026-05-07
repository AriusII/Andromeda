use super::Permission;

/// Certificate surface scope used before logical permission checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceScope {
    Application,
    Administration,
    Cluster,
    BackupAgent,
    MonitoringAgent,
}

impl SurfaceScope {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::Administration => "administration",
            Self::Cluster => "cluster",
            Self::BackupAgent => "backup_agent",
            Self::MonitoringAgent => "monitoring_agent",
        }
    }

    /// Surface-level policy. This check runs before role or direct permission
    /// grants so a certificate cannot cross from one QUIC surface to another.
    pub const fn permits_permission(self, permission: &Permission) -> bool {
        match self {
            Self::Application => matches!(
                permission,
                Permission::ExecuteProcedure(_) | Permission::ReadContractMetadata
            ),
            Self::Administration => matches!(
                permission,
                Permission::ReadContractMetadata
                    | Permission::AdminRoleManagement
                    | Permission::AdminCatalogPublish
                    | Permission::AdminShutdown
                    | Permission::AdminRecovery
                    | Permission::AuditRead
                    | Permission::AdminCertificateRotate
            ),
            Self::Cluster => matches!(
                permission,
                Permission::ClusterPromote
                    | Permission::ClusterFenceNode
                    | Permission::ClusterManifestUpdate
                    | Permission::AuditRead
            ),
            Self::BackupAgent => {
                matches!(
                    permission,
                    Permission::AdminRecovery | Permission::AuditRead
                )
            }
            Self::MonitoringAgent => {
                matches!(
                    permission,
                    Permission::AuditRead | Permission::ReadContractMetadata
                )
            }
        }
    }
}

impl std::fmt::Display for SurfaceScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
