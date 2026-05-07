use andromeda_catalog::{ProcedureContractRef, QualifiedName};
use andromeda_core::{CatalogVersion, ContractHash, ProcedureId};

use crate::procedure_model::ExecutableProcedurePlan;

use super::{error::ProcedureResolveError, manifest::SrplProcedureManifest};

/// Successful pre-transaction resolution outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureResolveResponse {
    pub procedure_id: ProcedureId,
    pub name: QualifiedName,
    pub contract_hash: ContractHash,
    pub catalog_version: CatalogVersion,
    pub contract_ref: ProcedureContractRef,
    pub manifest: SrplProcedureManifest,
    pub plan: ExecutableProcedurePlan,
}

impl ProcedureResolveResponse {
    pub fn validate(&self) -> Result<(), ProcedureResolveError> {
        self.contract_ref
            .validate()
            .map_err(ProcedureResolveError::invalid_response)?;
        self.manifest
            .validate()
            .map_err(ProcedureResolveError::invalid_response)?;
        self.plan
            .validate()
            .map_err(ProcedureResolveError::invalid_response)?;

        if self.procedure_id.get() == 0 {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver response id must not be zero".to_string(),
            });
        }
        if self.contract_hash.is_zero() {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver response contract hash must not be zero".to_string(),
            });
        }
        if self.catalog_version.get() == 0 {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver response catalog version must not be zero".to_string(),
            });
        }
        if self.contract_ref.procedure_id != self.procedure_id
            || self.contract_ref.contract_hash != self.contract_hash
            || self.contract_ref.catalog_version != self.catalog_version
        {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver response identities must match ProcedureContractRef"
                    .to_string(),
            });
        }
        if self.manifest.contract_ref != self.contract_ref {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver manifest must carry the resolved contract ref"
                    .to_string(),
            });
        }
        if self.plan.procedure_name != self.name {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver plan name must match the resolved procedure"
                    .to_string(),
            });
        }
        if self.plan.evidence.procedure_contract != self.contract_ref {
            return Err(ProcedureResolveError::InvalidResponse {
                message: "procedure resolver plan evidence must match the resolved contract ref"
                    .to_string(),
            });
        }

        Ok(())
    }
}
