//! Procedure manifest resolution DTOs for catalog-store consumers.
//!
//! This module intentionally avoids protobuf dependencies. Protocol adapters
//! live in proto and RPC owner crates.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use crate::{ProcedureManifest, QualifiedName};

/// Selector accepted by the catalog server manifest resolver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogManifestSelector {
    ProcedureId(ProcedureId),
    QualifiedName(QualifiedName),
}

impl CatalogManifestSelector {
    fn validate(&self) -> AndromedaResult<()> {
        match self {
            Self::ProcedureId(procedure_id) if procedure_id.get() == 0 => Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "catalog manifest selector procedure id must not be zero",
            )),
            Self::ProcedureId(_) | Self::QualifiedName(_) => Ok(()),
        }
    }
}

/// Runtime request for resolving a procedure manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestResolutionRequest {
    pub selector: CatalogManifestSelector,
    pub expected_contract_hash: Option<ContractHash>,
    pub expected_catalog_version: Option<CatalogVersion>,
    pub require_source_generator_ready: bool,
}

impl CatalogManifestResolutionRequest {
    pub fn by_id(procedure_id: ProcedureId) -> Self {
        Self {
            selector: CatalogManifestSelector::ProcedureId(procedure_id),
            expected_contract_hash: None,
            expected_catalog_version: None,
            require_source_generator_ready: false,
        }
    }

    pub fn by_qualified_name(name: QualifiedName) -> Self {
        Self {
            selector: CatalogManifestSelector::QualifiedName(name),
            expected_contract_hash: None,
            expected_catalog_version: None,
            require_source_generator_ready: false,
        }
    }

    pub fn by_name(name: &str) -> AndromedaResult<Self> {
        Ok(Self::by_qualified_name(QualifiedName::parse(name)?))
    }

    pub fn with_expected_contract_hash(mut self, contract_hash: ContractHash) -> Self {
        self.expected_contract_hash = Some(contract_hash);
        self
    }

    pub fn with_expected_catalog_version(mut self, catalog_version: CatalogVersion) -> Self {
        self.expected_catalog_version = Some(catalog_version);
        self
    }

    pub fn requiring_source_generator_ready(mut self) -> Self {
        self.require_source_generator_ready = true;
        self
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.selector.validate()?;

        if let Some(contract_hash) = self.expected_contract_hash
            && contract_hash.is_zero()
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "catalog manifest expected contract hash must not be zero",
            ));
        }

        if let Some(catalog_version) = self.expected_catalog_version
            && catalog_version.get() == 0
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "catalog manifest expected catalog version must not be zero",
            ));
        }

        Ok(())
    }
}

/// Failure details returned by the runtime resolver before projection to a
/// protocol status model.
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
    pub const fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::NotFound => "STATUS_NOT_FOUND",
            Self::ContractHashMismatch { .. } => "STATUS_CONTRACT_HASH_MISMATCH",
            Self::CatalogVersionMismatch { .. } => "STATUS_CATALOG_VERSION_MISMATCH",
            Self::NotSourceGeneratorReady => "STATUS_NOT_SOURCE_GENERATOR_READY",
            Self::PermissionDenied => "STATUS_PERMISSION_DENIED",
            Self::CatalogNotReady => "STATUS_CATALOG_NOT_READY",
            Self::Malformed(_) => "STATUS_MALFORMED",
            Self::Internal(_) => "STATUS_INTERNAL",
        }
    }
}

/// Resolver outcome with a caller-owned status enum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogManifestResolution<Status> {
    pub status: Status,
    pub manifest: Option<ProcedureManifest>,
    pub current_catalog_version: CatalogVersion,
    pub diagnostic_code: Option<&'static str>,
    pub failure: Option<CatalogManifestResolutionFailure>,
}

impl<Status> CatalogManifestResolution<Status> {
    pub fn resolved(
        manifest: ProcedureManifest,
        current_catalog_version: CatalogVersion,
        status: Status,
    ) -> Self {
        Self {
            status,
            manifest: Some(manifest),
            current_catalog_version,
            diagnostic_code: None,
            failure: None,
        }
    }

    pub fn failed(
        failure: CatalogManifestResolutionFailure,
        current_catalog_version: CatalogVersion,
        status: Status,
    ) -> Self {
        Self {
            status,
            manifest: None,
            current_catalog_version,
            diagnostic_code: Some(failure.diagnostic_code()),
            failure: Some(failure),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestResolutionStatus {
        Resolved,
        NotFound,
    }

    #[test]
    fn request_validation_rejects_zero_expected_contract_hash() {
        let request = CatalogManifestResolutionRequest::by_id(ProcedureId::new(1))
            .with_expected_contract_hash(ContractHash::zero());

        assert_eq!(
            request.validate().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }

    #[test]
    fn failed_resolution_uses_failure_diagnostic_code() {
        let resolution = CatalogManifestResolution::failed(
            CatalogManifestResolutionFailure::NotFound,
            CatalogVersion::new(7),
            TestResolutionStatus::NotFound,
        );

        assert_eq!(resolution.status, TestResolutionStatus::NotFound);
        assert_eq!(resolution.diagnostic_code, Some("STATUS_NOT_FOUND"));
        assert!(resolution.manifest.is_none());
    }

    #[test]
    fn resolved_resolution_carries_manifest_without_failure() {
        let manifest = ProcedureManifest {
            procedure_id: ProcedureId::new(1),
            qualified_name: "public.my_proc".to_string(),
            catalog_version: CatalogVersion::new(2),
            contract_hash: vec![0xA5; ContractHash::LEN],
            input_schema: Vec::new(),
            output_schema: Vec::new(),
            is_mutable: false,
            min_compatible_version: CatalogVersion::new(1),
            srpl_source: None,
            compiled_ir_handle: None,
        };

        let resolution = CatalogManifestResolution::resolved(
            manifest,
            CatalogVersion::new(2),
            TestResolutionStatus::Resolved,
        );

        assert_eq!(resolution.status, TestResolutionStatus::Resolved);
        assert!(resolution.manifest.is_some());
        assert!(resolution.failure.is_none());
    }
}
