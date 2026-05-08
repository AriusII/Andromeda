use andromeda_contract::{ProcedureContractRef, QualifiedName};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use super::{error::ProcedureResolveError, response::ProcedureResolveResponse};

/// Address of a procedure to resolve before transaction creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcedureResolveTarget {
    ProcedureId(ProcedureId),
    QualifiedName(QualifiedName),
}

impl ProcedureResolveTarget {
    pub(super) fn validate(&self) -> Result<(), ProcedureResolveError> {
        match self {
            Self::ProcedureId(procedure_id) if procedure_id.get() == 0 => {
                Err(ProcedureResolveError::InvalidRequest {
                    message: "procedure resolver target id must not be zero".to_string(),
                })
            }
            Self::ProcedureId(_) | Self::QualifiedName(_) => Ok(()),
        }
    }

    pub(super) fn matches_response(&self, response: &ProcedureResolveResponse) -> bool {
        match self {
            Self::ProcedureId(procedure_id) => *procedure_id == response.procedure_id,
            Self::QualifiedName(name) => name == &response.name,
        }
    }
}

/// Pre-transaction procedure resolution request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureResolveRequest {
    pub target: ProcedureResolveTarget,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
}

impl ProcedureResolveRequest {
    pub fn by_procedure_id(
        procedure_id: ProcedureId,
        contract_hash: ContractHash,
        catalog_version: CatalogVersion,
    ) -> Result<Self, ProcedureResolveError> {
        let request = Self {
            target: ProcedureResolveTarget::ProcedureId(procedure_id),
            contract_hash,
            catalog_version,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn by_qualified_name(
        name: QualifiedName,
        contract_hash: ContractHash,
        catalog_version: CatalogVersion,
    ) -> Result<Self, ProcedureResolveError> {
        let request = Self {
            target: ProcedureResolveTarget::QualifiedName(name),
            contract_hash,
            catalog_version,
        };
        request.validate()?;
        Ok(request)
    }

    pub fn from_contract_ref(
        contract_ref: ProcedureContractRef,
    ) -> Result<Self, ProcedureResolveError> {
        contract_ref
            .validate()
            .map_err(ProcedureResolveError::invalid_request)?;
        Self::by_procedure_id(
            contract_ref.procedure_id,
            contract_ref.contract_hash,
            contract_ref.catalog_version,
        )
    }

    pub fn validate(&self) -> Result<(), ProcedureResolveError> {
        self.target.validate()?;
        if self.contract_hash.is_zero() {
            return Err(ProcedureResolveError::InvalidRequest {
                message: "procedure resolver request contract hash must not be zero".to_string(),
            });
        }
        if self.catalog_version.get() == 0 {
            return Err(ProcedureResolveError::InvalidRequest {
                message: "procedure resolver request catalog version must not be zero".to_string(),
            });
        }
        Ok(())
    }

    pub fn expected_contract_ref(&self) -> Option<ProcedureContractRef> {
        match &self.target {
            ProcedureResolveTarget::ProcedureId(procedure_id) => Some(ProcedureContractRef {
                procedure_id: *procedure_id,
                contract_hash: self.contract_hash,
                catalog_version: self.catalog_version,
            }),
            ProcedureResolveTarget::QualifiedName(_) => None,
        }
    }

    /// Validate that a resolver response exactly satisfies this request.
    pub fn validate_response(
        &self,
        response: &ProcedureResolveResponse,
    ) -> Result<(), ProcedureResolveError> {
        self.validate()?;
        response.validate()?;

        if !self.target.matches_response(response) {
            return Err(ProcedureResolveError::UnknownProcedure {
                target: self.target.clone(),
            });
        }
        if response.catalog_version != self.catalog_version {
            return Err(ProcedureResolveError::VersionMismatch {
                requested: self.catalog_version,
                actual: response.catalog_version,
            });
        }
        if response.contract_hash != self.contract_hash {
            return Err(ProcedureResolveError::ContractMismatch {
                procedure_id: response.procedure_id,
                expected: self.contract_hash,
                actual: response.contract_hash,
            });
        }

        Ok(())
    }
}
