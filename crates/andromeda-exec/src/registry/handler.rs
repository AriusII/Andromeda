use andromeda_catalog::ProcedureContractRef;
use andromeda_core::{AndromedaResult, ProcedureId};

use crate::{InvocationContext, LocalProcedure, ResultStreamMetadata};

/// Executable Procedure adapter used by future registry-backed local dispatch.
///
/// Callers must invoke a handler only after admission, surface authorization,
/// permission checks, and catalog contract validation have accepted the
/// invocation. The handler does not own transaction allocation, WAL dispatch,
/// remote dispatch, or terminal completion mapping.
pub trait ProcedureHandler {
    /// Stable Procedure identifier used for request matching and registry keys.
    fn procedure_id(&self) -> ProcedureId;

    /// Contract metadata bound to the executable Procedure implementation.
    fn contract(&self) -> ProcedureContractRef;

    /// Result stream metadata that must be validated before payload emission.
    fn result_metadata(&self) -> ResultStreamMetadata;

    /// Execute an already-admitted invocation into the existing local runtime
    /// result shape.
    fn execute(&self, context: InvocationContext) -> AndromedaResult<LocalProcedure>;
}
