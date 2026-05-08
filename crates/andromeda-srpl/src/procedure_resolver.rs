//! Compatibility reexport for SRPL procedure resolver contracts.
//!
//! `andromeda-procedure-runtime` owns the pre-dispatch resolver contract. This
//! module preserves the historical `andromeda_srpl::procedure_resolver` import
//! path without wrapping or duplicating the runtime-owned types.

pub use andromeda_procedure_runtime::procedure_resolver::{
    ProcedureResolveError, ProcedureResolveRequest, ProcedureResolveResponse,
    ProcedureResolveTarget, ProcedureResolver, SrplProcedureManifest,
};
