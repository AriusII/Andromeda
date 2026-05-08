#![forbid(unsafe_code)]
#![doc = r#"
# andromeda-execution

Owner for admitted Procedure execution orchestration adapters.

This crate preserves Procedure contract-first invocation, ResultStream
metadata-before-payload sequencing, no business hardcoding, and no
application-facing SQL.
"#]

mod procedure_adapter;

pub use andromeda_procedure_runtime::{
    PreTransactionDispatchEvidence, ProcedureDispatchRequest, ProcedureDispatchUnavailableReason,
    ProcedureRequestResolver,
};
pub use procedure_adapter::{
    ExecutionProcedureDispatcher, RemoteProcedureDispatcherUnavailable, SrplProcedureRuntimeAdapter,
};
