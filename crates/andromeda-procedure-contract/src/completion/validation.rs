use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};

use super::{
    COMPLETION_ENVELOPE_CONTRACT_VERSION, CompletionProtocolVersion, RpcCompletion,
    RpcCompletionStatus, TransactionOutcome,
};

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
                self.validate_optional_transactional_binding("cancelled RPC completion")?;
            }
            RpcCompletionStatus::Poisoned | RpcCompletionStatus::SystemUnavailable => {
                if self.transaction_outcome == TransactionOutcome::Committed {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        "failed RPC completion must not declare committed transaction outcome",
                    ));
                }
                self.validate_optional_transactional_binding("failed RPC completion")?;
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

    /// Tighten Poisoned/Cancelled/SystemUnavailable: when the transaction
    /// outcome reports a terminal transaction state, require matching
    /// transaction id and nonzero durable LSN evidence (so a failure cannot
    /// claim transactional terminal status without durable proof). When the
    /// outcome is `NotStarted`, no transaction evidence may be carried.
    fn validate_optional_transactional_binding(&self, label: &str) -> AndromedaResult<()> {
        match self.transaction_outcome {
            TransactionOutcome::NotStarted => {
                if self.tx_id.is_some() || self.durable_lsn.is_some() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        format!(
                            "{label} declaring transaction not started must not carry transaction evidence"
                        ),
                    ));
                }
                Ok(())
            }
            TransactionOutcome::RolledBack => {
                if self.tx_id.is_none() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Transaction,
                        format!(
                            "{label} declaring rolled-back outcome must include transaction id"
                        ),
                    ));
                }
                match self.durable_lsn {
                    Some(lsn) if lsn > 0 => Ok(()),
                    _ => Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        format!(
                            "{label} declaring rolled-back outcome must include nonzero durable LSN evidence"
                        ),
                    )),
                }
            }
            TransactionOutcome::Cancelled | TransactionOutcome::Failed => {
                // Non-durable transient outcomes may not carry durable LSN
                // claims. Tx id remains optional for diagnostic correlation.
                if self.durable_lsn.is_some() {
                    return Err(AndromedaError::new(
                        AndromedaErrorKind::Storage,
                        format!(
                            "{label} declaring non-durable transaction outcome must not carry durable LSN evidence"
                        ),
                    ));
                }
                Ok(())
            }
            TransactionOutcome::Committed => Ok(()),
        }
    }

    /// Validate the completion against the negotiated protocol version. Use
    /// this at the protocol boundary to detect version drift between the
    /// completion-emitting executor and the wire envelope it travels in.
    pub fn validate_for_protocol_version<P>(&self, protocol_version: P) -> AndromedaResult<()>
    where
        P: CompletionProtocolVersion,
    {
        protocol_version.validate_completion_protocol_version()?;
        if !COMPLETION_ENVELOPE_CONTRACT_VERSION.is_compatible_with_protocol(
            protocol_version.completion_protocol_major(),
            protocol_version.completion_protocol_minor(),
        ) {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Protocol,
                "RPC completion envelope version drift against negotiated protocol version",
            ));
        }
        self.validate()
    }
}
