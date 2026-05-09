mod access;
mod certificate_fields;
mod construction;

use super::{
    CertificateFingerprint, Permission, PermissionSet, PrincipalId, PrincipalRole, PrincipalStatus,
    SessionToken,
};
use andromeda_error::{AndromedaError, AndromedaErrorKind};
use std::time::SystemTime;

/// Principal identity bound to certificate, role, and session token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub id: PrincipalId,
    pub role: PrincipalRole,
    pub status: PrincipalStatus,
    pub session_token: SessionToken,
    pub cert_fingerprint: CertificateFingerprint,
    pub created_at: SystemTime,
}

pub type UserPrincipal = Principal;

fn security_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Security, message)
}
