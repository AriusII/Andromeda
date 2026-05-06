use super::{
    CertificateFingerprint, Permission, PermissionSet, PrincipalId, PrincipalRole, SessionToken,
};

/// Principal identity bound to certificate, role, and session token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub id: PrincipalId,
    pub role: PrincipalRole,
    pub session_token: SessionToken,
    pub cert_fingerprint: CertificateFingerprint,
    pub created_at: std::time::SystemTime,
}

impl Principal {
    pub fn new(
        id: PrincipalId,
        role: PrincipalRole,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
    ) -> Option<Self> {
        if id.is_zero() || session_token.is_empty() || cert_fingerprint.is_empty() {
            return None;
        }

        Some(Self {
            id,
            role,
            session_token,
            cert_fingerprint,
            created_at: std::time::SystemTime::now(),
        })
    }

    pub fn new_with_timestamp(
        id: PrincipalId,
        role: PrincipalRole,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
        created_at: std::time::SystemTime,
    ) -> Option<Self> {
        if id.is_zero() || session_token.is_empty() || cert_fingerprint.is_empty() {
            return None;
        }

        Some(Self {
            id,
            role,
            session_token,
            cert_fingerprint,
            created_at,
        })
    }

    pub fn permissions(&self) -> PermissionSet {
        self.role.permissions()
    }

    pub fn has_permission(&self, required: &Permission) -> bool {
        self.permissions().has_permission(required)
    }

    pub fn masked_display(&self) -> String {
        format!(
            "Principal{{id: {}, role: {} ({:?}), cert: {}***}}",
            self.id,
            self.role,
            self.role,
            if self.cert_fingerprint.len() > 6 {
                &self.cert_fingerprint.as_str()[..6]
            } else {
                "***"
            }
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

        Self::new(
            principal_id,
            PrincipalRole::User,
            session_token,
            fingerprint,
        )
        .ok_or_else(|| {
            crate::AndromedaError::new(
                crate::AndromedaErrorKind::Security,
                "principal creation failed: invariant violation",
            )
        })
    }
}
