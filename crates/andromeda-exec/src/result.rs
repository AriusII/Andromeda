use andromeda_core::{AndromedaResult, InvocationId};
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
        crate::services::ResultValidationService::validate_before_payload(self)
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
    use andromeda_core::AndromedaErrorKind;

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

    #[test]
    fn result_metadata_enforces_declared_cardinality_bounds() {
        for (cardinality, row_count) in [
            (Cardinality::One, 0),
            (Cardinality::One, 2),
            (Cardinality::OptionalOne, 2),
            (Cardinality::NonEmptyMany, 0),
        ] {
            let metadata = ResultStreamMetadata {
                stream_id: 1,
                row_count_exact: Some(row_count),
                column_count: 2,
                cardinality,
            };

            assert_eq!(
                metadata.validate_before_payload().unwrap_err().kind(),
                AndromedaErrorKind::Contract
            );
        }

        let many = ResultStreamMetadata {
            stream_id: 1,
            row_count_exact: None,
            column_count: 2,
            cardinality: Cardinality::Many,
        };
        assert!(many.validate_before_payload().is_ok());
    }
}
