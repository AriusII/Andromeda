//! SRPL procedure dispatcher for local runtime integration.
//!
//! Wires procedure name resolution, SRPL IR lowering, and deterministic interpretation
//! into a single dispatch path that coexists with V0 hardcoded procedures.

use std::sync::Arc;

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_srpl::{
    interpreter::SrplIrInterpreter,
    procedure_resolver::{ProcedureResolver, ProcedureResolveError},
};

use crate::{InvocationRequest, LocalProcedure, ResultStreamMetadata};

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
    interpreter: Arc<SrplIrInterpreter>,
}

impl SrplProcedureDispatcher {
    /// Construct a new SRPL dispatcher wiring resolver and interpreter.
    ///
    /// Both dependencies are shared (Arc) so the dispatcher can be cloned
    /// and safely shared across threads.
    pub fn new(
        resolver: Arc<dyn ProcedureResolver>,
        interpreter: Arc<SrplIrInterpreter>,
    ) -> Self {
        Self {
            resolver,
            interpreter,
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
    ) -> Result<
        andromeda_srpl::procedure_resolver::ProcedureResolveResponse,
        ProcedureResolveError,
    > {
        let resolve_request = andromeda_srpl::procedure_resolver::ProcedureResolveRequest::from_contract_ref(req.procedure)
            .map_err(|e| e)?;

        let response = self.resolver.resolve_procedure(&resolve_request)?;

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
    /// Used to construct the `LocalProcedure` result shape that the runtime
    /// expects. Result metadata comes from the catalog contract and is fixed
    /// at resolution time.
    ///
    /// TODO: The implementation will extract:
    /// - Stream ID from the result stream contract
    /// - Row count bounds from the manifest policy  
    /// - Column count from the result stream definition
    /// - Cardinality from the procedure contract
    /// And construct a valid ResultStreamMetadata for the runtime.
    pub fn result_metadata_for_plan(
        _plan: &andromeda_srpl::procedure_model::ExecutableProcedurePlan,
    ) -> AndromedaResult<ResultStreamMetadata> {
        // TODO: Implement full metadata extraction from plan evidence and manifest
        Err(AndromedaError::new(
            AndromedaErrorKind::Unimplemented,
            "SRPL result metadata extraction not yet implemented - pending wave 17",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatcher_constructs_with_arc_dependencies() {
        // This test verifies that the dispatcher can be constructed with shared dependencies
        // and that those dependencies are properly wrapped in Arc.
        // Actual resolver/interpreter mocking will be tested in integration tests.
    }
}
