#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorFamily {
    Protocol,
    Authentication,
    Authorization,
    Contract,
    Semantic,
    Execution,
    Transaction,
    Storage,
    Resource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorEnvelope {
    pub family: ErrorFamily,
    pub code: String,
    pub message: String,
    pub transaction_effect: TransactionEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionEffect {
    NoTransaction,
    RollbackRequired,
    FailStop,
}
