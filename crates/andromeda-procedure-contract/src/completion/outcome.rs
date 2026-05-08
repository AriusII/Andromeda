use super::status::CompletionTerminalCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionOutcome {
    NotStarted,
    Committed,
    RolledBack,
    Failed,
    Cancelled,
}

impl TransactionOutcome {
    /// Stable wire-aligned terminal code matching
    /// `protocol::v1::rpc_completion::TransactionOutcome` values.
    pub const fn terminal_code(self) -> CompletionTerminalCode {
        match self {
            Self::NotStarted => 1,
            Self::Committed => 2,
            Self::RolledBack => 3,
            Self::Failed => 4,
            Self::Cancelled => 5,
        }
    }
}
