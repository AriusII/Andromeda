use super::{BTREE_DURABLE_FORMAT_PROMOTED, BTreeError, PageId};
use andromeda_error::AndromedaResult;

/// Logical operation class for B-Tree latch-coupling decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeOperationKind {
    Lookup,
    Insert,
    Delete,
    RangeScan,
}

/// Transient latch mode required by a B-Tree traversal step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeLatchMode {
    Shared,
    Exclusive,
}

/// Logical position of a page in the B-Tree latch hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeLatchLevel {
    Root,
    Internal,
    Leaf,
    Sibling,
}

/// A transient latch target used by concurrency-policy validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BTreeLatchTarget {
    pub page_id: PageId,
    pub level: BTreeLatchLevel,
    pub depth: u16,
}

impl BTreeLatchTarget {
    pub const fn new(page_id: PageId, level: BTreeLatchLevel, depth: u16) -> Self {
        Self {
            page_id,
            level,
            depth,
        }
    }
}

/// Reason a concurrent B-Tree operation must release its latch path and retry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeRestartReason {
    ChildMaySplit,
    ChildMayMergeOrRedistribute,
    StructureChanged,
    RootChanged,
    LatchUnavailable,
}

/// Range-scan consistency contract for the pre-persistence B-Tree candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeScanConsistency {
    StatementSnapshot,
}

/// Interaction between B-Tree latches and transaction visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeMvccInteraction {
    LatchesProtectStructureOnly,
}

/// Fail-closed behavior for poisoned or panicking latch implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreePanicPoisonBehavior {
    FailClosedReturnError,
}

/// Typed scaffolding for B-Tree lock-coupling/crab traversal.
///
/// This type intentionally carries no lock primitive. It is a policy contract
/// for future buffer-pool/page-latch integration and for regression tests. It
/// does not alter page bytes, WAL records, recovery replay, or manifests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BTreeConcurrencyPolicy {
    pub max_restart_attempts: u8,
    pub scan_consistency: BTreeScanConsistency,
    pub mvcc_interaction: BTreeMvccInteraction,
    pub panic_poison_behavior: BTreePanicPoisonBehavior,
}

impl Default for BTreeConcurrencyPolicy {
    fn default() -> Self {
        Self {
            max_restart_attempts: 8,
            scan_consistency: BTreeScanConsistency::StatementSnapshot,
            mvcc_interaction: BTreeMvccInteraction::LatchesProtectStructureOnly,
            panic_poison_behavior: BTreePanicPoisonBehavior::FailClosedReturnError,
        }
    }
}

impl BTreeConcurrencyPolicy {
    pub fn validate(&self) -> AndromedaResult<()> {
        if self.max_restart_attempts == 0 {
            return Err(BTreeError::SerializationError(
                "B-Tree concurrency policy requires a bounded non-zero restart budget".to_string(),
            )
            .into());
        }
        Ok(())
    }

    /// Latch mode for traversal before a page is known to be unsafe.
    pub const fn descent_latch_mode(operation: BTreeOperationKind) -> BTreeLatchMode {
        match operation {
            BTreeOperationKind::Lookup | BTreeOperationKind::RangeScan => BTreeLatchMode::Shared,
            BTreeOperationKind::Insert | BTreeOperationKind::Delete => BTreeLatchMode::Shared,
        }
    }

    /// Latch mode required before mutating a page image or sibling links.
    pub const fn mutation_latch_mode(operation: BTreeOperationKind) -> BTreeLatchMode {
        match operation {
            BTreeOperationKind::Lookup | BTreeOperationKind::RangeScan => BTreeLatchMode::Shared,
            BTreeOperationKind::Insert | BTreeOperationKind::Delete => BTreeLatchMode::Exclusive,
        }
    }

    /// Returns true when the child can absorb the operation without requiring
    /// parent structure changes after the parent latch is released.
    pub fn child_safe_for_descent(
        operation: BTreeOperationKind,
        child_key_count: usize,
        branching_factor: u16,
        child_is_root: bool,
    ) -> bool {
        let max_keys = branching_factor.saturating_sub(1) as usize;
        match operation {
            BTreeOperationKind::Lookup | BTreeOperationKind::RangeScan => true,
            BTreeOperationKind::Insert => child_key_count < max_keys,
            BTreeOperationKind::Delete => {
                child_is_root || child_key_count > non_root_min_keys(branching_factor)
            },
        }
    }

    /// Returns the restart reason for an unsafe descent, if any.
    pub fn restart_reason_for_unsafe_child(
        operation: BTreeOperationKind,
        child_key_count: usize,
        branching_factor: u16,
        child_is_root: bool,
    ) -> Option<BTreeRestartReason> {
        if Self::child_safe_for_descent(operation, child_key_count, branching_factor, child_is_root)
        {
            return None;
        }

        match operation {
            BTreeOperationKind::Lookup | BTreeOperationKind::RangeScan => None,
            BTreeOperationKind::Insert => Some(BTreeRestartReason::ChildMaySplit),
            BTreeOperationKind::Delete => Some(BTreeRestartReason::ChildMayMergeOrRedistribute),
        }
    }

    /// Validate deadlock-avoidance ordering for a prospective latch acquisition.
    ///
    /// The contract is root-to-leaf only. A new child must be deeper than all
    /// currently held pages. Same-depth sibling movement is permitted only in
    /// increasing `PageId` order. This function does not acquire locks.
    pub fn can_acquire_after(
        &self,
        held: &[BTreeLatchTarget],
        requested: BTreeLatchTarget,
    ) -> bool {
        if requested.page_id.is_zero()
            || held
                .iter()
                .any(|target| target.page_id == requested.page_id)
        {
            return false;
        }

        let Some(max_depth) = held.iter().map(|target| target.depth).max() else {
            return true;
        };

        if requested.depth > max_depth {
            return !held
                .iter()
                .any(|target| target.level == BTreeLatchLevel::Sibling);
        }

        if requested.depth == max_depth && requested.level == BTreeLatchLevel::Sibling {
            let max_page_at_depth = held
                .iter()
                .filter(|target| target.depth == requested.depth)
                .map(|target| target.page_id)
                .max();
            return match max_page_at_depth {
                Some(page_id) => requested.page_id > page_id,
                None => false,
            };
        }

        false
    }

    pub const fn durable_format_promoted(&self) -> bool {
        BTREE_DURABLE_FORMAT_PROMOTED
    }
}

pub(crate) fn non_root_min_keys(branching_factor: u16) -> usize {
    (branching_factor / 2).saturating_sub(1) as usize
}
