use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, ProcedureId};

use crate::{InvocationContext, LocalProcedure, dispatch::validate_dispatch_permissions_or_error};

use super::handler::ProcedureHandler;

pub(super) fn validate_handler_registration(
    handler: &(dyn ProcedureHandler + Send + Sync),
) -> AndromedaResult<()> {
    let procedure_id = handler.procedure_id();

    if procedure_id.get() == 0 {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "registered Procedure id must not be zero",
        ));
    }

    let contract = handler.contract();
    contract.validate()?;

    if contract.procedure_id != procedure_id {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "registered Procedure handler id must match its contract Procedure id",
        ));
    }

    handler.result_metadata().validate_before_payload()?;
    Ok(())
}

pub(super) fn validate_dispatch_result(
    procedure_id: ProcedureId,
    handler: &(dyn ProcedureHandler + Send + Sync),
    procedure: &LocalProcedure,
    context: &InvocationContext,
) -> AndromedaResult<()> {
    procedure.validate()?;

    if procedure.contract != handler.contract() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "dispatched Procedure contract must match registered handler contract",
        ));
    }

    if procedure.contract.procedure_id != procedure_id {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "dispatched Procedure id must match requested Procedure id",
        ));
    }

    // Validate that invocation permissions do not exceed handler contract permissions
    validate_dispatch_permissions_or_error(
        &context.granted_permissions,
        &procedure.required_permissions,
    )?;

    Ok(())
}

pub(super) fn duplicate_procedure_error(procedure_id: ProcedureId) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Contract,
        format!(
            "ProcedureRegistry already contains ProcedureId {}",
            procedure_id.get()
        ),
    )
}

pub(super) fn unknown_procedure_error(procedure_id: ProcedureId) -> AndromedaError {
    AndromedaError::new(
        AndromedaErrorKind::Execution,
        format!(
            "ProcedureRegistry does not contain ProcedureId {}",
            procedure_id.get()
        ),
    )
}
