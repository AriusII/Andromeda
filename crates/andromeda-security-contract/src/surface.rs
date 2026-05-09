use crate::{Permission, PermissionFamily};

pub const SURFACE_ID_APPLICATION: &str = "application";
pub const SURFACE_ID_ADMINISTRATION: &str = "administration";
pub const SURFACE_ID_CLUSTER: &str = "cluster";
pub const SURFACE_ID_BACKUP_AGENT: &str = "backup_agent";
pub const SURFACE_ID_MONITORING_AGENT: &str = "monitoring_agent";

/// Public security surface bound to an authenticated transport or agent class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecuritySurface {
    Application,
    Administration,
    Cluster,
    BackupAgent,
    MonitoringAgent,
}

pub const ALL_SECURITY_SURFACES: [SecuritySurface; 5] = [
    SecuritySurface::Application,
    SecuritySurface::Administration,
    SecuritySurface::Cluster,
    SecuritySurface::BackupAgent,
    SecuritySurface::MonitoringAgent,
];

/// Public RPC surface plane before certificate-scope binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecuritySurfacePlane {
    Application,
    Administration,
    HighAvailability,
    Monitoring,
}

impl SecuritySurfacePlane {
    pub const fn required_surface(self) -> SecuritySurface {
        match self {
            Self::Application => SecuritySurface::Application,
            Self::Administration => SecuritySurface::Administration,
            Self::HighAvailability => SecuritySurface::Cluster,
            Self::Monitoring => SecuritySurface::MonitoringAgent,
        }
    }

    pub const fn binding(self) -> SurfacePlaneBinding {
        SurfacePlaneBinding {
            plane: self,
            required_surface: self.required_surface(),
        }
    }
}

/// Explicit mapping between a public surface plane and certificate scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfacePlaneBinding {
    pub plane: SecuritySurfacePlane,
    pub required_surface: SecuritySurface,
}

impl SecuritySurface {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Application => SURFACE_ID_APPLICATION,
            Self::Administration => SURFACE_ID_ADMINISTRATION,
            Self::Cluster => SURFACE_ID_CLUSTER,
            Self::BackupAgent => SURFACE_ID_BACKUP_AGENT,
            Self::MonitoringAgent => SURFACE_ID_MONITORING_AGENT,
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        security_surface_from_id(id)
    }

    pub const fn permits_family(self, family: PermissionFamily) -> bool {
        match self {
            Self::Application => matches!(family, PermissionFamily::Application),
            Self::Administration => matches!(
                family,
                PermissionFamily::Definition
                    | PermissionFamily::Diagnostics
                    | PermissionFamily::Security
                    | PermissionFamily::Recovery
            ),
            Self::Cluster => matches!(family, PermissionFamily::Cluster),
            Self::BackupAgent => matches!(
                family,
                PermissionFamily::Recovery | PermissionFamily::Diagnostics
            ),
            Self::MonitoringAgent => matches!(family, PermissionFamily::Diagnostics),
        }
    }

    pub const fn permits_permission(self, permission: Permission) -> bool {
        self.permits_family(permission.family())
    }
}

impl core::fmt::Display for SecuritySurface {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub fn security_surface_from_id(id: &str) -> Option<SecuritySurface> {
    match id {
        SURFACE_ID_APPLICATION => Some(SecuritySurface::Application),
        SURFACE_ID_ADMINISTRATION => Some(SecuritySurface::Administration),
        SURFACE_ID_CLUSTER => Some(SecuritySurface::Cluster),
        SURFACE_ID_BACKUP_AGENT => Some(SecuritySurface::BackupAgent),
        SURFACE_ID_MONITORING_AGENT => Some(SecuritySurface::MonitoringAgent),
        _ => None,
    }
}

pub const fn surface_permits_permission(surface: SecuritySurface, permission: Permission) -> bool {
    surface.permits_permission(permission)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_ids_roundtrip() {
        for surface in ALL_SECURITY_SURFACES {
            assert_eq!(SecuritySurface::from_id(surface.as_str()), Some(surface));
        }
    }

    #[test]
    fn test_surface_plane_bindings_are_explicit() {
        assert_eq!(
            SecuritySurfacePlane::Application.required_surface(),
            SecuritySurface::Application
        );
        assert_eq!(
            SecuritySurfacePlane::Administration.required_surface(),
            SecuritySurface::Administration
        );
        assert_eq!(
            SecuritySurfacePlane::HighAvailability.required_surface(),
            SecuritySurface::Cluster
        );
        assert_eq!(
            SecuritySurfacePlane::Monitoring.required_surface(),
            SecuritySurface::MonitoringAgent
        );
    }

    #[test]
    fn test_application_surface_is_application_only() {
        assert!(SecuritySurface::Application.permits_permission(Permission::ExecuteProcedure));
        assert!(!SecuritySurface::Application.permits_permission(Permission::ManageSecurity));
        assert!(!SecuritySurface::Application.permits_permission(Permission::ClusterPromote));
        assert!(!SecuritySurface::Application.permits_permission(Permission::Restore));
    }

    #[test]
    fn test_agent_and_cluster_surfaces_are_separate() {
        assert!(SecuritySurface::BackupAgent.permits_permission(Permission::Backup));
        assert!(SecuritySurface::BackupAgent.permits_permission(Permission::ReadAudit));
        assert!(!SecuritySurface::BackupAgent.permits_permission(Permission::ManageSecurity));
        assert!(!SecuritySurface::BackupAgent.permits_permission(Permission::ClusterPromote));

        assert!(SecuritySurface::MonitoringAgent.permits_permission(Permission::InspectPlans));
        assert!(!SecuritySurface::MonitoringAgent.permits_permission(Permission::Restore));

        assert!(SecuritySurface::Cluster.permits_permission(Permission::ClusterFenceNode));
        assert!(!SecuritySurface::Cluster.permits_permission(Permission::ManageSecurity));
    }
}
