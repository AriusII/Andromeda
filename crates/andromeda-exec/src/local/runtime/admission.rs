use andromeda_catalog::CatalogSnapshot;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::Permission as AuditPermission;
use andromeda_observe::{
    DecisionTrace, SecurityAuditOutcome, SurfaceScope as AuditSurfaceScope, TraceId,
};

use crate::{
    AuthorizedProcedureDispatch, InvocationContext, InvocationRequest, local::types::LocalProcedure,
};

pub(super) struct PreTransactionAdmission {
    pub(super) admission_trace: DecisionTrace,
    pub(super) contract_trace: DecisionTrace,
    pub(super) authorization_trace: Option<DecisionTrace>,
}

pub(super) fn validate_pre_transaction_admission(
    request: &InvocationRequest,
    procedure: &LocalProcedure,
    trace_id: TraceId,
    context: Option<&InvocationContext>,
) -> AndromedaResult<PreTransactionAdmission> {
    procedure.validate()?;
    let admission_trace = request
        .validate_admission(trace_id)
        .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;
    let contract_trace = request
        .validate_before_transaction(procedure.contract_binding, trace_id)
        .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;
    require_authorization_context_for_permissioned_procedure(procedure, context)?;
    let authorization_trace = context
        .map(|context| {
            context
                .authorize(&procedure.required_permissions)
                .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Security, reject.reason))
        })
        .transpose()?;

    Ok(PreTransactionAdmission {
        admission_trace,
        contract_trace,
        authorization_trace,
    })
}

pub(super) fn validate_catalog_resolved_procedure(
    request: &InvocationRequest,
    procedure: &LocalProcedure,
    catalog: &CatalogSnapshot,
    trace_id: TraceId,
) -> AndromedaResult<()> {
    if request.catalog_version != catalog.version {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "CatalogVersion mismatch against visible catalog snapshot before transaction creation",
        ));
    }

    let published_contract = catalog
        .get_procedure_by_id(request.procedure.procedure_id)
        .ok_or_else(|| {
            AndromedaError::new(
                AndromedaErrorKind::Contract,
                "procedure is absent from visible catalog snapshot before transaction creation",
            )
        })?;

    if !catalog.is_active_object(published_contract.object.object_id) {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "procedure is not active in visible catalog snapshot before transaction creation",
        ));
    }

    request
        .validate_before_transaction(published_contract.binding(), trace_id)
        .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;

    if procedure.contract != published_contract.as_ref() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "local executable procedure does not match visible catalog contract before transaction creation",
        ));
    }

    if procedure.contract_binding != published_contract.binding() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "local executable ProcedureContractBinding does not match visible catalog binding before transaction creation",
        ));
    }

    Ok(())
}

pub(super) fn validate_surface_dispatch_token(
    context: &InvocationContext,
    dispatch: &AuthorizedProcedureDispatch,
) -> AndromedaResult<()> {
    let audit = dispatch.audit();
    if audit.trace_id != context.trace_id {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Security,
            "procedure dispatch authorization trace id must match invocation context before transaction creation",
        ));
    }
    if audit.outcome != SecurityAuditOutcome::Allowed {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Security,
            "procedure dispatch authorization token must carry an allowed audit decision",
        ));
    }
    if audit.surface != AuditSurfaceScope::Application {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Security,
            "procedure dispatch authorization token must be scoped to the Application surface",
        ));
    }
    if audit.permission != AuditPermission::ExecuteProcedure {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Security,
            "procedure dispatch authorization token must prove ExecuteProcedure permission",
        ));
    }

    Ok(())
}

fn require_authorization_context_for_permissioned_procedure(
    procedure: &LocalProcedure,
    context: Option<&InvocationContext>,
) -> AndromedaResult<()> {
    if context.is_none() && !procedure.required_permissions.is_empty() {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Security,
            "permissioned Procedure execution requires IAM authorization context before transaction creation",
        ));
    }

    Ok(())
}
