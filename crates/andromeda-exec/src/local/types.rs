use andromeda_catalog::ProcedureContractRef;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, TransactionId};
use andromeda_observe::DecisionTrace;

use crate::{InvocationCompletion, ResultStreamMetadata};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalProcedure {
    pub contract: ProcedureContractRef,
    pub required_permissions: Vec<String>,
    pub result_metadata: ResultStreamMetadata,
    pub mutation_payload: Vec<u8>,
    pub rows_affected: u64,
}

impl LocalProcedure {
    pub fn validate(&self) -> AndromedaResult<()> {
        self.contract.validate()?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerticalInvocationOutcome {
    pub completion: InvocationCompletion,
    /// Recovery-safe transaction id allocated by the runtime's
    /// `TransactionManager` for this invocation. This is the authoritative
    /// id stamped on every WAL record and result-stream frame; downstream
    /// emitters must read it from here rather than re-deriving it from
    /// [`InvocationCompletion::invocation_id`].
    pub transaction_id: TransactionId,
    pub admission_trace: DecisionTrace,
    pub contract_trace: DecisionTrace,
    pub authorization_trace: Option<DecisionTrace>,
    pub result_metadata: ResultStreamMetadata,
}
