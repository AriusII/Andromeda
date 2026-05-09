mod identity;
mod record;

pub use self::identity::{ProcedureRuntimePlanId, ProcedureRuntimeRecordId};
pub use self::record::{InvocationRuntimeRecord, InvocationRuntimeRecordOutcome};

use andromeda_error::{AndromedaError, AndromedaErrorKind};

fn runtime_contract_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Contract, message)
}
