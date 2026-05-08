//! Contract materialization: builder types and hash-binding methods on `ProcedureContract`.

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_types::{ColumnDescriptor, ContractHash, ProcedureId};

use crate::CatalogObjectRef;
use crate::names::QualifiedName;

use super::{
    CompatibilityPolicy, ContractCompatibilityDiagnostic, MultiResultPolicy, PolicyVersion,
    ProcedureContract, ProcedureContractBinding, ProcedureErrorPolicy, ProtocolLayoutRef,
    ResultMetadataPolicy, ResultStreamContract, StatsVersion, TransactionPolicy,
    hash::{
        canonical_policy_version, canonical_procedure_contract_hash,
        canonical_procedure_contract_hash_parts,
    },
    validation::diagnose_procedure_contract_compatibility,
};

impl ProcedureContract {
    pub fn canonical_hash(&self) -> ContractHash {
        canonical_procedure_contract_hash(self)
    }

    /// Compute the policy-only digest for this contract.
    ///
    /// Includes only the policy surface — required permissions, transaction
    /// policy, compatibility policy, result-metadata policy, error policy,
    /// multi-result policy, and stats version — so two contracts with
    /// different shapes but identical policies produce identical
    /// `PolicyVersion` values.  Schema changes (inputs, structured inputs,
    /// result stream columns, protocol layout, name) do **not** influence it.
    pub fn policy_version(&self) -> PolicyVersion {
        canonical_policy_version(
            self.stats_version,
            &self.required_permissions,
            self.transaction_policy,
            self.compatibility_policy,
            self.result_metadata_policy,
            &self.error_policy,
            self.multi_result_policy,
        )
    }

    /// Build the four-identity binding evidence
    /// (`ContractHash` / `CatalogVersion` / `StatsVersion` / `PolicyVersion`)
    /// required by the SRPL specification for every procedure invocation.
    ///
    /// This method projects the fields stored on the contract. Use
    /// [`Self::validated_binding`] before accepting or persisting binding
    /// evidence because that path also proves the stored `ContractHash` still
    /// matches the canonical contract shape.
    pub fn binding(&self) -> ProcedureContractBinding {
        ProcedureContractBinding {
            procedure_id: self.procedure_id,
            catalog_version: self.object.catalog_version,
            contract_hash: self.contract_hash,
            stats_version: self.stats_version,
            policy_version: self.policy_version(),
        }
    }

    pub fn validated_binding(&self) -> AndromedaResult<ProcedureContractBinding> {
        let binding = self.binding();
        self.validate_binding(&binding)?;
        Ok(binding)
    }

    pub fn validate_binding(&self, binding: &ProcedureContractBinding) -> AndromedaResult<()> {
        self.validate_canonical_hash()?;
        binding.validate()?;

        if binding.procedure_id != self.procedure_id {
            return Err(binding_mismatch(
                "procedure binding id must match the procedure contract",
            ));
        }
        if binding.catalog_version != self.object.catalog_version {
            return Err(binding_mismatch(
                "procedure binding catalog version must match the procedure contract",
            ));
        }
        if binding.contract_hash != self.contract_hash {
            return Err(binding_mismatch(
                "procedure binding contract hash must match the procedure contract",
            ));
        }
        if binding.stats_version != self.stats_version {
            return Err(binding_mismatch(
                "procedure binding stats version must match the procedure contract",
            ));
        }
        if binding.policy_version != self.policy_version() {
            return Err(binding_mismatch(
                "procedure binding policy version must match the procedure contract",
            ));
        }

        Ok(())
    }

    pub fn validate_canonical_hash(&self) -> AndromedaResult<()> {
        self.validate()?;
        if self.contract_hash != self.canonical_hash() {
            return Err(andromeda_error::AndromedaError::new(
                andromeda_error::AndromedaErrorKind::Contract,
                "procedure contract hash must match canonical contract shape",
            ));
        }

        Ok(())
    }

    pub fn validated(self) -> AndromedaResult<Self> {
        self.validated_binding()?;
        Ok(self)
    }

    pub fn compatibility_with(
        &self,
        previous: &ProcedureContract,
    ) -> ContractCompatibilityDiagnostic {
        diagnose_procedure_contract_compatibility(previous, self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureContractCandidate {
    pub object: CatalogObjectRef,
    pub procedure_id: ProcedureId,
    pub stats_version: StatsVersion,
    pub protocol_layout: ProtocolLayoutRef,
    pub inputs: Vec<ColumnDescriptor>,
    pub structured_inputs: Vec<QualifiedName>,
    pub result_streams: Vec<ResultStreamContract>,
    pub required_permissions: Vec<String>,
    pub transaction_policy: TransactionPolicy,
    pub compatibility_policy: CompatibilityPolicy,
    pub result_metadata_policy: ResultMetadataPolicy,
    pub error_policy: ProcedureErrorPolicy,
    pub multi_result_policy: MultiResultPolicy,
}

impl ProcedureContractCandidate {
    pub fn materialize(self) -> AndromedaResult<ProcedureContract> {
        let contract_hash = canonical_procedure_contract_hash_parts(
            &self.object.name,
            self.stats_version,
            self.protocol_layout,
            &self.inputs,
            &self.structured_inputs,
            &self.result_streams,
            &self.required_permissions,
            self.transaction_policy,
            self.compatibility_policy,
            self.result_metadata_policy,
            &self.error_policy,
            self.multi_result_policy,
        );
        let contract = ProcedureContract {
            object: self.object,
            procedure_id: self.procedure_id,
            contract_hash,
            stats_version: self.stats_version,
            protocol_layout: self.protocol_layout,
            inputs: self.inputs,
            structured_inputs: self.structured_inputs,
            result_streams: self.result_streams,
            required_permissions: self.required_permissions,
            transaction_policy: self.transaction_policy,
            compatibility_policy: self.compatibility_policy,
            result_metadata_policy: self.result_metadata_policy,
            error_policy: self.error_policy,
            multi_result_policy: self.multi_result_policy,
        };
        contract.validated_binding()?;
        Ok(contract)
    }
}

fn binding_mismatch(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Contract, message)
}
