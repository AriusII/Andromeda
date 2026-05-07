use andromeda_error::AndromedaResult;
use andromeda_types::{RequestId, SessionId, TransactionId};

use crate::generated::protocol;
use crate::{
    ResultRowCountSummary, RpcCompletion, RpcCompletionStatus,
    TransactionOutcome as DomainTransactionOutcome,
};

use super::protocol_error;

pub fn validate_generated_rpc_completion(
    completion: &protocol::v1::RpcCompletion,
) -> AndromedaResult<()> {
    let status = validate_completion_status(completion.status)?;
    let transaction_outcome = validate_transaction_outcome(completion.transaction_outcome)?;
    let result_row_counts = completion
        .result_row_counts
        .iter()
        .map(|summary| ResultRowCountSummary {
            result_name: summary.result_name.clone(),
            rows_emitted: summary.rows_emitted,
            row_count_exact: summary.row_count_exact,
        })
        .collect::<Vec<_>>();

    RpcCompletion {
        request_id: completion.request_id.map(RequestId::new),
        session_id: completion.session_id.map(SessionId::new),
        trace_id: completion.trace_id.clone(),
        status,
        transaction_outcome,
        rows_affected: completion.rows_affected,
        result_row_counts,
        tx_id: completion.tx_id.map(TransactionId::new),
        durable_lsn: completion.durable_lsn,
    }
    .validate()
}

fn validate_completion_status(status: i32) -> AndromedaResult<RpcCompletionStatus> {
    let Ok(code) = u32::try_from(status) else {
        return protocol_error("unknown RPC completion status");
    };

    match RpcCompletionStatus::from_terminal_code(code) {
        Some(status) => Ok(status),
        None if status == protocol::v1::rpc_completion::Status::Unspecified as i32 => {
            protocol_error("RPC completion status must be specified")
        }
        None => protocol_error("unknown RPC completion status"),
    }
}

fn validate_transaction_outcome(outcome: i32) -> AndromedaResult<DomainTransactionOutcome> {
    use protocol::v1::rpc_completion::TransactionOutcome;

    match outcome {
        value if value == TransactionOutcome::NotStarted as i32 => {
            Ok(DomainTransactionOutcome::NotStarted)
        }
        value if value == TransactionOutcome::Committed as i32 => {
            Ok(DomainTransactionOutcome::Committed)
        }
        value if value == TransactionOutcome::RolledBack as i32 => {
            Ok(DomainTransactionOutcome::RolledBack)
        }
        value if value == TransactionOutcome::Failed as i32 => Ok(DomainTransactionOutcome::Failed),
        value if value == TransactionOutcome::Cancelled as i32 => {
            Ok(DomainTransactionOutcome::Cancelled)
        }
        value if value == TransactionOutcome::Unspecified as i32 => {
            protocol_error("RPC completion transaction outcome must be specified")
        }
        _ => protocol_error("unknown RPC completion transaction outcome"),
    }
}
