use andromeda_core::{AndromedaErrorKind, AndromedaResult};
use andromeda_observe::CertificateIdentity;

use crate::{SurfacePlane, mtls_identity::plane_to_required_surface_scope};

use super::pool_error;

/// Connection pool key: a server certificate identity scoped to one surface plane.
///
/// The runtime may have different socket addresses for the same logical server,
/// but pooling is keyed by authenticated server fingerprint and surface plane.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectionPoolKey {
    server_fingerprint: String,
    plane: SurfacePlane,
}

impl ConnectionPoolKey {
    pub fn new(
        server_fingerprint: impl Into<String>,
        plane: SurfacePlane,
    ) -> AndromedaResult<Self> {
        let key = Self {
            server_fingerprint: server_fingerprint.into(),
            plane,
        };
        key.validate()?;
        Ok(key)
    }

    pub fn from_server_identity(
        identity: &CertificateIdentity,
        plane: SurfacePlane,
    ) -> AndromedaResult<Self> {
        let required_scope = plane_to_required_surface_scope(plane);
        if identity.surface != required_scope {
            return Err(pool_error(
                AndromedaErrorKind::Security,
                "server identity surface scope does not match pool plane",
            ));
        }
        Self::new(identity.fingerprint.clone(), plane)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        if self.server_fingerprint.trim().is_empty() {
            return Err(pool_error(
                AndromedaErrorKind::Security,
                "server identity fingerprint cannot be empty",
            ));
        }
        Ok(())
    }

    pub fn server_fingerprint(&self) -> &str {
        &self.server_fingerprint
    }

    pub const fn plane(&self) -> SurfacePlane {
        self.plane
    }
}
