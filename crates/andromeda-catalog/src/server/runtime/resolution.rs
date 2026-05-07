use andromeda_error::{AndromedaError, AndromedaErrorKind};
use andromeda_types::{CatalogVersion, ContractHash};

use crate::ProcedureManifest;

use super::status::CatalogManifestResolutionStatus;

/// Failure details returned by the runtime resolver before projection to the
/// governed proto catalog status model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogManifestResolutionFailure {
    NotFound,
    ContractHashMismatch {
        expected: ContractHash,
        actual: ContractHash,
    },
    CatalogVersionMismatch {
        expected: CatalogVersion,
        actual: CatalogVersion,
    },
    NotSourceGeneratorReady,
    PermissionDenied,
    CatalogNotReady,
    Malformed(String),
    Internal(String),
}

impl CatalogManifestResolutionFailure {
    pub fn status(&self) -> CatalogManifestResolutionStatus {
        match self {
            Self::NotFound => CatalogManifestResolutionStatus::NotFound,
            Self::ContractHashMismatch { .. } => {
                CatalogManifestResolutionStatus::ContractHashMismatch
            }
            Self::CatalogVersionMismatch { .. } => {
                CatalogManifestResolutionStatus::CatalogVersionMismatch
            }
            Self::NotSourceGeneratorReady => {
                CatalogManifestResolutionStatus::NotSourceGeneratorReady
            }
            Self::PermissionDenied => CatalogManifestResolutionStatus::PermissionDenied,
            Self::CatalogNotReady => CatalogManifestResolutionStatus::CatalogNotReady,
            Self::Malformed(_) => CatalogManifestResolutionStatus::Malformed,
            Self::Internal(_) => CatalogManifestResolutionStatus::Internal,
        }
    }

    pub fn diagnostic_code(&self) -> &'static str {
        self.status().diagnostic_code()
    }
}

/// Resolver outcome with proto catalog status mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestResolution {
    pub status: CatalogManifestResolutionStatus,
    pub manifest: Option<ProcedureManifest>,
    pub current_catalog_version: CatalogVersion,
    pub diagnostic_code: Option<&'static str>,
    pub failure: Option<CatalogManifestResolutionFailure>,
}

impl CatalogManifestResolution {
    pub(super) fn resolved(
        manifest: ProcedureManifest,
        current_catalog_version: CatalogVersion,
    ) -> Self {
        Self {
            status: CatalogManifestResolutionStatus::Resolved,
            manifest: Some(manifest),
            current_catalog_version,
            diagnostic_code: None,
            failure: None,
        }
    }

    pub(super) fn failed(
        failure: CatalogManifestResolutionFailure,
        current_catalog_version: CatalogVersion,
    ) -> Self {
        Self {
            status: failure.status(),
            manifest: None,
            current_catalog_version,
            diagnostic_code: Some(failure.diagnostic_code()),
            failure: Some(failure),
        }
    }

    pub fn is_resolved(&self) -> bool {
        self.status == CatalogManifestResolutionStatus::Resolved
    }
}

pub(super) fn andromeda_error_from_resolution(
    resolution: &CatalogManifestResolution,
) -> AndromedaError {
    let kind = match resolution.status {
        CatalogManifestResolutionStatus::ContractHashMismatch
        | CatalogManifestResolutionStatus::Malformed => AndromedaErrorKind::Contract,
        CatalogManifestResolutionStatus::PermissionDenied
        | CatalogManifestResolutionStatus::AuthRequired => AndromedaErrorKind::Security,
        CatalogManifestResolutionStatus::Internal => AndromedaErrorKind::Internal,
        _ => AndromedaErrorKind::Catalog,
    };
    let diagnostic = resolution
        .diagnostic_code
        .unwrap_or_else(|| resolution.status.diagnostic_code());
    AndromedaError::new(
        kind,
        format!("catalog manifest resolution failed: {diagnostic}"),
    )
}
