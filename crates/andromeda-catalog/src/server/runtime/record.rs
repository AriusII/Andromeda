use andromeda_core::AndromedaResult;

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
