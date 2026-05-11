//! Catalog-backed procedure manifest resolver.
//!
//! [`CatalogBackedProcedureResolver`] provides catalog-grounded procedure
//! manifest resolution against a narrow [`CatalogManifestStore`] trait.
//! Resolution validates that the catalog is durably published, that the
//! requested procedure exists, and that the `ContractHash` and
//! `CatalogVersion` match the caller's expectations before returning a
//! [`ProcedureManifest`].
//!
//! This module does NOT implement [`ProcedureResolver`] — full SRPL plan
//! compilation is deferred to the W2 mission.

use andromeda_catalog_store::ProcedureManifest;
use andromeda_error::{AndromedaError, AndromedaErrorKind};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

/// Narrow read interface for catalog manifest data required by
/// [`CatalogBackedProcedureResolver`].
///
/// Implementors provide durability state, current catalog version, and
/// per-procedure manifest lookup. The trait must not expose WAL replay,
/// mutation, or network-facing surfaces.
pub trait CatalogManifestStore: Send + Sync {
    /// Returns `true` if the catalog has been durably published via WAL.
    ///
    /// A catalog that has not yet been durably published must be rejected at
    /// the resolution boundary to prevent serving stale or uncommitted
    /// manifest data.
    fn is_durably_published(&self) -> bool;

    /// Returns the current catalog version visible to readers.
    fn current_catalog_version(&self) -> CatalogVersion;

    /// Attempts to resolve a procedure manifest by its stable identifier.
    ///
    /// Returns `None` if the procedure is not registered in the catalog.
    fn resolve_procedure_manifest(&self, procedure_id: ProcedureId) -> Option<ProcedureManifest>;
}

/// Error variants produced by [`CatalogBackedProcedureResolver::resolve`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogResolverError {
    /// The catalog has not yet been durably published via WAL.
    ///
    /// Callers must wait for durable publication before dispatching procedures.
    CatalogNotDurablyPublished,

    /// The requested procedure is not registered in the catalog.
    ProcedureNotFound {
        /// The `ProcedureId` that was requested but not found.
        procedure_id: ProcedureId,
    },

    /// The procedure exists in the catalog but its `ContractHash` does not
    /// match the caller's expected hash.
    ///
    /// This indicates a contract version skew between the caller and the
    /// catalog. Callers must refresh their contract reference before retrying.
    ContractHashMismatch {
        /// The `ProcedureId` involved in the mismatch.
        procedure_id: ProcedureId,
        /// Hash the caller expected to find.
        expected: ContractHash,
        /// Hash actually stored in the catalog.
        actual: ContractHash,
    },

    /// The procedure exists in the catalog but its `CatalogVersion` does not
    /// match the caller's expected version.
    ///
    /// This indicates a stale catalog reference. Callers must refresh their
    /// contract reference before retrying.
    CatalogVersionMismatch {
        /// The `ProcedureId` involved in the mismatch.
        procedure_id: ProcedureId,
        /// Catalog version the caller expected to find.
        expected: CatalogVersion,
        /// Catalog version actually stored in the manifest.
        actual: CatalogVersion,
    },
}

impl CatalogResolverError {
    /// Maps the error to its canonical [`AndromedaErrorKind`].
    pub fn kind(&self) -> AndromedaErrorKind {
        match self {
            Self::CatalogNotDurablyPublished => AndromedaErrorKind::Catalog,
            Self::ProcedureNotFound { .. } => AndromedaErrorKind::Catalog,
            Self::ContractHashMismatch { .. } => AndromedaErrorKind::Contract,
            Self::CatalogVersionMismatch { .. } => AndromedaErrorKind::Catalog,
        }
    }

    /// Converts the error into an [`AndromedaError`] with an appropriate
    /// error kind and description.
    pub fn into_andromeda_error(self) -> AndromedaError {
        let kind = self.kind();
        let message = match &self {
            Self::CatalogNotDurablyPublished => {
                "catalog has not been durably published; procedure resolution unavailable"
                    .to_string()
            },
            Self::ProcedureNotFound { procedure_id } => {
                format!("procedure id {} not found in catalog", procedure_id.get())
            },
            Self::ContractHashMismatch {
                procedure_id,
                expected,
                actual,
            } => {
                format!(
                    "procedure id {} contract hash mismatch: expected {:?}, got {:?}",
                    procedure_id.get(),
                    expected.as_bytes(),
                    actual.as_bytes()
                )
            },
            Self::CatalogVersionMismatch {
                procedure_id,
                expected,
                actual,
            } => {
                format!(
                    "procedure id {} catalog version mismatch: expected {}, got {}",
                    procedure_id.get(),
                    expected.get(),
                    actual.get()
                )
            },
        };
        AndromedaError::new(kind, message)
    }
}

