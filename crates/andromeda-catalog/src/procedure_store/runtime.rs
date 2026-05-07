mod counters;
mod identity;
mod record;
mod status;

pub use self::counters::ProcedureRuntimeCounters;
pub use self::identity::{ProcedureRuntimePlanId, ProcedureRuntimeRecordId};
pub use self::record::{InvocationRuntimeRecord, InvocationRuntimeRecordOutcome};
pub use self::status::ProcedureRuntimeStatus;

use andromeda_core::{AndromedaError, AndromedaErrorKind};

fn runtime_contract_error(message: impl Into<String>) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Contract, message)
}
