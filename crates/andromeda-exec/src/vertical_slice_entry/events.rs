use andromeda_catalog::ProcedureContract;
use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId};
use andromeda_observe::{
    CompletionEmittedTrace, EventCorrelation, EventEmitter, EventSink, ProtocolCorrelation,
    TraceEvent,
};
use andromeda_quic::{FrameType, StreamRole};

use crate::{CompletionStatus, InvocationContext, InvocationReject, InvocationRequest};

use super::V0InventoryRecoverableOutcome;

const V0_INVENTORY_RESERVE_STOCK_CONTRACT_KIND: u16 = 1;

pub(super) fn emit_v0_outcome_events<S: EventSink>(
    outcome: &V0InventoryRecoverableOutcome,
    emitter: &mut EventEmitter<S>,
) -> AndromedaResult<()> {
    let command_protocol = v0_execute_request_protocol();
    let completion_protocol = ProtocolCorrelation {
        protocol_version: Some(1),
        stream_id: Some(outcome.vertical.result_metadata.stream_id),
        stream_role: Some(StreamRole::ResultUnidirectional as u16),
        frame_type: Some(FrameType::RpcCompletion.wire_code() as u16),
        payload_kind: Some(FrameType::RpcCompletion.wire_code() as u16),
        sequence: Some(3),
    };
    let pre_transaction_correlation = EventCorrelation {
        request_id: Some(outcome.command_frame.header.request_id),
        session_id: Some(outcome.command_frame.header.session_id),
        contract_hash: Some(outcome.srpl_plan.evidence.procedure_contract.contract_hash),
        catalog_version: Some(outcome.srpl_plan.evidence.catalog_version),
        catalog_object_id: Some(outcome.srpl_plan.evidence.procedure_object.object_id),
        transaction_id: None,
        durable_lsn: None,
        protocol: Some(command_protocol),
    };
    let transaction_id = outcome.vertical.transaction_id;
    let durable_lsn = outcome.vertical.completion.durable_lsn.ok_or_else(|| {
        AndromedaError::new(
            AndromedaErrorKind::Transaction,
            "V0 completion event requires durable LSN evidence",
        )
    })?;
    let completion_correlation = EventCorrelation {
        transaction_id: Some(transaction_id),
        durable_lsn: Some(durable_lsn.get()),
        protocol: Some(completion_protocol),
        ..pre_transaction_correlation
    };
    emitter.emit(
        pre_transaction_correlation,
        TraceEvent::Decision(outcome.vertical.admission_trace.clone()),
    )?;
    emitter.emit(
        pre_transaction_correlation,
        TraceEvent::Decision(outcome.vertical.contract_trace.clone()),
    )?;
    if let Some(authorization_trace) = &outcome.vertical.authorization_trace {
        emitter.emit(
            pre_transaction_correlation,
            TraceEvent::Decision(authorization_trace.clone()),
        )?;
    }
    emitter.emit(
        completion_correlation,
        TraceEvent::CompletionEmitted(CompletionEmittedTrace {
            trace_id: outcome.vertical.completion.trace_id,
            protocol: completion_protocol,
            completion_code: Some(1),
            committed: true,
            durable_lsn: Some(durable_lsn.get()),
            reason: "V0 Inventory.ReserveStock emitted committed completion after durable WAL"
                .to_string(),
        }),
    )?;
    Ok(())
}

/// Emit typed evidence for V0 invocation refusals that happen before a
/// transaction exists.
///
/// This helper is intentionally limited to the C4 pre-transaction surface:
/// contract/admission rejections and authorization denials. It never fabricates
/// transaction or durable-LSN correlation, and it lets [`EventEmitter`] surface
/// sink/envelope failures to the caller.
pub fn emit_v0_inventory_reserve_stock_pre_transaction_refusal<S: EventSink>(
    request: &InvocationRequest,
    context: &InvocationContext,
    request_id: RequestId,
    session_id: SessionId,
    contract: &ProcedureContract,
    reject: &InvocationReject,
    emitter: &mut EventEmitter<S>,
) -> AndromedaResult<()> {
    let protocol = v0_execute_request_protocol();
    let correlation = EventCorrelation {
        request_id: Some(request_id),
        session_id: Some(session_id),
        contract_hash: Some(request.expected_contract_hash),
        catalog_version: Some(request.catalog_version),
        catalog_object_id: Some(contract.object.object_id),
        transaction_id: None,
        durable_lsn: None,
        protocol: Some(protocol),
    };

    match reject.status {
        CompletionStatus::ContractRejected | CompletionStatus::FailedBeforeTransaction => {
            if let Some(trace) = reject.contract_rejected_trace(
                context.trace_id,
                protocol,
                V0_INVENTORY_RESERVE_STOCK_CONTRACT_KIND,
                reject.status.terminal_code() as u16,
            ) {
                emitter.emit(correlation, TraceEvent::ContractRejected(trace))?;
            }
        },
        CompletionStatus::PermissionDenied => {
            if let Some(trace) = reject.authorization_denial_trace(
                context.trace_id,
                denied_permission_for_context(contract, context),
            ) {
                emitter.emit(correlation, TraceEvent::AuthorizationDenied(trace))?;
            }
        },
        CompletionStatus::SystemUnavailable
        | CompletionStatus::Cancelled
        | CompletionStatus::Poisoned
        | CompletionStatus::Committed
        | CompletionStatus::RolledBack => {},
    }

    if request.invocation_id.get() != 0 {
        emitter.emit(
            correlation,
            TraceEvent::ExecutionTransition(reject.project_transition(
                request.invocation_id,
                context.trace_id,
                Some(request_id),
                Some(session_id),
            )),
        )?;
    }

    Ok(())
}

pub(super) fn v0_pre_transaction_reject_from_error(
    error: &AndromedaError,
) -> Option<InvocationReject> {
    let status = match error.kind() {
        AndromedaErrorKind::Contract => CompletionStatus::ContractRejected,
        AndromedaErrorKind::Security => CompletionStatus::PermissionDenied,
        _ => return None,
    };

    Some(InvocationReject {
        status,
        reason: error.message().to_string(),
    })
}

fn v0_execute_request_protocol() -> ProtocolCorrelation {
    ProtocolCorrelation {
        protocol_version: Some(1),
        stream_id: None,
        stream_role: Some(StreamRole::CommandBidirectional as u16),
        frame_type: Some(FrameType::RpcExecuteRequest.wire_code() as u16),
        payload_kind: Some(FrameType::RpcExecuteRequest.wire_code() as u16),
        sequence: Some(1),
    }
}

fn denied_permission_for_context(
    contract: &ProcedureContract,
    context: &InvocationContext,
) -> String {
    contract
        .required_permissions
        .iter()
        .find(|permission| !context.grants(permission))
        .cloned()
        .unwrap_or_else(|| "unknown required permission".to_string())
}
