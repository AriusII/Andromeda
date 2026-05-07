use andromeda_core::{AndromedaError, AndromedaErrorKind};

/// Error types specific to TxWalAdapterTrait failures.
///
/// New adapter implementations can use these to categorize their errors
/// before converting to `AndromedaError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxWalAdapterError {
    /// Transaction ID does not exist or is not in expected state
    InvalidTransactionState,
    /// WAL append operation failed (disk full, permissions, corruption)
    WalAppendFailed,
    /// WAL flush operation failed (I/O error, network timeout)
    WalFlushFailed,
    /// WAL reported success but did not flush through the commit LSN
    DurableLsnBehindCommit,
    /// WAL replay terminal evidence is not covered by the durable WAL prefix
    DurableLsnBehindTerminal,
    /// Status table operation failed (concurrent modification, corruption)
    StatusTableError,
    /// Internal invariant violated (implementation bug)
    InvariantViolated,
    /// Replay boundary record did not carry a transaction identifier
    MissingReplayTransactionId,
    /// Replay saw more than one TxBegin for the same transaction
    DuplicateBeginRecord,
    /// Replay saw transaction data or a terminal record without TxBegin
    ReplayRecordWithoutBegin,
    /// Replay saw both commit and rollback terminal records
    ConflictingTerminalRecord,
    /// Replay saw transaction data after commit or rollback evidence
    RecordAfterTerminal,
    /// Replay input regressed in LSN order for one transaction
    ReplayLsnRegression,
}

impl TxWalAdapterError {
    pub const fn kind(self) -> AndromedaErrorKind {
        match self {
            Self::InvalidTransactionState
            | Self::MissingReplayTransactionId
            | Self::DuplicateBeginRecord
            | Self::ReplayRecordWithoutBegin
            | Self::ConflictingTerminalRecord
            | Self::RecordAfterTerminal
            | Self::ReplayLsnRegression => AndromedaErrorKind::Transaction,
            Self::WalAppendFailed
            | Self::WalFlushFailed
            | Self::DurableLsnBehindCommit
            | Self::DurableLsnBehindTerminal => AndromedaErrorKind::Storage,
            Self::StatusTableError | Self::InvariantViolated => AndromedaErrorKind::Internal,
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::InvalidTransactionState => "transaction is not in expected state for operation",
            Self::WalAppendFailed => "WAL append failed",
            Self::WalFlushFailed => "WAL flush failed",
            Self::DurableLsnBehindCommit => "durable WAL flush ended before commit LSN",
            Self::DurableLsnBehindTerminal => {
                "durable WAL replay coverage ended before terminal LSN"
            }
            Self::StatusTableError => "transaction status table error",
            Self::InvariantViolated => "TxWalAdapter invariant violation",
            Self::MissingReplayTransactionId => {
                "transaction WAL replay boundary record is missing transaction id"
            }
            Self::DuplicateBeginRecord => "transaction WAL replay saw duplicate TxBegin",
            Self::ReplayRecordWithoutBegin => "transaction WAL replay record has no TxBegin",
            Self::ConflictingTerminalRecord => {
                "transaction WAL replay saw conflicting terminal records"
            }
            Self::RecordAfterTerminal => {
                "transaction WAL replay saw record after terminal boundary"
            }
            Self::ReplayLsnRegression => "transaction WAL replay LSN order regressed",
        }
    }

    /// Convert this error to an AndromedaError with appropriate categorization.
    pub fn into_andromeda_error(self) -> AndromedaError {
        AndromedaError::new(self.kind(), self.message())
    }

    pub fn into_andromeda_error_with_source(self, source: AndromedaError) -> AndromedaError {
        AndromedaError::new(self.kind(), format!("{}: {source}", self.message()))
    }
}

impl From<TxWalAdapterError> for AndromedaError {
    fn from(error: TxWalAdapterError) -> Self {
        error.into_andromeda_error()
    }
}

pub(super) fn tx_adapter_error(error: TxWalAdapterError) -> AndromedaError {
    error.into_andromeda_error()
}
