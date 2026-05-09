use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{
    ProcedureContract, ProcedureContractBinding, ProtocolLayoutRef, QualifiedName, StatsVersion,
    TransactionPolicy,
};
use andromeda_types::{CatalogVersion, ContractHash, ProcedureId};

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

    /// Validate the entry's internal consistency.
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

    pub fn validate_decision_binding(
        &self,
        binding: ProcedureContractBinding,
    ) -> AndromedaResult<()> {
        self.validate_binding_for_context(binding, "decision evidence")
    }

    pub fn validate_runtime_binding(
        &self,
        binding: ProcedureContractBinding,
    ) -> AndromedaResult<()> {
        self.validate_binding_for_context(binding, "runtime evidence")
    }

    pub fn validate_feedback_stats_version(
        &self,
        stats_version: StatsVersion,
    ) -> AndromedaResult<()> {
        if self.binding.stats_version != stats_version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure store feedback stats version does not match registered binding",
            ));
        }
        Ok(())
    }

    fn validate_binding_for_context(
        &self,
        binding: ProcedureContractBinding,
        context: &'static str,
    ) -> AndromedaResult<()> {
        if self.binding.procedure_id != binding.procedure_id {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                format!("procedure store {context} procedure id does not match registered binding"),
            ));
        }
        if self.binding.contract_hash != binding.contract_hash {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                format!(
                    "procedure store {context} contract hash does not match registered binding"
                ),
            ));
        }
        if self.binding.catalog_version != binding.catalog_version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                format!(
                    "procedure store {context} catalog version does not match registered binding"
                ),
            ));
        }
        if self.binding.stats_version != binding.stats_version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                format!(
                    "procedure store {context} stats version does not match registered binding"
                ),
            ));
        }
        if self.binding.policy_version != binding.policy_version {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                format!(
                    "procedure store {context} policy version does not match registered binding"
                ),
            ));
        }
        Ok(())
    }
}
