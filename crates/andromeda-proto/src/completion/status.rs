/// Stable wire-aligned terminal completion code for the in-memory
/// `RpcCompletionStatus` enum. Values must match the generated
/// `protocol::v1::rpc_completion::Status` integer codes 1..=8 declared in
/// `crates/andromeda-proto/proto/andromeda/protocol/v1/completion.proto`. Code
/// `0` is reserved for `STATUS_UNSPECIFIED` and is intentionally unreachable
/// from this enum.
///
/// Consumers MUST use this code instead of relying on `Debug`/`Display`
/// projections when emitting telemetry, journaling, or comparing statuses
/// across protocol versions.
pub type CompletionTerminalCode = u32;

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

impl RpcCompletionStatus {
    /// Stable terminal completion code aligned with the generated
    /// `protocol::v1::rpc_completion::Status` integer values.
    pub const fn terminal_code(self) -> CompletionTerminalCode {
        match self {
            Self::Committed => 1,
            Self::RolledBack => 2,
            Self::FailedBeforeTransaction => 3,
            Self::Cancelled => 4,
            Self::Poisoned => 5,
            Self::PermissionDenied => 6,
            Self::ContractRejected => 7,
            Self::SystemUnavailable => 8,
        }
    }

    /// Reverse lookup from a stable wire code to the in-memory status.
    pub const fn from_terminal_code(code: CompletionTerminalCode) -> Option<Self> {
        match code {
            1 => Some(Self::Committed),
            2 => Some(Self::RolledBack),
            3 => Some(Self::FailedBeforeTransaction),
            4 => Some(Self::Cancelled),
            5 => Some(Self::Poisoned),
            6 => Some(Self::PermissionDenied),
            7 => Some(Self::ContractRejected),
            8 => Some(Self::SystemUnavailable),
            _ => None,
        }
    }

    /// Returns true when the status reports a transaction that reached a
    /// durable terminal state (committed or rolled back).
    pub const fn is_transactional_terminal(self) -> bool {
        matches!(self, Self::Committed | Self::RolledBack)
    }
}
