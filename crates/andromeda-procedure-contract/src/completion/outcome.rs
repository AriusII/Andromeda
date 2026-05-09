use super::status::CompletionTerminalCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionOutcome {
    NotStarted,
    Committed,
    RolledBack,
    Failed,
    Cancelled,
}

/// Canonical lockstep matrix for transaction outcome terminal codes embedded
/// in procedure completion envelopes.
pub const TRANSACTION_OUTCOME_TERMINAL_CODES: &[(TransactionOutcome, CompletionTerminalCode)] = &[
    (TransactionOutcome::NotStarted, 1),
    (TransactionOutcome::Committed, 2),
    (TransactionOutcome::RolledBack, 3),
    (TransactionOutcome::Failed, 4),
    (TransactionOutcome::Cancelled, 5),
];

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

    /// Reverse lookup from a stable wire code to the in-memory transaction
    /// outcome.
    pub const fn from_terminal_code(code: CompletionTerminalCode) -> Option<Self> {
        match code {
            1 => Some(Self::NotStarted),
            2 => Some(Self::Committed),
            3 => Some(Self::RolledBack),
            4 => Some(Self::Failed),
            5 => Some(Self::Cancelled),
            _ => None,
        }
    }
}
