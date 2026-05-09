#![forbid(unsafe_code)]
#![doc = r#"
# andromeda-procedure-runtime

Owner for generic Procedure runtime dispatch after admission.

This crate preserves Procedure contract-first invocation, generic handler
dispatch, no business hardcoding, no application-facing SQL, and durable
terminal evidence handoff.
"#]

mod dispatch;
pub mod procedure_resolver;
mod result_metadata_extractor;

pub use dispatch::{
    PreTransactionDispatchEvidence, ProcedureDispatchRequest, ProcedureDispatchUnavailableReason,
    ProcedureDispatcher, ProcedureRequestResolver, RemoteProcedureDispatcherUnavailable,
    SrplDispatcherAdapter,
};
pub use procedure_resolver::{
    ProcedureResolveError, ProcedureResolveRequest, ProcedureResolveResponse,
    ProcedureResolveTarget, ProcedureResolver, SrplProcedureManifest,
};
pub use result_metadata_extractor::{DefaultResultMetadataExtractor, ResultMetadataExtractor};
