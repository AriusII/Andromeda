use super::{CertificateFingerprint, SurfaceScope};
use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Lifecycle status for an mTLS certificate identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CertificateIdentityStatus {
    Active,
    Disabled,
    Revoked,
}

impl CertificateIdentityStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Revoked => "revoked",
        }
    }

    pub const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

impl std::fmt::Display for CertificateIdentityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Certificate evidence bound during mTLS admission.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CertificateIdentity {
    fingerprint: CertificateFingerprint,
    subject: String,
    surface_scope: SurfaceScope,
    status: CertificateIdentityStatus,
}

impl CertificateIdentity {
    pub fn new(
        fingerprint_sha256: impl Into<String>,
        subject: impl Into<String>,
        surface_scope: SurfaceScope,
    ) -> AndromedaResult<Self> {
        Self::new_with_status(
            fingerprint_sha256,
            subject,
            surface_scope,
            CertificateIdentityStatus::Active,
        )
    }

    pub fn new_with_status(
        fingerprint_sha256: impl Into<String>,
        subject: impl Into<String>,
        surface_scope: SurfaceScope,
        status: CertificateIdentityStatus,
    ) -> AndromedaResult<Self> {
        let fingerprint = CertificateFingerprint::new(fingerprint_sha256).ok_or_else(|| {
            security_error("certificate identity requires a non-empty fingerprint")
        })?;
        if !fingerprint.is_valid_sha256() {
            return Err(security_error(
                "certificate identity fingerprint must be valid SHA-256 evidence",
            ));
        }

        let subject = subject.into();
        let subject = subject.trim();
        if subject.is_empty() {
            return Err(security_error(
                "certificate identity requires a non-empty subject",
            ));
        }

        Ok(Self {
            fingerprint,
            subject: subject.to_string(),
            surface_scope,
            status,
        })
    }

    pub fn fingerprint(&self) -> &CertificateFingerprint {
        &self.fingerprint
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub const fn surface_scope(&self) -> SurfaceScope {
        self.surface_scope
    }

    pub const fn status(&self) -> CertificateIdentityStatus {
        self.status
    }

    pub const fn is_active(&self) -> bool {
        self.status.is_active()
    }

    pub fn with_status(&self, status: CertificateIdentityStatus) -> Self {
        Self {
            fingerprint: self.fingerprint.clone(),
            subject: self.subject.clone(),
            surface_scope: self.surface_scope,
            status,
        }
    }

    pub fn has_identity_evidence(&self) -> bool {
        !self.fingerprint.is_empty() && !self.subject.trim().is_empty()
    }
}

fn security_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}
