use andromeda_core::{AndromedaError, AndromedaErrorKind, AndromedaResult, InvocationId};
use andromeda_observe::TraceId;
use andromeda_srpl::Cardinality;
use andromeda_storage::Lsn;
use andromeda_tx::TransactionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultStreamMetadata {
    pub stream_id: u64,
    pub row_count_exact: Option<u64>,
    pub column_count: u32,
    pub cardinality: Cardinality,
}

impl ResultStreamMetadata {
    pub fn validate_before_payload(self) -> AndromedaResult<()> {
        if self.column_count == 0 {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream metadata must declare at least one column",
            ));
        }

        if self.cardinality.requires_exact_row_count() && self.row_count_exact.is_none() {
            return Err(AndromedaError::new(
                AndromedaErrorKind::Contract,
                "result stream requires RowCountExact before payload",
            ));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionStatus {
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
pub struct InvocationCompletion {
    pub invocation_id: InvocationId,
    pub status: CompletionStatus,
    pub rows_affected: Option<u64>,
    pub transaction_state: Option<TransactionState>,
    pub durable_lsn: Option<Lsn>,
    pub trace_id: TraceId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_metadata_keeps_shape_before_payload_contract() {
        let metadata = ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: Some(3),
            column_count: 2,
            cardinality: Cardinality::NonEmptyMany,
        };

        assert!(metadata.validate_before_payload().is_ok());
    }

    #[test]
    fn exact_cardinality_requires_row_count_before_payload() {
        let metadata = ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: None,
            column_count: 2,
            cardinality: Cardinality::One,
        };

        assert_eq!(
            metadata.validate_before_payload().unwrap_err().kind(),
            AndromedaErrorKind::Contract
        );
    }
}
