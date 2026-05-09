use andromeda_catalog_store::{CatalogManifestResolutionStatus, ProcedureManifest};
use andromeda_error::{AndromedaError, AndromedaErrorKind};
use andromeda_types::CatalogVersion;

pub use andromeda_catalog_store::CatalogManifestResolutionFailure;

/// Resolver outcome with a protocol-free catalog-store status.
pub type CatalogManifestResolution =
    andromeda_catalog_store::CatalogManifestResolution<CatalogManifestResolutionStatus>;

pub(super) fn resolved(
    manifest: ProcedureManifest,
    current_catalog_version: CatalogVersion,
) -> CatalogManifestResolution {
    CatalogManifestResolution::resolved(
        manifest,
        current_catalog_version,
        CatalogManifestResolutionStatus::Resolved,
    )
}

pub(super) fn failed(
    failure: CatalogManifestResolutionFailure,
    current_catalog_version: CatalogVersion,
) -> CatalogManifestResolution {
    let status = status_from_failure(&failure);
    CatalogManifestResolution::failed(failure, current_catalog_version, status)
}

fn status_from_failure(
    failure: &CatalogManifestResolutionFailure,
) -> CatalogManifestResolutionStatus {
    match failure {
        CatalogManifestResolutionFailure::NotFound => CatalogManifestResolutionStatus::NotFound,
        CatalogManifestResolutionFailure::ContractHashMismatch { .. } => {
            CatalogManifestResolutionStatus::ContractHashMismatch
        },
        CatalogManifestResolutionFailure::CatalogVersionMismatch { .. } => {
            CatalogManifestResolutionStatus::CatalogVersionMismatch
        },
        CatalogManifestResolutionFailure::NotSourceGeneratorReady => {
            CatalogManifestResolutionStatus::NotSourceGeneratorReady
        },
        CatalogManifestResolutionFailure::PermissionDenied => {
            CatalogManifestResolutionStatus::PermissionDenied
        },
        CatalogManifestResolutionFailure::CatalogNotReady => {
            CatalogManifestResolutionStatus::CatalogNotReady
        },
        CatalogManifestResolutionFailure::Malformed(_) => {
            CatalogManifestResolutionStatus::Malformed
        },
        CatalogManifestResolutionFailure::Internal(_) => CatalogManifestResolutionStatus::Internal,
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
