/// B-Tree operation type for validation gating.
///
/// Used to distinguish read-only operations from deferred mutations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeOperationType {
    Lookup,
    RangeScan,
    Insert,
    Delete,
    Split,
    Merge,
}

impl BTreeOperationType {
    /// Check if this operation is read-only.
    pub const fn is_read_only(&self) -> bool {
        matches!(
            self,
            BTreeOperationType::Lookup | BTreeOperationType::RangeScan
        )
    }

    /// Get the operation name used in validation diagnostics.
    pub const fn name(&self) -> &'static str {
        match self {
            BTreeOperationType::Lookup => "lookup",
            BTreeOperationType::RangeScan => "range_scan",
            BTreeOperationType::Insert => "insert",
            BTreeOperationType::Delete => "delete",
            BTreeOperationType::Split => "split",
            BTreeOperationType::Merge => "merge",
        }
    }
}
