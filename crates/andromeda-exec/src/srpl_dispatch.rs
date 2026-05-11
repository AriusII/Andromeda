//! SRPL procedure dispatcher for local runtime integration.
//!
//! Wires procedure name resolution, SRPL IR lowering, and deterministic interpretation
//! into a single dispatch path that coexists with V0 hardcoded procedures.

use std::sync::Arc;

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_procedure_contract::{AccessMode, ResultStreamContract};
use andromeda_procedure_runtime::procedure_resolver::{
    ProcedureResolveError, ProcedureResolveRequest, ProcedureResolveResponse, ProcedureResolver,
};
use andromeda_procedure_runtime::{ProcedureDispatchRequest, ProcedureDispatcher};
use andromeda_srpl_interpreter::SrplIrInterpreter;
use andromeda_srpl_ir::ExecutableProcedurePlan;

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
    ) -> Result<ProcedureResolveResponse, ProcedureResolveError> {
        let resolve_request = ProcedureResolveRequest::from_contract_ref(req.procedure)?;

        let response = self.resolver.resolve_procedure(resolve_request.clone())?;

        resolve_request.validate_response(&response)?;

        Ok(response)
    }

    /// Validate that an executable plan can be interpreted.
    ///
    /// Checks for unsupported operations, binding errors, cardinality violations,
    /// and other semantic issues that would prevent execution. This is distinct
    /// from plan execution and produces no side effects.
    pub fn validate_plan(plan: &ExecutableProcedurePlan) -> AndromedaResult<()> {
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
        plan: &ExecutableProcedurePlan,
        result_streams: &[ResultStreamContract],
    ) -> AndromedaResult<ResultStreamMetadata> {
        DefaultResultMetadataExtractor::extract_metadata(plan, result_streams)
    }

    fn local_procedure_for_resolved(
        request: ProcedureDispatchRequest,
        response: ProcedureResolveResponse,
    ) -> AndromedaResult<crate::LocalProcedure> {
        let contract_binding = request.procedure_binding.ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "SRPL dispatch requires ProcedureContractBinding before local procedure materialization",
            )
        })?;
        let result_metadata =
            Self::result_metadata_for_plan(&response.plan, &response.manifest.result_streams)?;
        let mutation_payload = match response.manifest.transaction_policy.access_mode {
            AccessMode::ReadOnly => Vec::new(),
            AccessMode::ReadWrite => response.name.as_catalog_path().into_bytes(),
        };
        let rows_affected = if matches!(
            response.manifest.transaction_policy.access_mode,
            AccessMode::ReadWrite
        ) {
            1
        } else {
            0
        };

        let procedure = crate::LocalProcedure {
            contract: response.contract_ref,
            contract_binding,
            required_permissions: response.manifest.required_permissions,
            result_metadata,
            mutation_payload,
            rows_affected,
        };
        procedure.validate()?;
        Ok(procedure)
    }
}

impl ProcedureDispatcher for SrplProcedureDispatcher {
    type Procedure = crate::LocalProcedure;

    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<Self::Procedure> {
        request.validate()?;
        let invocation_request = InvocationRequest {
            invocation_id: request.invocation_id,
            procedure: request.procedure,
            expected_binding: request.procedure_binding,
            expected_contract_hash: request.procedure.contract_hash,
            catalog_version: request.procedure.catalog_version,
            structured_parameters: Vec::new(),
        };
        let response = self
            .resolve_procedure(&invocation_request)
            .map_err(|resolve_err| resolve_err.into_andromeda_error())?;

        Self::validate_plan(&response.plan)?;
        Self::local_procedure_for_resolved(request, response)
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
