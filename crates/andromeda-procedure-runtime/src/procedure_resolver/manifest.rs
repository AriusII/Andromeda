use std::collections::BTreeSet;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{
    MultiResultPolicy, ProcedureContract, ProcedureContractRef, ProcedureErrorPolicy,
    ProtocolLayoutRef, QualifiedName, ResultMetadataPolicy, ResultStreamContract,
    TransactionPolicy,
};
use andromeda_types::ColumnDescriptor;

use super::validation::validate_dense_columns_allow_empty;

/// Resolver-visible manifest for a resolved SRPL procedure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SrplProcedureManifest {
    pub contract_ref: ProcedureContractRef,
    pub inputs: Vec<ColumnDescriptor>,
    pub structured_inputs: Vec<QualifiedName>,
    pub result_streams: Vec<ResultStreamContract>,
    pub transaction_policy: TransactionPolicy,
    pub required_permissions: Vec<String>,
    pub protocol_layout: ProtocolLayoutRef,
    pub result_metadata_policy: ResultMetadataPolicy,
    pub error_policy: ProcedureErrorPolicy,
    pub multi_result_policy: MultiResultPolicy,
}

impl SrplProcedureManifest {
    pub fn from_contract(contract: &ProcedureContract) -> AndromedaResult<Self> {
        contract.validate()?;
        Ok(Self {
            contract_ref: contract.as_ref(),
            inputs: contract.inputs.clone(),
            structured_inputs: contract.structured_inputs.clone(),
            result_streams: contract.result_streams.clone(),
            transaction_policy: contract.transaction_policy,
            required_permissions: contract.required_permissions.clone(),
            protocol_layout: contract.protocol_layout,
            result_metadata_policy: contract.result_metadata_policy,
            error_policy: contract.error_policy.clone(),
            multi_result_policy: contract.multi_result_policy,
        })
    }

    pub fn validate(&self) -> AndromedaResult<()> {
        self.contract_ref.validate()?;
        validate_dense_columns_allow_empty(&self.inputs, "procedure resolver input columns")?;
        self.protocol_layout.validate()?;
        self.error_policy.validate()?;
        self.validate_permissions()?;
        self.validate_structured_inputs()?;
        self.validate_result_streams()?;

        Ok(())
    }

    fn validate_permissions(&self) -> AndromedaResult<()> {
        if self.required_permissions.is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Security,
                "procedure resolver manifest must declare required permissions",
            ));
        }

        let mut permissions = BTreeSet::new();
        for permission in &self.required_permissions {
            if permission.trim().is_empty() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "procedure resolver manifest permissions must not be empty",
                ));
            }
            if !permissions.insert(permission.as_str()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "procedure resolver manifest permissions must be unique",
                ));
            }
        }

        Ok(())
    }

    fn validate_structured_inputs(&self) -> AndromedaResult<()> {
        let mut structured_inputs = BTreeSet::new();
        for structured_input in &self.structured_inputs {
            if !structured_inputs.insert(structured_input) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure resolver manifest structured inputs must be unique",
                ));
            }
        }

        Ok(())
    }

    fn validate_result_streams(&self) -> AndromedaResult<()> {
        let mut stream_ids = BTreeSet::new();
        let mut stream_names = BTreeSet::new();
        for stream in &self.result_streams {
            stream.validate()?;
            if !stream_ids.insert(stream.stream_id) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure resolver manifest result stream ids must be unique",
                ));
            }
            if !stream_names.insert(stream.name.as_str()) {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Contract,
                    "procedure resolver manifest result stream names must be unique",
                ));
            }
        }
        if self.multi_result_policy == MultiResultPolicy::SingleResultOnly
            && self.result_streams.len() > 1
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure resolver manifest multi-result policy allows only one stream",
            ));
        }

        Ok(())
    }
}
