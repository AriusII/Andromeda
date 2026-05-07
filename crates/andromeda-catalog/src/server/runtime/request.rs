use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, CatalogVersion, ContractHash, ProcedureId,
};

use crate::QualifiedName;

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
