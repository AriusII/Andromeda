use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_proto::generated::contract::v1::catalog_procedure_manifest_resolution_response::Status as ProtoCatalogManifestResolutionStatus;

/// Catalog-owned manifest resolution status.
///
/// The generated Protobuf enum is accepted only at the boundary. This internal
/// status type intentionally has no `Unspecified` variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogManifestResolutionStatus {
    Resolved,
    NotFound,
    CatalogVersionMismatch,
    ContractHashMismatch,
    NotSourceGeneratorReady,
    PermissionDenied,
    Unsupported,
    Malformed,
    Internal,
    CatalogNotReady,
    AuthRequired,
}

impl CatalogManifestResolutionStatus {
    pub const fn to_proto(self) -> ProtoCatalogManifestResolutionStatus {
        match self {
            Self::Resolved => ProtoCatalogManifestResolutionStatus::Resolved,
            Self::NotFound => ProtoCatalogManifestResolutionStatus::NotFound,
            Self::CatalogVersionMismatch => {
                ProtoCatalogManifestResolutionStatus::CatalogVersionMismatch
            }
            Self::ContractHashMismatch => {
                ProtoCatalogManifestResolutionStatus::ContractHashMismatch
            }
            Self::NotSourceGeneratorReady => {
                ProtoCatalogManifestResolutionStatus::NotSourceGeneratorReady
            }
            Self::PermissionDenied => ProtoCatalogManifestResolutionStatus::PermissionDenied,
            Self::Unsupported => ProtoCatalogManifestResolutionStatus::Unsupported,
            Self::Malformed => ProtoCatalogManifestResolutionStatus::Malformed,
            Self::Internal => ProtoCatalogManifestResolutionStatus::Internal,
            Self::CatalogNotReady => ProtoCatalogManifestResolutionStatus::CatalogNotReady,
            Self::AuthRequired => ProtoCatalogManifestResolutionStatus::AuthRequired,
        }
    }

    pub fn from_proto(status: ProtoCatalogManifestResolutionStatus) -> AndromedaResult<Self> {
        match status {
            ProtoCatalogManifestResolutionStatus::Resolved => Ok(Self::Resolved),
            ProtoCatalogManifestResolutionStatus::NotFound => Ok(Self::NotFound),
            ProtoCatalogManifestResolutionStatus::CatalogVersionMismatch => {
                Ok(Self::CatalogVersionMismatch)
            }
            ProtoCatalogManifestResolutionStatus::ContractHashMismatch => {
                Ok(Self::ContractHashMismatch)
            }
            ProtoCatalogManifestResolutionStatus::NotSourceGeneratorReady => {
                Ok(Self::NotSourceGeneratorReady)
            }
            ProtoCatalogManifestResolutionStatus::PermissionDenied => Ok(Self::PermissionDenied),
            ProtoCatalogManifestResolutionStatus::Unsupported => Ok(Self::Unsupported),
            ProtoCatalogManifestResolutionStatus::Malformed => Ok(Self::Malformed),
            ProtoCatalogManifestResolutionStatus::Internal => Ok(Self::Internal),
            ProtoCatalogManifestResolutionStatus::CatalogNotReady => Ok(Self::CatalogNotReady),
            ProtoCatalogManifestResolutionStatus::AuthRequired => Ok(Self::AuthRequired),
            ProtoCatalogManifestResolutionStatus::Unspecified => Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "catalog manifest resolution status must be specified",
            )),
        }
    }

    pub fn from_proto_i32(status: i32) -> AndromedaResult<Self> {
        match status {
            value if value == ProtoCatalogManifestResolutionStatus::Resolved as i32 => {
                Ok(Self::Resolved)
            }
            value if value == ProtoCatalogManifestResolutionStatus::NotFound as i32 => {
                Ok(Self::NotFound)
            }
            value
                if value == ProtoCatalogManifestResolutionStatus::CatalogVersionMismatch as i32 =>
            {
                Ok(Self::CatalogVersionMismatch)
            }
            value if value == ProtoCatalogManifestResolutionStatus::ContractHashMismatch as i32 => {
                Ok(Self::ContractHashMismatch)
            }
            value
                if value
                    == ProtoCatalogManifestResolutionStatus::NotSourceGeneratorReady as i32 =>
            {
                Ok(Self::NotSourceGeneratorReady)
            }
            value if value == ProtoCatalogManifestResolutionStatus::PermissionDenied as i32 => {
                Ok(Self::PermissionDenied)
            }
            value if value == ProtoCatalogManifestResolutionStatus::Unsupported as i32 => {
                Ok(Self::Unsupported)
            }
            value if value == ProtoCatalogManifestResolutionStatus::Malformed as i32 => {
                Ok(Self::Malformed)
            }
            value if value == ProtoCatalogManifestResolutionStatus::Internal as i32 => {
                Ok(Self::Internal)
            }
            value if value == ProtoCatalogManifestResolutionStatus::CatalogNotReady as i32 => {
                Ok(Self::CatalogNotReady)
            }
            value if value == ProtoCatalogManifestResolutionStatus::AuthRequired as i32 => {
                Ok(Self::AuthRequired)
            }
            value if value == ProtoCatalogManifestResolutionStatus::Unspecified as i32 => {
                Err(AndromedaError::new(
                    AndromedaErrorKind::Protocol,
                    "catalog manifest resolution status must be specified",
                ))
            }
            _ => Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "unknown catalog manifest resolution status",
            )),
        }
    }

    pub const fn diagnostic_code(self) -> &'static str {
        match self {
            Self::Resolved => "STATUS_RESOLVED",
            Self::NotFound => "STATUS_NOT_FOUND",
            Self::CatalogVersionMismatch => "STATUS_CATALOG_VERSION_MISMATCH",
            Self::ContractHashMismatch => "STATUS_CONTRACT_HASH_MISMATCH",
            Self::NotSourceGeneratorReady => "STATUS_NOT_SOURCE_GENERATOR_READY",
            Self::PermissionDenied => "STATUS_PERMISSION_DENIED",
            Self::Unsupported => "STATUS_UNSUPPORTED",
            Self::Malformed => "STATUS_MALFORMED",
            Self::Internal => "STATUS_INTERNAL",
            Self::CatalogNotReady => "STATUS_CATALOG_NOT_READY",
            Self::AuthRequired => "STATUS_AUTH_REQUIRED",
        }
    }
}
