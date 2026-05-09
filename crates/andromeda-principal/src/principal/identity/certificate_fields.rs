use super::*;
use andromeda_error::AndromedaResult;

impl Principal {
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
    ) -> AndromedaResult<Self> {
        let cn = subject_cn.trim();
        if cn.is_empty() {
            return Err(security_error("certificate must have non-empty subject CN"));
        }

        let fingerprint = CertificateFingerprint::new(fingerprint_sha256)
            .ok_or_else(|| security_error("certificate fingerprint invalid or empty"))?;

        if !fingerprint.is_valid_sha256() {
            return Err(security_error(
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
