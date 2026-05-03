use andromeda_core::{
    AndromedaError, AndromedaErrorKind, AndromedaResult, RequestId, SessionId, TransactionId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcCompletionStatus {
    Committed,
    RolledBack,
    FailedBeforeTransaction,
    Cancelled,
    Poisoned,
    PermissionDenied,
    ContractRejected,
    SystemUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionOutcome {
    NotStarted,
    Committed,
    RolledBack,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRowCountSummary {
    pub result_name: String,
    pub rows_emitted: u64,
    pub row_count_exact: Option<u64>,
}

impl ResultRowCountSummary {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.result_name.trim().is_empty() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result row-count summary name must not be empty",
            ));
        }

        if let Some(exact) = self.row_count_exact
            && exact != self.rows_emitted
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "exact result row count must match emitted rows",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcCompletion {
    pub request_id: Option<RequestId>,
    pub session_id: Option<SessionId>,
    pub trace_id: Option<String>,
    pub status: RpcCompletionStatus,
    pub transaction_outcome: TransactionOutcome,
    pub rows_affected: Option<u64>,
    pub result_row_counts: Vec<ResultRowCountSummary>,
    pub tx_id: Option<TransactionId>,
    pub durable_lsn: Option<u64>,
}

impl RpcCompletion {
    pub fn validate(&self) -> AndromedaResult<()> {
        if matches!(self.trace_id.as_deref(), Some(trace_id) if trace_id.trim().is_empty()) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "trace correlation id must not be empty when present",
            ));
        }

        for summary in &self.result_row_counts {
            summary.validate()?;
        }

        if self.status == RpcCompletionStatus::Committed {
            if self.transaction_outcome != TransactionOutcome::Committed {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "committed RPC completion must declare committed transaction outcome",
                ));
            }

            if self.tx_id.is_none() {
                return Err(AndromedaError::new(
                    AndromedaErrorKind::Transaction,
                    "committed RPC completion must include transaction id",
                ));
            }

            match self.durable_lsn {
                Some(lsn) if lsn > 0 => {}
                _ => {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        "committed RPC completion must include nonzero durable LSN evidence",
                    ));
                }
            }
        }

        if self.transaction_outcome == TransactionOutcome::Committed
            && self.status != RpcCompletionStatus::Committed
        {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                "committed transaction outcome requires committed RPC status",
            ));
        }

        Ok(())
    }
}
