use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

use crate::{
    ProcedureContract, ProcedureContractBinding, ProtocolLayoutRef, QualifiedName,
    TransactionPolicy,
};

/// Canonical, durable identity + contract metadata for an executable
/// procedure. Holds only contract-derived data; never plan caches, runtime
/// state, or buffered results.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureStoreEntry {
    pub procedure_id: ProcedureId,
    pub name: QualifiedName,
    pub binding: ProcedureContractBinding,
    pub protocol_layout: ProtocolLayoutRef,
    pub transaction_policy: TransactionPolicy,
    pub required_permissions: Vec<String>,
}

impl ProcedureStoreEntry {
    /// Project a validated [`ProcedureContract`] into a store entry.
    pub fn from_contract(contract: &ProcedureContract) -> AndromedaResult<Self> {
        let binding = contract.validated_binding()?;
        Ok(Self {
            procedure_id: contract.procedure_id,
            name: contract.object.name.clone(),
            binding,
            protocol_layout: contract.protocol_layout,
            transaction_policy: contract.transaction_policy,
            required_permissions: contract.required_permissions.clone(),
        })
    }

    /// Validate the entry's internal consistency (delegates to the
    /// underlying contract building blocks).
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.procedure_id.get() == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Catalog,
                "procedure store entry id must not be zero",
            ));
        }
        self.binding.validate()?;
        if self.binding.procedure_id != self.procedure_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store entry id and binding id must agree",
            ));
        }
        self.protocol_layout.validate()?;
        if self.required_permissions.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "procedure store entry must declare required permissions",
            ));
        }
        Ok(())
    }

    pub fn contract_hash(&self) -> ContractHash {
        self.binding.contract_hash
    }

    pub fn catalog_version(&self) -> CatalogVersion {
        self.binding.catalog_version
    }
}
