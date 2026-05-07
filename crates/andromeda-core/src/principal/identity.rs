use super::{
    CertificateFingerprint, Permission, PermissionSet, PrincipalId, PrincipalRole, PrincipalStatus,
    SessionToken,
};
use crate::{AndromedaError, AndromedaErrorKind, AndromedaResult};

/// Principal identity bound to certificate, role, and session token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub id: PrincipalId,
    pub role: PrincipalRole,
    pub status: PrincipalStatus,
    pub session_token: SessionToken,
    pub cert_fingerprint: CertificateFingerprint,
    pub created_at: std::time::SystemTime,
}

pub type UserPrincipal = Principal;

impl Principal {
    pub fn new(
        id: PrincipalId,
        role: PrincipalRole,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
    ) -> Option<Self> {
        Self::try_new(id, role, session_token, cert_fingerprint).ok()
    }

    pub fn try_new(
        id: PrincipalId,
        role: PrincipalRole,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
    ) -> AndromedaResult<Self> {
        Self::try_new_with_status(
            id,
            role,
            PrincipalStatus::Active,
            session_token,
            cert_fingerprint,
        )
    }

    pub fn new_with_timestamp(
        id: PrincipalId,
        role: PrincipalRole,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
        created_at: std::time::SystemTime,
    ) -> Option<Self> {
        Self::try_new_with_timestamp(id, role, session_token, cert_fingerprint, created_at).ok()
    }

    pub fn try_new_with_timestamp(
        id: PrincipalId,
        role: PrincipalRole,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
        created_at: std::time::SystemTime,
    ) -> AndromedaResult<Self> {
        Self::try_new_with_status_and_timestamp(
            id,
            role,
            PrincipalStatus::Active,
            session_token,
            cert_fingerprint,
            created_at,
        )
    }

    pub fn new_with_status(
        id: PrincipalId,
        role: PrincipalRole,
        status: PrincipalStatus,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
    ) -> Option<Self> {
        Self::try_new_with_status(id, role, status, session_token, cert_fingerprint).ok()
    }

    pub fn try_new_with_status(
        id: PrincipalId,
        role: PrincipalRole,
        status: PrincipalStatus,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
    ) -> AndromedaResult<Self> {
        Self::try_new_with_status_and_timestamp(
            id,
            role,
            status,
            session_token,
            cert_fingerprint,
            std::time::SystemTime::now(),
        )
    }

    pub fn new_with_status_and_timestamp(
        id: PrincipalId,
        role: PrincipalRole,
        status: PrincipalStatus,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
        created_at: std::time::SystemTime,
    ) -> Option<Self> {
        Self::try_new_with_status_and_timestamp(
            id,
            role,
            status,
            session_token,
            cert_fingerprint,
            created_at,
        )
        .ok()
    }

    pub fn try_new_with_status_and_timestamp(
        id: PrincipalId,
        role: PrincipalRole,
        status: PrincipalStatus,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
        created_at: std::time::SystemTime,
    ) -> AndromedaResult<Self> {
        if id.is_zero() {
            return Err(security_error("principal id must not be zero"));
        }
        if session_token.is_empty() {
            return Err(security_error(
                "principal session token evidence must not be empty",
            ));
        }
        if cert_fingerprint.is_empty() {
            return Err(security_error(
                "principal certificate fingerprint evidence must not be empty",
            ));
        }

        Ok(Self {
            id,
            role,
            status,
            session_token,
            cert_fingerprint,
            created_at,
        })
    }

    pub fn permissions(&self) -> PermissionSet {
        if self.is_active() {
            self.role.permissions()
        } else {
            PermissionSet::new()
        }
    }

    pub fn has_permission(&self, required: &Permission) -> bool {
        self.is_active() && self.permissions().has_permission(required)
    }

    pub const fn is_active(&self) -> bool {
        self.status.is_active()
    }

    pub fn with_status(&self, status: PrincipalStatus) -> Self {
        let mut principal = self.clone();
        principal.status = status;
        principal
    }

    pub fn masked_display(&self) -> String {
        let fingerprint = self.cert_fingerprint.as_str();
        let cert_prefix = if let Some((end, _)) = fingerprint.char_indices().nth(6) {
            &fingerprint[..end]
        } else {
            "***"
        };

        format!(
            "Principal{{id: {}, role: {} ({:?}), status: {}, cert: {}***}}",
            self.id, self.role, self.role, self.status, cert_prefix
        )
    }

    /// Build principal from certificate CN + SHA-256 fingerprint.
    pub fn from_certificate_fields(
        subject_cn: &str,
        fingerprint_sha256: &str,
    ) -> crate::AndromedaResult<Self> {
        let cn = subject_cn.trim();
        if cn.is_empty() {
            return Err(crate::AndromedaError::new(
                crate::AndromedaErrorKind::Security,
                "certificate must have non-empty subject CN",
            ));
        }

        let fingerprint = CertificateFingerprint::new(fingerprint_sha256).ok_or_else(|| {
            crate::AndromedaError::new(
                crate::AndromedaErrorKind::Security,
                "certificate fingerprint invalid or empty",
            )
        })?;

        if !fingerprint.is_valid_sha256() {
            return Err(crate::AndromedaError::new(
                crate::AndromedaErrorKind::Security,
                "certificate fingerprint must be valid SHA-256 (64 hex chars)",
            ));
        }

        let session_token = SessionToken::from_certificate_fingerprint(&fingerprint);
        let principal_id = PrincipalId::from_certificate_fingerprint(&fingerprint)?;

        Self::try_new(
            principal_id,
            PrincipalRole::User,
            session_token,
            fingerprint,
        )
    }
}

fn security_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}
