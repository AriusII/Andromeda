#![forbid(unsafe_code)]
#![doc = r#"
# andromeda-execution

Owner for admitted Procedure execution orchestration adapters.

This crate preserves Procedure contract-first invocation, ResultStream
metadata-before-payload sequencing, no business hardcoding, and no
application-facing SQL.
"#]

mod catalog_resolver;
mod local_procedure;
mod procedure_adapter;
mod procedure_registry;
mod srpl_adapters;

pub use andromeda_admission::InvocationContext;
pub use andromeda_procedure_runtime::{
    PreTransactionDispatchEvidence, ProcedureDispatchRequest, ProcedureDispatchUnavailableReason,
    ProcedureRequestResolver,
};
pub use andromeda_result_stream::ResultStreamMetadata;
pub use catalog_resolver::{
    CatalogBackedProcedureResolver, CatalogManifestStore, CatalogResolverError,
};
pub use local_procedure::LocalProcedure;
pub use procedure_adapter::{
    ExecutionProcedureDispatcher, RemoteProcedureDispatcherUnavailable, SrplProcedureRuntimeAdapter,
};
pub use procedure_registry::{ProcedureHandler, ProcedureRegistry};
pub use srpl_adapters::{
    FieldValue, SrplExecutionAdapter, SrplStreamBackpressure, SrplTransactionContext,
    SrplTypedEnvironment, StructuredObject,
};
