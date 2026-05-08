use andromeda_core::SurfaceScope;
use andromeda_quic::{SurfacePlane, mtls_identity::plane_to_required_surface_scope};

/// Explicit QUIC runtime surface selection for Quinn endpoint wiring.
///
/// The concrete Quinn runtime binds peer certificate identity extraction to
/// the selected transport plane. Callers must pick the plane at construction
/// time instead of inheriting an implicit Application surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QuinnRuntimeSurface {
    plane: SurfacePlane,
    required_scope: SurfaceScope,
}

impl QuinnRuntimeSurface {
    /// Builds a runtime surface from the transport plane.
    pub const fn for_plane(plane: SurfacePlane) -> Self {
        Self {
            plane,
            required_scope: plane_to_required_surface_scope(plane),
        }
    }

    /// Returns the selected QUIC transport plane.
    pub const fn plane(self) -> SurfacePlane {
        self.plane
    }

    /// Returns the certificate scope required for peers on this plane.
    pub const fn required_scope(self) -> SurfaceScope {
        self.required_scope
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_surface_derives_required_scope_from_plane() {
        let app = QuinnRuntimeSurface::for_plane(SurfacePlane::Application);
        let admin = QuinnRuntimeSurface::for_plane(SurfacePlane::Administration);
        let hadr = QuinnRuntimeSurface::for_plane(SurfacePlane::HighAvailability);
        let monitoring = QuinnRuntimeSurface::for_plane(SurfacePlane::Monitoring);

        assert_eq!(app.required_scope(), SurfaceScope::Application);
        assert_eq!(admin.required_scope(), SurfaceScope::Administration);
        assert_eq!(hadr.required_scope(), SurfaceScope::Cluster);
        assert_eq!(monitoring.required_scope(), SurfaceScope::MonitoringAgent);
    }
}
