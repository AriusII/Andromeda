use andromeda_error::AndromedaResult;

use crate::ProcedureManifest;

/// Runtime metadata attached to a resolved manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CatalogManifestRuntimeMetadata {
    pub source_generator_ready: bool,
    pub permission_granted: bool,
}

impl CatalogManifestRuntimeMetadata {
    pub const fn ready() -> Self {
        Self {
            source_generator_ready: true,
            permission_granted: true,
        }
    }
}

impl Default for CatalogManifestRuntimeMetadata {
    fn default() -> Self {
        Self::ready()
    }
}

/// Manifest plus runtime gates read from a catalog-backed store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestRecord {
    pub manifest: ProcedureManifest,
    pub metadata: CatalogManifestRuntimeMetadata,
}

impl CatalogManifestRecord {
    pub fn new(
        manifest: ProcedureManifest,
        metadata: CatalogManifestRuntimeMetadata,
    ) -> AndromedaResult<Self> {
        manifest.validate()?;
        Ok(Self { manifest, metadata })
    }
}

/// Protocol-free catalog manifest resolution status.
///
/// Generated Protobuf adapters live in proto/RPC owners. This internal status
/// type intentionally has no unspecified variant.
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

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

    #[test]
    fn manifest_record_validates_manifest_payload() {
        let manifest = ProcedureManifest {
            procedure_id: ProcedureId::new(1),
            qualified_name: "inventory.reserve_stock".to_string(),
            catalog_version: CatalogVersion::new(3),
            contract_hash: vec![0xA5; ContractHash::LEN],
            input_schema: Vec::new(),
            output_schema: Vec::new(),
            is_mutable: false,
            min_compatible_version: CatalogVersion::new(1),
        };

        let record = CatalogManifestRecord::new(
            manifest.clone(),
            CatalogManifestRuntimeMetadata {
                source_generator_ready: false,
                permission_granted: true,
            },
        )
        .unwrap();

        assert_eq!(record.manifest, manifest);
        assert!(!record.metadata.source_generator_ready);
        assert!(record.metadata.permission_granted);
    }

    #[test]
    fn runtime_status_exposes_protocol_free_diagnostic_codes() {
        assert_eq!(
            CatalogManifestResolutionStatus::NotSourceGeneratorReady.diagnostic_code(),
            "STATUS_NOT_SOURCE_GENERATOR_READY"
        );
        assert_eq!(
            CatalogManifestResolutionStatus::AuthRequired.diagnostic_code(),
            "STATUS_AUTH_REQUIRED"
        );
    }
}
