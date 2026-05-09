use andromeda_catalog::CatalogSnapshot;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_observe::Permission as AuditPermission;
use andromeda_observe::{
    DecisionTrace, SecurityAuditOutcome, SurfaceScope as AuditSurfaceScope, TraceId,
};

use crate::{
    AuthorizedProcedureDispatch, InvocationContext, InvocationRequest, local::types::LocalProcedure,
};

#[derive(Debug)]
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
    let admission_trace = request
        .validate_admission(trace_id)
        .map_err(|reject| AndromedaError::new(AndromedaErrorKind::Contract, reject.reason))?;
    procedure.validate()?;
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
    let visible_catalog_version = catalog.visible_version();
    let has_durable_publication =
        catalog.is_durably_published() || catalog.visible_publication_receipt().is_some();
    if !has_durable_publication {
        return Err(AndromedaError::new(
            AndromedaErrorKind::Contract,
            "catalog-resolved procedure execution requires a durably published visible catalog snapshot before transaction creation",
        ));
    }

    if request.catalog_version != visible_catalog_version {
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

#[cfg(test)]
mod tests {
    use andromeda_catalog::inventory_reserve_stock_contract;
    use andromeda_core::{
        AndromedaErrorKind, CatalogVersion, ContractHash, InvocationId, ProcedureId,
    };
    use andromeda_observe::TraceId;
    use andromeda_procedure_contract::{
        PolicyVersion, ProcedureContract, ProcedureContractBinding, ProcedureContractRef,
        StatsVersion,
    };
    use andromeda_srpl_ir::Cardinality;

    use crate::local::types::LocalProcedure;
    use crate::{InvocationContext, InvocationRequest, ResultStreamMetadata};

    use super::validate_pre_transaction_admission;

    fn request(contract: &ProcedureContract) -> InvocationRequest {
        InvocationRequest {
            invocation_id: InvocationId::new(900),
            procedure: contract.as_ref(),
            expected_binding: Some(contract.binding()),
            expected_contract_hash: contract.contract_hash,
            catalog_version: contract.object.catalog_version,
            structured_parameters: Vec::new(),
        }
    }

    fn local_procedure(contract: &ProcedureContract) -> LocalProcedure {
        LocalProcedure {
            contract: contract.as_ref(),
            contract_binding: contract.binding(),
            required_permissions: contract.required_permissions.clone(),
            result_metadata: ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: Some(1),
                row_count_max: Some(1),
                column_count: contract.result_streams[0].columns.len() as u32,
                cardinality: Cardinality::One,
            },
            mutation_payload: b"Inventory.ReserveStock".to_vec(),
            rows_affected: 1,
        }
    }

    fn binding_for(procedure: ProcedureContractRef) -> ProcedureContractBinding {
        ProcedureContractBinding {
            procedure_id: procedure.procedure_id,
            catalog_version: procedure.catalog_version,
            contract_hash: procedure.contract_hash,
            stats_version: StatsVersion::new(1),
            policy_version: PolicyVersion::new([7; PolicyVersion::LEN]),
        }
    }

    #[test]
    fn request_admission_precedes_local_executable_validation() {
        let contract = inventory_reserve_stock_contract().unwrap();
        let mut request = request(&contract);
        request.invocation_id = InvocationId::new(0);
        let mut procedure = local_procedure(&contract);
        procedure.required_permissions = vec![String::new()];

        let error = validate_pre_transaction_admission(
            &request,
            &procedure,
            TraceId::new(9100),
            Some(&InvocationContext::new(
                TraceId::new(9100),
                contract.required_permissions.clone(),
            )),
        )
        .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert!(error.message().contains("InvocationId"));
    }

    #[test]
    fn contract_validation_precedes_authorization_in_pre_transaction_admission() {
        let contract = inventory_reserve_stock_contract().unwrap();
        let mut request = request(&contract);
        request.expected_contract_hash = ContractHash::test_vector(0x91);
        let procedure = local_procedure(&contract);

        let error = validate_pre_transaction_admission(
            &request,
            &procedure,
            TraceId::new(9101),
            Some(&InvocationContext::new(TraceId::new(9101), Vec::new())),
        )
        .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert!(error.message().contains("ContractHash"));
    }

    #[test]
    fn full_binding_is_required_before_pre_transaction_dispatch() {
        let contract = inventory_reserve_stock_contract().unwrap();
        let mut request = request(&contract);
        request.procedure = ProcedureContractRef {
            procedure_id: ProcedureId::new(0x9102),
            contract_hash: ContractHash::test_vector(0x92),
            catalog_version: CatalogVersion::new(9),
        };
        request.expected_binding = Some(binding_for(request.procedure));
        request.expected_contract_hash = request.procedure.contract_hash;
        request.catalog_version = request.procedure.catalog_version;
        let procedure = LocalProcedure {
            contract: request.procedure,
            contract_binding: binding_for(request.procedure),
            required_permissions: Vec::new(),
            result_metadata: ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: Some(1),
                row_count_max: Some(1),
                column_count: 1,
                cardinality: Cardinality::One,
            },
            mutation_payload: b"Inventory.ReserveStock".to_vec(),
            rows_affected: 1,
        };

        request.expected_binding = None;

        let error =
            validate_pre_transaction_admission(&request, &procedure, TraceId::new(9102), None)
                .unwrap_err();

        assert_eq!(error.kind(), AndromedaErrorKind::Contract);
        assert!(error.message().contains("ProcedureContractBinding"));
    }
}
