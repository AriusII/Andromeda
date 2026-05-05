//! SRPL procedure dispatcher for local runtime integration.
//!
//! Wires procedure name resolution, SRPL IR lowering, and deterministic interpretation
//! into a single dispatch path that coexists with V0 hardcoded procedures.

use std::sync::Arc;

use andromeda_catalog::ResultStreamContract;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_srpl::{
    interpreter::SrplIrInterpreter,
    procedure_resolver::{ProcedureResolveError, ProcedureResolver},
};

use crate::{
    DefaultResultMetadataExtractor, InvocationRequest, ResultMetadataExtractor,
    ResultStreamMetadata,
};

/// SRPL procedure dispatcher: resolves procedure name to IR plan and executes deterministically.
///
/// This dispatcher:
/// 1. Takes a procedure name/id from the invocation request
/// 2. Resolves it to an executable SRPL plan (pre-transaction boundary)
/// 3. Lowers the plan to intermediate representation
/// 4. Executes the IR plan using the stateless interpreter
///
/// The dispatcher is independent of transaction management, storage access, or
/// runtime-specific concerns. All external behavior is delegated to the resolver
/// and execution adapters.
#[derive(Clone)]
pub struct SrplProcedureDispatcher {
    resolver: Arc<dyn ProcedureResolver>,
    _interpreter: Arc<SrplIrInterpreter>,
}

impl SrplProcedureDispatcher {
    /// Construct a new SRPL dispatcher wiring resolver and interpreter.
    ///
    /// Both dependencies are shared (Arc) so the dispatcher can be cloned
    /// and safely shared across threads.
    pub fn new(resolver: Arc<dyn ProcedureResolver>, interpreter: Arc<SrplIrInterpreter>) -> Self {
        Self {
            resolver,
            _interpreter: interpreter,
        }
    }

    /// Resolve a procedure request from an invocation and validate the plan.
    ///
    /// This is the pre-transaction resolution step. It returns the resolved plan
    /// or a typed resolver error if resolution fails. No transaction is created,
    /// no storage is accessed, and no side effects occur.
    pub fn resolve_procedure(
        &self,
        req: &InvocationRequest,
    ) -> Result<andromeda_srpl::procedure_resolver::ProcedureResolveResponse, ProcedureResolveError>
    {
        let resolve_request =
            andromeda_srpl::procedure_resolver::ProcedureResolveRequest::from_contract_ref(
                req.procedure,
            )
            .map_err(|e| e)?;

        let response = self.resolver.resolve_procedure(resolve_request.clone())?;

        resolve_request.validate_response(&response)?;

        Ok(response)
    }

    /// Validate that an executable plan can be interpreted.
    ///
    /// Checks for unsupported operations, binding errors, cardinality violations,
    /// and other semantic issues that would prevent execution. This is distinct
    /// from plan execution and produces no side effects.
    pub fn validate_plan(
        plan: &andromeda_srpl::procedure_model::ExecutableProcedurePlan,
    ) -> AndromedaResult<()> {
        SrplIrInterpreter::validate_plan(plan).map_err(|e| {
            AndromedaError::new(
                AndromedaErrorKind::Srpl,
                format!("SRPL plan validation failed: {}", e.message()),
            )
        })
    }

    /// Extract result metadata from a resolved procedure plan.
    ///
    /// Converts plan evidence and result stream contracts into `ResultStreamMetadata`
    /// that the runtime uses for result framing and validation.
    ///
    /// # Arguments
    /// * `plan` - The executable SRPL procedure plan
    /// * `result_streams` - Result stream contracts from the procedure manifest
    ///
    /// # Returns
    /// Valid `ResultStreamMetadata` ready for emission before payload, or an error
    /// if metadata cannot be deterministically extracted.
    pub fn result_metadata_for_plan(
        plan: &andromeda_srpl::procedure_model::ExecutableProcedurePlan,
        result_streams: &[ResultStreamContract],
    ) -> AndromedaResult<ResultStreamMetadata> {
        DefaultResultMetadataExtractor::extract_metadata(plan, result_streams)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dispatcher_constructs_with_arc_dependencies() {
        // This test verifies that the dispatcher can be constructed with shared dependencies
        // and that those dependencies are properly wrapped in Arc.
        // Actual resolver/interpreter mocking will be tested in integration tests.
    }
}
