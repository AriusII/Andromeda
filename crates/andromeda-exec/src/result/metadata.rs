use andromeda_core::AndromedaResult;
use andromeda_srpl::Cardinality;
use andromeda_storage::Lsn;
use andromeda_tx::TransactionState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultStreamMetadata {
    pub stream_id: u64,
    pub row_count_exact: Option<u64>,
    /// Optional inclusive upper bound on the number of rows the stream may emit.
    pub row_count_max: Option<u64>,
    pub column_count: u32,
    pub cardinality: Cardinality,
}

impl ResultStreamMetadata {
    /// Construct a metadata header for a stream whose row count is known
    /// exactly before payload emission.
    pub const fn exact(
        stream_id: u64,
        column_count: u32,
        cardinality: Cardinality,
        row_count_exact: u64,
    ) -> Self {
        Self {
            stream_id,
            row_count_exact: Some(row_count_exact),
            row_count_max: Some(row_count_exact),
            column_count,
            cardinality,
        }
    }

    /// Construct a metadata header for a bounded stream.
    pub const fn bounded(
        stream_id: u64,
        column_count: u32,
        cardinality: Cardinality,
        row_count_max: u64,
    ) -> Self {
        Self {
            stream_id,
            row_count_exact: None,
            row_count_max: Some(row_count_max),
            column_count,
            cardinality,
        }
    }

    pub fn validate_before_payload(self) -> AndromedaResult<()> {
        crate::services::ResultValidationService::validate_before_payload(self)
    }

    pub fn validate_completed_stream(self, actual_row_count: u64) -> AndromedaResult<()> {
        crate::services::ResultValidationService::validate_completed_stream(self, actual_row_count)
    }

    /// Bind the terminal completion of this result stream to a transaction
    /// terminal state and durable LSN evidence. Use this at the
    /// completion-emission boundary so a result-stream completion can never
    /// be emitted ahead of (or without) the transaction reaching a terminal
    /// state with durable WAL evidence. The metadata-before-payload contract
    /// and exact row count contract are enforced in the same pass.
    pub fn validate_terminal_completion(
        self,
        transaction_state: TransactionState,
        durable_lsn: Lsn,
        actual_row_count: u64,
    ) -> AndromedaResult<()> {
        crate::result::validate_terminal_completion(
            self,
            transaction_state,
            durable_lsn,
            actual_row_count,
        )
    }
}
