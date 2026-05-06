use std::{collections::HashMap, sync::Arc};

use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ProcedureId};

use crate::{InvocationContext, LocalProcedure, ProcedureDispatchRequest, ProcedureDispatcher};

use super::{
    handler::ProcedureHandler,
    validation::{
        duplicate_procedure_error, unknown_procedure_error, validate_dispatch_result,
        validate_handler_registration,
    },
};

/// Registry for local executable Procedure handlers.
///
/// The registry is keyed by [`ProcedureId`], not by raw names, SQL text, remote
/// addresses, or transport method names. Callers must reach this boundary only
/// after admission, surface authorization, permission checks, and catalog
/// contract validation have accepted the invocation.
#[derive(Clone, Default)]
pub struct ProcedureRegistry {
    handlers: HashMap<ProcedureId, Arc<dyn ProcedureHandler + Send + Sync>>,
}

impl ProcedureRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    pub fn register<H>(&mut self, handler: H) -> AndromedaResult<()>
    where
        H: ProcedureHandler + Send + Sync + 'static,
    {
        self.register_arc(Arc::new(handler))
    }

    pub fn register_arc(
        &mut self,
        handler: Arc<dyn ProcedureHandler + Send + Sync>,
    ) -> AndromedaResult<()> {
        let procedure_id = handler.procedure_id();

        if self.handlers.contains_key(&procedure_id) {
            return Err(duplicate_procedure_error(procedure_id));
        }

        validate_handler_registration(handler.as_ref())?;
        self.handlers.insert(procedure_id, handler);
        Ok(())
    }

    pub fn contains(&self, procedure_id: ProcedureId) -> bool {
        self.handlers.contains_key(&procedure_id)
    }

    pub fn lookup(
        &self,
        procedure_id: ProcedureId,
    ) -> Option<Arc<dyn ProcedureHandler + Send + Sync>> {
        self.handlers.get(&procedure_id).cloned()
    }

    /// Dispatch an already-admitted local Procedure invocation.
    ///
    /// This method accepts only a canonical [`ProcedureId`]. It does not perform
    /// remote dispatch, transaction allocation, WAL emission, or terminal
    /// completion mapping.
    ///
    /// **Security:** Validates that invocation permissions do not exceed handler contract
    /// permissions before execution, preventing privilege escalation.
    pub fn dispatch(
        &self,
        procedure_id: ProcedureId,
        context: InvocationContext,
    ) -> AndromedaResult<LocalProcedure> {
        let handler = self
            .lookup(procedure_id)
            .ok_or_else(|| unknown_procedure_error(procedure_id))?;
        let procedure = handler.execute(context.clone())?;
        validate_dispatch_result(procedure_id, handler.as_ref(), &procedure, &context)?;
        Ok(procedure)
    }
}

impl ProcedureDispatcher for ProcedureRegistry {
    fn dispatch_procedure(
        &self,
        request: ProcedureDispatchRequest,
    ) -> AndromedaResult<LocalProcedure> {
        request.validate()?;

        let procedure_id = request.procedure.procedure_id;
        let handler = self
            .lookup(procedure_id)
            .ok_or_else(|| unknown_procedure_error(procedure_id))?;

        if handler.contract() != request.procedure {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "dispatch request contract must match registered handler contract before execution",
            ));
        }

        let procedure = handler.execute(request.context.clone())?;
        validate_dispatch_result(procedure_id, handler.as_ref(), &procedure, &request.context)?;
        Ok(procedure)
    }
}
