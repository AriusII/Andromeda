use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenialAuditReason {
    NoPermissionsGranted,
    PermissionNotGranted,
    ProcedureIdMismatch,
    SuperAdminOperationNotAudited,
    WildcardDeniedByPolicy,
    SessionExpired,
    CertificateRevoked,
    InternalError,
}

impl DenialAuditReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoPermissionsGranted => "no_permissions_granted",
            Self::PermissionNotGranted => "permission_not_granted",
            Self::ProcedureIdMismatch => "procedure_id_mismatch",
            Self::SuperAdminOperationNotAudited => "superadmin_operation_not_audited",
            Self::WildcardDeniedByPolicy => "wildcard_denied_by_policy",
            Self::SessionExpired => "session_expired",
            Self::CertificateRevoked => "certificate_revoked",
            Self::InternalError => "internal_error",
        }
    }

    pub fn explanation(self) -> String {
        match self {
            Self::NoPermissionsGranted => "principal has no permissions granted".to_string(),
            Self::PermissionNotGranted => {
                "required permission is not in principal's permission set".to_string()
            }
            Self::ProcedureIdMismatch => {
                "requested procedure ID does not match granted procedure".to_string()
            }
            Self::SuperAdminOperationNotAudited => {
                "super-admin operation attempted without explicit authorization audit record"
                    .to_string()
            }
            Self::WildcardDeniedByPolicy => {
                "wildcard permission was explicitly denied by policy".to_string()
            }
            Self::SessionExpired => "principal's session has expired".to_string(),
            Self::CertificateRevoked => "certificate revocation check failed".to_string(),
            Self::InternalError => "internal error during permission evaluation".to_string(),
        }
    }
}

impl fmt::Display for DenialAuditReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
