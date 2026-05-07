use super::*;
use crate::AndromedaResult;

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
        created_at: SystemTime,
    ) -> Option<Self> {
        Self::try_new_with_timestamp(id, role, session_token, cert_fingerprint, created_at).ok()
    }

    pub fn try_new_with_timestamp(
        id: PrincipalId,
        role: PrincipalRole,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
        created_at: SystemTime,
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
            SystemTime::now(),
        )
    }

    fn try_new_with_status_and_timestamp(
        id: PrincipalId,
        role: PrincipalRole,
        status: PrincipalStatus,
        session_token: SessionToken,
        cert_fingerprint: CertificateFingerprint,
        created_at: SystemTime,
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
}