/// Catalog-grounded procedure manifest resolver.
///
/// Resolves a [`ProcedureManifest`] for a given `(ProcedureId, ContractHash,
/// CatalogVersion)` triple by querying a [`CatalogManifestStore`] and
/// validating the response against the caller's expectations.
///
/// # Resolution contract
///
/// 1. The catalog must be durably published (`is_durably_published() == true`).
/// 2. The procedure must exist in the catalog.
/// 3. The stored `ContractHash` must match the caller's `expected_hash`.
/// 4. The stored `CatalogVersion` must match the caller's `expected_version`.
///
/// Any violation produces a typed [`CatalogResolverError`] that callers can
/// inspect without parsing error messages.
///
/// # SRPL compilation
///
/// This resolver returns a raw [`ProcedureManifest`] and does not perform SRPL
/// plan compilation. Full [`ProcedureResolver`] integration is deferred to the
/// W2 mission.
pub struct CatalogBackedProcedureResolver<S: CatalogManifestStore> {
    store: S,
}

impl<S: CatalogManifestStore> CatalogBackedProcedureResolver<S> {
    /// Creates a resolver backed by the given catalog manifest store.
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Resolves the manifest for a procedure, validating durability, existence,
    /// contract hash, and catalog version.
    ///
    /// Returns the stored [`ProcedureManifest`] when all checks pass, or a
    /// [`CatalogResolverError`] describing the first violation.
    pub fn resolve(
        &self,
        procedure_id: ProcedureId,
        expected_hash: ContractHash,
        expected_version: CatalogVersion,
    ) -> Result<ProcedureManifest, CatalogResolverError> {
        if !self.store.is_durably_published() {
            return Err(CatalogResolverError::CatalogNotDurablyPublished);
        }

        let manifest = self
            .store
            .resolve_procedure_manifest(procedure_id)
            .ok_or(CatalogResolverError::ProcedureNotFound { procedure_id })?;

        let actual_hash = ContractHash::from_slice(manifest.contract_hash.as_slice())
            .map_err(|_| CatalogResolverError::ProcedureNotFound { procedure_id })?;

        if actual_hash != expected_hash {
            return Err(CatalogResolverError::ContractHashMismatch {
                procedure_id,
                expected: expected_hash,
                actual: actual_hash,
            });
        }

        if manifest.catalog_version != expected_version {
            return Err(CatalogResolverError::CatalogVersionMismatch {
                procedure_id,
                expected: expected_version,
                actual: manifest.catalog_version,
            });
        }

        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROCEDURE_A: ProcedureId = ProcedureId::new(101);
    const PROCEDURE_B: ProcedureId = ProcedureId::new(102);

    fn contract_hash(byte: u8) -> ContractHash {
        ContractHash::new([byte; ContractHash::LEN])
    }

    fn catalog_version(v: u64) -> CatalogVersion {
        CatalogVersion::new(v)
    }

    fn manifest_for(procedure_id: ProcedureId, hash_byte: u8, version: u64) -> ProcedureManifest {
        ProcedureManifest {
            procedure_id,
            qualified_name: format!("public.proc_{}", procedure_id.get()),
            catalog_version: CatalogVersion::new(version),
            contract_hash: vec![hash_byte; ContractHash::LEN],
            input_schema: Vec::new(),
            output_schema: Vec::new(),
            is_mutable: false,
            min_compatible_version: CatalogVersion::new(1),
            srpl_source: None,
            compiled_ir_handle: None,
        }
    }

    struct FakeCatalogStore {
        durably_published: bool,
        manifest: Option<ProcedureManifest>,
    }

    impl CatalogManifestStore for FakeCatalogStore {
        fn is_durably_published(&self) -> bool {
            self.durably_published
        }

        fn current_catalog_version(&self) -> CatalogVersion {
            self.manifest
                .as_ref()
                .map(|m| m.catalog_version)
                .unwrap_or(CatalogVersion::new(0))
        }

        fn resolve_procedure_manifest(
            &self,
            procedure_id: ProcedureId,
        ) -> Option<ProcedureManifest> {
            self.manifest
                .clone()
                .filter(|m| m.procedure_id == procedure_id)
        }
    }

    #[test]
    fn catalog_resolver_happy_path_returns_manifest() {
        let manifest = manifest_for(PROCEDURE_A, 0xAA, 5);
        let store = FakeCatalogStore {
            durably_published: true,
            manifest: Some(manifest.clone()),
        };
        let resolver = CatalogBackedProcedureResolver::new(store);

        let result = resolver
            .resolve(PROCEDURE_A, contract_hash(0xAA), catalog_version(5))
            .expect("happy path must succeed");

        assert_eq!(result.procedure_id, PROCEDURE_A);
        assert_eq!(result.catalog_version, catalog_version(5));
        assert_eq!(result.contract_hash, vec![0xAA_u8; ContractHash::LEN]);
    }

    #[test]
    fn catalog_resolver_rejects_when_catalog_not_durably_published() {
        let manifest = manifest_for(PROCEDURE_A, 0xAA, 5);
        let store = FakeCatalogStore {
            durably_published: false,
            manifest: Some(manifest),
        };
        let resolver = CatalogBackedProcedureResolver::new(store);

        let err = resolver
            .resolve(PROCEDURE_A, contract_hash(0xAA), catalog_version(5))
            .expect_err("non-durable catalog must be rejected");

        assert_eq!(err, CatalogResolverError::CatalogNotDurablyPublished);
        assert_eq!(err.kind(), AndromedaErrorKind::Catalog);
    }

    #[test]
    fn catalog_resolver_rejects_unknown_procedure() {
        let store = FakeCatalogStore {
            durably_published: true,
            manifest: None,
        };
        let resolver = CatalogBackedProcedureResolver::new(store);

        let err = resolver
            .resolve(PROCEDURE_B, contract_hash(0xBB), catalog_version(3))
            .expect_err("unknown procedure must be rejected");

        assert_eq!(
            err,
            CatalogResolverError::ProcedureNotFound {
                procedure_id: PROCEDURE_B
            }
        );
        assert_eq!(err.kind(), AndromedaErrorKind::Catalog);
    }

    #[test]
    fn catalog_resolver_rejects_contract_hash_mismatch() {
        let manifest = manifest_for(PROCEDURE_A, 0xAA, 5);
        let store = FakeCatalogStore {
            durably_published: true,
            manifest: Some(manifest),
        };
        let resolver = CatalogBackedProcedureResolver::new(store);

        let wrong_hash = contract_hash(0xFF);
        let err = resolver
            .resolve(PROCEDURE_A, wrong_hash, catalog_version(5))
            .expect_err("contract hash mismatch must be rejected");

        assert!(matches!(
            err,
            CatalogResolverError::ContractHashMismatch {
                procedure_id: PROCEDURE_A,
                expected,
                actual,
            } if expected == wrong_hash && actual == contract_hash(0xAA)
        ));
        assert_eq!(err.kind(), AndromedaErrorKind::Contract);
    }

    #[test]
    fn catalog_resolver_rejects_catalog_version_mismatch() {
        let manifest = manifest_for(PROCEDURE_A, 0xAA, 5);
        let store = FakeCatalogStore {
            durably_published: true,
            manifest: Some(manifest),
        };
        let resolver = CatalogBackedProcedureResolver::new(store);

        let wrong_version = catalog_version(99);
        let err = resolver
            .resolve(PROCEDURE_A, contract_hash(0xAA), wrong_version)
            .expect_err("catalog version mismatch must be rejected");

        assert!(matches!(
            err,
            CatalogResolverError::CatalogVersionMismatch {
                procedure_id: PROCEDURE_A,
                expected,
                actual,
            } if expected == wrong_version && actual == catalog_version(5)
        ));
        assert_eq!(err.kind(), AndromedaErrorKind::Catalog);
    }
}
