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

        match self.status {
            RpcCompletionStatus::Committed => {
                self.validate_transactional_completion(
                    TransactionOutcome::Committed,
                    "committed RPC completion",
                )?;
            }
            RpcCompletionStatus::RolledBack => {
                self.validate_transactional_completion(
                    TransactionOutcome::RolledBack,
                    "rolled-back RPC completion",
                )?;

                if self.rows_affected != Some(0) {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        "rolled-back RPC completion must report zero rows affected",
                    ));
                }
            }
            RpcCompletionStatus::FailedBeforeTransaction
            | RpcCompletionStatus::PermissionDenied
            | RpcCompletionStatus::ContractRejected => {
                self.validate_not_started_completion("pre-transaction RPC completion")?;
            }
            RpcCompletionStatus::Cancelled => {
                if self.transaction_outcome == TransactionOutcome::Committed {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        "cancelled RPC completion must not declare committed transaction outcome",
                    ));
                }
            }
            RpcCompletionStatus::Poisoned | RpcCompletionStatus::SystemUnavailable => {
                if self.transaction_outcome == TransactionOutcome::Committed {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        "failed RPC completion must not declare committed transaction outcome",
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

    fn validate_transactional_completion(
        &self,
        expected_outcome: TransactionOutcome,
        label: &str,
    ) -> AndromedaResult<()> {
        if self.transaction_outcome != expected_outcome {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!("{label} must declare matching transaction outcome"),
            ));
        }

        if self.tx_id.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!("{label} must include transaction id"),
            ));
        }

        match self.durable_lsn {
            Some(lsn) if lsn > 0 => Ok(()),
            _ => Err(AndromedaError::new(
                AndromedaErrorKind::Storage,
                format!("{label} must include nonzero durable LSN evidence"),
            )),
        }
    }

    fn validate_not_started_completion(&self, label: &str) -> AndromedaResult<()> {
        if self.transaction_outcome != TransactionOutcome::NotStarted {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!("{label} must declare transaction not started"),
            ));
        }

        if self.tx_id.is_some() || self.durable_lsn.is_some() || self.rows_affected.is_some() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Transaction,
                format!("{label} must not carry transaction result evidence"),
            ));
        }

        Ok(())
    }
}
