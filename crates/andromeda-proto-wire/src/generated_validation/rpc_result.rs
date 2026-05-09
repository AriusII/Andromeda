use andromeda_error::AndromedaResult;
use andromeda_procedure_contract::{
    ResultRowCountSummary, RpcCompletion, RpcCompletionStatus, TransactionOutcome,
};
use andromeda_types::{RequestId, SessionId, TransactionId};

use crate::validate_result_batch_payload_parts;

use super::{
    common::{contract_error, protocol_error},
    result_stream::validate_generated_result_streams,
    views::{
        GeneratedResultCompletionPolicyView, GeneratedResultRowCountSummaryView,
        GeneratedRpcBatchView, GeneratedRpcCompletionView, GeneratedRpcMetadataView,
    },
};

pub fn validate_generated_rpc_metadata<T>(metadata: &T) -> AndromedaResult<()>
where
    T: GeneratedRpcMetadataView,
{
    validate_generated_result_streams(metadata.result_streams())?;

    let Some(policy) = metadata.completion_policy() else {
        return contract_error("generated RPC metadata requires completion_policy");
    };

    validate_result_completion_policy(policy)
}

pub fn validate_generated_rpc_batch<T>(batch: &T) -> AndromedaResult<()>
where
    T: GeneratedRpcBatchView,
{
    if batch.result_name().trim().is_empty() {
        return contract_error("generated RPC batch result_name must be non-empty");
    }

    validate_result_batch_payload_parts(
        batch.structured_payload(),
        batch.rows_emitted(),
        batch.row_count_exact(),
    )
}

pub fn validate_generated_rpc_completion<T>(completion: &T) -> AndromedaResult<()>
where
    T: GeneratedRpcCompletionView,
{
    let status = validate_completion_status(completion.status())?;
    let transaction_outcome = validate_transaction_outcome(completion.transaction_outcome())?;
    let result_row_counts = completion
        .result_row_counts()
        .iter()
        .map(|summary| ResultRowCountSummary {
            result_name: summary.result_name().to_string(),
            rows_emitted: summary.rows_emitted(),
            row_count_exact: summary.row_count_exact(),
        })
        .collect::<Vec<_>>();

    RpcCompletion {
        request_id: completion.request_id().map(RequestId::new),
        session_id: completion.session_id().map(SessionId::new),
        trace_id: completion.trace_id().map(str::to_string),
        status,
        transaction_outcome,
        rows_affected: completion.rows_affected(),
        result_row_counts,
        tx_id: completion.tx_id().map(TransactionId::new),
        durable_lsn: completion.durable_lsn(),
    }
    .validate()
}

fn validate_completion_status(status: i32) -> AndromedaResult<RpcCompletionStatus> {
    let Ok(code) = u32::try_from(status) else {
        return protocol_error("unknown RPC completion status");
    };

    match RpcCompletionStatus::from_terminal_code(code) {
        Some(status) => Ok(status),
        None if status == 0 => protocol_error("RPC completion status must be specified"),
        None => protocol_error("unknown RPC completion status"),
    }
}

fn validate_transaction_outcome(outcome: i32) -> AndromedaResult<TransactionOutcome> {
    match outcome {
        1 => Ok(TransactionOutcome::NotStarted),
        2 => Ok(TransactionOutcome::Committed),
        3 => Ok(TransactionOutcome::RolledBack),
        4 => Ok(TransactionOutcome::Failed),
        5 => Ok(TransactionOutcome::Cancelled),
        0 => protocol_error("RPC completion transaction outcome must be specified"),
        _ => protocol_error("unknown RPC completion transaction outcome"),
    }
}

fn validate_result_completion_policy<T>(policy: &T) -> AndromedaResult<()>
where
    T: GeneratedResultCompletionPolicyView,
{
    match policy.completion_shape() {
        1..=3 => Ok(()),
        0 => contract_error("generated RPC metadata completion_shape must be specified"),
        _ => protocol_error("unknown generated RPC metadata completion_shape"),
    }
}
