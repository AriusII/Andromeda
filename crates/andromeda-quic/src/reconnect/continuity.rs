use andromeda_core::{AndromedaErrorKind, AndromedaResult};
use andromeda_observe::CertificateIdentity;

use crate::{SurfacePlane, mtls_identity::validate_fingerprint};

use super::{ConnectionPoolKey, pool_error};

/// Runtime-free certificate continuity policy for reconnect handshakes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CertificateContinuityPolicy {
    pub allow_declared_rotation: bool,
}

impl CertificateContinuityPolicy {
    pub const fn strict() -> Self {
        Self {
            allow_declared_rotation: false,
        }
    }

    pub const fn allow_declared_rotation() -> Self {
        Self {
            allow_declared_rotation: true,
        }
    }

    pub fn validate_reconnect(
        self,
        previous_key: &ConnectionPoolKey,
        presented_identity: &CertificateIdentity,
        plane: SurfacePlane,
        declared_rotation: Option<&CertificateRotationDeclaration>,
    ) -> AndromedaResult<CertificateContinuityDecision> {
        if previous_key.plane() != plane {
            return Err(pool_error(
                AndromedaErrorKind::Security,
                "certificate continuity plane does not match previous pool key",
            ));
        }

        validate_fingerprint(previous_key.server_fingerprint())?;
        validate_fingerprint(&presented_identity.fingerprint)?;

        let presented_key = ConnectionPoolKey::from_server_identity(presented_identity, plane)?;
        if previous_key == &presented_key {
            return Ok(CertificateContinuityDecision::AcceptSameFingerprint);
        }

        let Some(rotation) = declared_rotation else {
            return Err(pool_error(
                AndromedaErrorKind::Security,
                "server certificate fingerprint changed without declared rotation",
            ));
        };

        if !self.allow_declared_rotation {
            return Err(pool_error(
                AndromedaErrorKind::Security,
                "certificate rotation is not allowed by reconnect policy",
            ));
        }

        rotation.validate()?;
        if rotation.matches(previous_key, &presented_key) {
            Ok(CertificateContinuityDecision::AcceptDeclaredRotation)
        } else {
            Err(pool_error(
                AndromedaErrorKind::Security,
                "declared certificate rotation does not match reconnect identities",
            ))
        }
    }
}

impl Default for CertificateContinuityPolicy {
    fn default() -> Self {
        Self::strict()
    }
}

/// Explicit server certificate rotation declaration for one surface plane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateRotationDeclaration {
    from_fingerprint: String,
    to_fingerprint: String,
    plane: SurfacePlane,
}

impl CertificateRotationDeclaration {
    pub fn new(
        from_fingerprint: impl Into<String>,
        to_fingerprint: impl Into<String>,
        plane: SurfacePlane,
    ) -> AndromedaResult<Self> {
        let declaration = Self {
            from_fingerprint: from_fingerprint.into(),
            to_fingerprint: to_fingerprint.into(),
            plane,
        };
        declaration.validate()?;
        Ok(declaration)
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        validate_fingerprint(&self.from_fingerprint)?;
        validate_fingerprint(&self.to_fingerprint)?;
        if self.from_fingerprint == self.to_fingerprint {
            return Err(pool_error(
                AndromedaErrorKind::Security,
                "certificate rotation requires different fingerprints",
            ));
        }
        Ok(())
    }

    pub fn matches(
        &self,
        previous_key: &ConnectionPoolKey,
        presented_key: &ConnectionPoolKey,
    ) -> bool {
        self.plane == previous_key.plane()
            && self.plane == presented_key.plane()
            && self.from_fingerprint == previous_key.server_fingerprint()
            && self.to_fingerprint == presented_key.server_fingerprint()
    }

    pub fn source_fingerprint(&self) -> &str {
        &self.from_fingerprint
    }

    pub fn target_fingerprint(&self) -> &str {
        &self.to_fingerprint
    }

    pub const fn plane(&self) -> SurfacePlane {
        self.plane
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateContinuityDecision {
    AcceptSameFingerprint,
    AcceptDeclaredRotation,
}
