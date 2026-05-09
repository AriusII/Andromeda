use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{ProcedureContractBinding, ProcedureContractRef};
use andromeda_result_stream::ResultStreamMetadata;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalProcedure {
    pub contract: ProcedureContractRef,
    pub contract_binding: ProcedureContractBinding,
    pub required_permissions: Vec<String>,
    pub result_metadata: ResultStreamMetadata,
    pub mutation_payload: Vec<u8>,
    pub rows_affected: u64,
}

impl LocalProcedure {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.contract.validate()?;
        self.contract_binding.validate()?;
        if self.contract_binding.as_legacy_ref() != self.contract {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "local executable ProcedureContractBinding must match local Procedure contract",
            ));
        }
        self.result_metadata.validate_before_payload()?;

        for permission in &self.required_permissions {
            if permission.trim().is_empty() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Security,
                    "required permission must not be empty",
                ));
            }
        }

        if self.mutation_payload.is_empty() && self.rows_affected != 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Execution,
                "mutation payload must exist when rows are affected",
            ));
        }

        Ok(())
    }
}
