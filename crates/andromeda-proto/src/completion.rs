use andromeda_core::TransactionId;

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
pub struct RpcCompletion {
    pub status: RpcCompletionStatus,
    pub rows_affected: Option<u64>,
    pub tx_id: Option<TransactionId>,
}
