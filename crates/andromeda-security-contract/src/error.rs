use core::fmt;

/// Contract-level error for invalid public security metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecurityContractError {
    UnknownSecuritySurface,
    UnknownPermissionFamily,
    UnknownPermissionId,
    InvalidEvidenceSchemaVersion,
    MissingPolicyVersion,
}

impl SecurityContractError {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnknownSecuritySurface => "unknown_security_surface",
            Self::UnknownPermissionFamily => "unknown_permission_family",
            Self::UnknownPermissionId => "unknown_permission_id",
            Self::InvalidEvidenceSchemaVersion => "invalid_evidence_schema_version",
            Self::MissingPolicyVersion => "missing_policy_version",
        }
    }
}

impl fmt::Display for SecurityContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::error::Error for SecurityContractError {}
