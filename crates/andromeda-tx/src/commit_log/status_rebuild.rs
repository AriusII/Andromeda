/// Summary returned after rebuilding MVCC status from local durable records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionStatusRebuild {
    pub committed_restored: usize,
    pub rolled_back_restored: usize,
    pub already_present: usize,
}
