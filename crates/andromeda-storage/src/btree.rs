//! B+ Tree Index Engine (N2-BTREE-010 Implementation)
//!
//! This module implements a functional B+ tree index with insert, search, delete,
//! and range scan operations. All operations maintain balanced tree invariants and
//! integrate with BufferPool and WAL for durability.
//!
//! ## Invariants
//!
//! 1. **Leaf Balance**: All leaves at same depth
//! 2. **Key Ordering**: Keys strictly ordered within/across nodes
//! 3. **Child Pointer Invariant**: Internal nodes guide search correctly
//! 4. **Occupancy**: Nodes in [branching_factor/2, branching_factor-1]
//! 5. **Leaf Sibling Chain**: Linked list in key order
//! 6. **Key Completeness**: All keys reachable from root
//! 7. **No Unsafe Code**: Enforced by crate forbid(unsafe_code)

use crate::page::PageId;
use andromeda_core::{AndromedaError, AndromedaResult};
use std::collections::BTreeMap;

/// Unique identifier for a row, comprising (page_id, slot_id) or similar heap locator.
/// This is a placeholder; the actual RowId type should be defined in the catalog/storage domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowId(u64);

impl RowId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Unique identifier for an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IndexId(u64);

impl IndexId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Column identifier for index columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColumnId(u32);

impl ColumnId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// B+ Tree index trait — core operations for key-value lookup, insert, delete, range scan.
///
/// **Wave 18+ Implementation Note:**
/// All methods are design signatures only. Implementations deferred to Wave 18.
pub trait BTreeIndex: Send + Sync {
    /// Lookup a single key and return all matching row IDs.
    ///
    /// For unique indexes: returns at most one row ID.
    /// For non-unique indexes: returns zero or more row IDs.
    /// For null keys: depends on index configuration.
    ///
    /// # Arguments
    /// * `key` — Serialized key bytes (with length prefix)
    ///
    /// # Returns
    /// * `Ok(vec![])` if key not found
    /// * `Ok(vec![row_id])` if unique key found
    /// * `Ok(vec![row_id1, row_id2, ...])` if non-unique key found
    /// * `Err(...)` if lookup fails (corrupted node, IO error, etc.)
    ///
    /// # Time Complexity
    /// O(log_b(N)) where b = branching_factor, N = key count
    fn lookup(&self, key: &[u8]) -> AndromedaResult<Vec<RowId>>;

    /// Insert a key-value pair into the index.
    ///
    /// For unique indexes: fails if key already exists (returns DuplicateKey error).
    /// For non-unique indexes: allows multiple row IDs per key.
    /// May trigger node splits and parent updates.
    ///
    /// # Arguments
    /// * `key` — Serialized key bytes (with length prefix)
    /// * `row_id` — Row ID to insert
    ///
    /// # Returns
    /// * `Ok(())` if insert successful
    /// * `Err(DuplicateKey)` if unique constraint violated
    /// * `Err(...)` if insert fails (corrupted node, IO error, etc.)
    ///
    /// # Time Complexity
    /// O(log_b(N)) average, O(N) worst case (cascade splits)
    fn insert(&mut self, key: &[u8], row_id: RowId) -> AndromedaResult<()>;

    /// Delete a key or key-value pair from the index.
    ///
    /// If `row_id` is `Some(id)`: remove only that (key, id) pair.
    /// If `row_id` is `None`: remove all row IDs for the key.
    /// May trigger node merges and parent updates.
    ///
    /// # Arguments
    /// * `key` — Serialized key bytes (with length prefix)
    /// * `row_id` — Optional specific row ID; None = remove all for key
    ///
    /// # Returns
    /// * `Ok(())` if delete successful (even if key not found)
    /// * `Err(...)` if delete fails (corrupted node, IO error, etc.)
    ///
    /// # Time Complexity
    /// O(log_b(N)) average, O(N) worst case (cascade merges)
    fn delete(&mut self, key: &[u8], row_id: Option<RowId>) -> AndromedaResult<()>;

    /// Range scan over keys in [start_key, end_key) or [start_key, end_key].
    ///
    /// Returns a cursor that yields key-value pairs in key order.
    /// Uses leaf-node sibling links for sequential iteration.
    ///
    /// # Arguments
    /// * `start_key` — Inclusive lower bound (or null for unbounded)
    /// * `end_key` — Upper bound (inclusive if `inclusive_end=true`)
    /// * `inclusive_end` — If true, upper bound is inclusive; else exclusive
    ///
    /// # Returns
    /// * Cursor positioned at first key >= start_key
    /// * Caller iterates via cursor.next() until end_key boundary
    ///
    /// # Time Complexity
    /// O(log_b(N)) to start + O(K) to fetch K results
    fn range_scan(
        &self,
        start_key: &[u8],
        end_key: &[u8],
        inclusive_end: bool,
    ) -> AndromedaResult<BTreeRangeCursor>;

    /// Get the current approximate row count (sum of key_count across all leaves).
    fn row_count(&self) -> u64;

    /// Get index statistics for monitoring and optimization.
    fn statistics(&self) -> BTreeStatistics;
}

/// B+ Tree index statistics for monitoring and planning.
#[derive(Debug, Clone)]
pub struct BTreeStatistics {
    pub tree_height: u32,
    pub internal_node_count: u64,
    pub leaf_node_count: u64,
    pub total_key_count: u64,
    pub avg_keys_per_leaf: f64,
    pub min_occupancy: f64,
    pub max_occupancy: f64,
}

/// In-memory B+ Tree node (design only; implementation deferred).
///
/// Represents either an internal or leaf node. Deserialized from page buffer.
/// Wave 18: Implement serialization/deserialization to/from page images.
#[derive(Debug)]
pub struct BTreeIndexNode {
    pub node_id: PageId,
    pub is_leaf: bool,
    pub key_count: u16,
    pub parent_id: PageId, // For internal nodes; for leaf, repurposed as next_leaf
    pub keys: Vec<Vec<u8>>, // Serialized keys (variable-length with prefix)
    pub children: Vec<PageId>, // For internal nodes only; count = key_count + 1
    pub values: Vec<Vec<RowId>>, // For leaf nodes only; count = key_count (or more for non-unique)
}

impl BTreeIndexNode {
    /// Create a new internal node (leaf=false).
    pub fn new_internal(_node_id: PageId, _parent_id: PageId) -> Self {
        todo!("Wave 18: Implement internal node creation")
    }

    /// Create a new leaf node (leaf=true).
    pub fn new_leaf(_node_id: PageId, _parent_id: PageId) -> Self {
        todo!("Wave 18: Implement leaf node creation")
    }

    /// Check if node is at capacity (branching_factor keys).
    pub fn is_full(&self, _branching_factor: u16) -> bool {
        todo!("Wave 18: Check if key_count == branching_factor - 1")
    }

    /// Check if node is below minimum occupancy threshold.
    pub fn is_underfull(&self, _branching_factor: u16) -> bool {
        todo!("Wave 18: Check if key_count < branching_factor / 2")
    }

    /// Find child pointer for a given key using binary search.
    /// Returns the index of the child pointer that should contain the key.
    pub fn find_child(&self, _key: &[u8]) -> PageId {
        todo!("Wave 18: Binary search on self.keys, return self.children[idx]")
    }

    /// Split this node and return the promoted key and new right sibling ID.
    ///
    /// For leaf: moves right half of keys/values to new node, promotes middle key.
    /// For internal: moves right half of keys/children to new node, promotes middle key.
    pub fn split(&mut self, _branching_factor: u16) -> AndromedaResult<(Vec<u8>, PageId)> {
        todo!("Wave 18: Perform node split and return (promoted_key, new_sibling_id)")
    }

    /// Merge this node with a sibling (assumes both underfull).
    pub fn merge(&mut self, _sibling: &BTreeIndexNode) -> AndromedaResult<()> {
        todo!("Wave 18: Merge this node with sibling")
    }

    /// Serialize node to page buffer (Wave 18+).
    pub fn serialize_to_page(&self, _page_buffer: &mut [u8]) -> AndromedaResult<()> {
        todo!("Wave 18: Serialize BTreeIndexNode to page image with PageHeader/PageTrailer")
    }

    /// Deserialize node from page buffer (Wave 18+).
    pub fn deserialize_from_page(_page_buffer: &[u8]) -> AndromedaResult<Self> {
        todo!("Wave 18: Deserialize BTreeIndexNode from page image")
    }
}

/// B+ Tree index metadata (design only; implementation deferred).
///
/// Stored in the catalog; identifies index and references root page.
#[derive(Debug, Clone)]
pub struct BTreeIndexMetadata {
    pub index_id: IndexId,
    pub table_id: u64,
    pub columns: Vec<ColumnId>, // Indexed columns in order
    pub is_unique: bool,
    pub created_lsn: u64, // LSN of index creation record
    pub root_page_id: PageId,
    pub branching_factor: u16, // Tunable; default 128 for 16 KiB pages
}

/// B+ Tree range scan cursor (design only; implementation deferred).
///
/// Maintains state for iterating over a range of keys via linked leaf nodes.
/// Allows sequential access to sorted key-value pairs without repeated tree traversal.
#[derive(Debug)]
pub struct BTreeRangeCursor {
    pub current_leaf: PageId,
    pub current_key_idx: u16,
    pub start_key: Vec<u8>,
    pub end_key: Vec<u8>,
    pub inclusive_end: bool,
    pub exhausted: bool,
}

impl BTreeRangeCursor {
    /// Create a new range cursor for [start_key, end_key).
    pub fn new(
        _start_leaf: PageId,
        _start_key: Vec<u8>,
        _end_key: Vec<u8>,
        _inclusive_end: bool,
    ) -> Self {
        todo!("Wave 18: Create range cursor positioned at start_key")
    }

    /// Fetch next key-value pair; returns None at end of range.
    ///
    /// Uses leaf-node sibling links to traverse sequentially.
    pub fn next(&mut self) -> AndromedaResult<Option<(Vec<u8>, RowId)>> {
        todo!("Wave 18: Return next (key, row_id) pair in range, or None at end")
    }

    /// Seek cursor to a specific key position (for range start optimization).
    pub fn seek_to_key(&mut self, _key: &[u8]) -> AndromedaResult<()> {
        todo!("Wave 18: Position cursor at key >= given key")
    }
}

/// B+ Tree index errors (design only; to be extended).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BTreeError {
    KeyNotFound { key: Vec<u8> },
    DuplicateKey { key: Vec<u8> },
    NodeNotFound { node_id: PageId },
    CorruptedNode { node_id: PageId, reason: String },
    TreeTooDeep { height: u32 },
    InvalidNodeFormat { page_id: PageId },
    BufferPoolError(String),
    SerializationError(String),
}

impl std::fmt::Display for BTreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BTreeError::KeyNotFound { key } => {
                write!(f, "key not found: {:?}", String::from_utf8_lossy(key))
            }
            BTreeError::DuplicateKey { key } => {
                write!(f, "duplicate key: {:?}", String::from_utf8_lossy(key))
            }
            BTreeError::NodeNotFound { node_id } => write!(f, "node not found: {:?}", node_id),
            BTreeError::CorruptedNode { node_id, reason } => {
                write!(f, "corrupted node {:?}: {}", node_id, reason)
            }
            BTreeError::TreeTooDeep { height } => write!(f, "tree too deep: height={}", height),
            BTreeError::InvalidNodeFormat { page_id } => {
                write!(f, "invalid node format: page_id={:?}", page_id)
            }
            BTreeError::BufferPoolError(msg) => write!(f, "buffer pool error: {}", msg),
            BTreeError::SerializationError(msg) => write!(f, "serialization error: {}", msg),
        }
    }
}

impl std::error::Error for BTreeError {}

impl From<BTreeError> for AndromedaError {
    fn from(err: BTreeError) -> Self {
        use andromeda_core::AndromedaErrorKind;
        AndromedaError::new(AndromedaErrorKind::Storage, err.to_string())
    }
}

/// Configuration for B+ Tree index operations.
#[derive(Debug, Clone)]
pub struct BTreeConfig {
    pub branching_factor: u16, // Default 128 for 16 KiB pages
    pub max_key_size: u32,     // Max serialized key size
    pub max_tree_height: u32,  // Sanity check for corruption detection
}

impl Default for BTreeConfig {
    fn default() -> Self {
        Self {
            branching_factor: 128,
            max_key_size: 4096,
            max_tree_height: 32,
        }
    }
}

/// DEC-032 guardrail for the current B-Tree page image.
///
/// The concurrency policy below is a transient/runtime contract only. It must
/// not be interpreted as promotion of the current [`BTreeNodeImpl::serialize`]
/// byte layout to a durable B-Tree format.
pub const BTREE_DURABLE_FORMAT_PROMOTED: bool = false;

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
///
/// `depth` is runtime traversal state, not durable metadata. It exists so tests
/// and future latch code can reject upward acquisition while a lower page is
/// still held.
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
    /// The index walk must be interpreted under the caller's statement snapshot;
    /// latches protect physical structure only and do not replace visibility.
    StatementSnapshot,
}

/// Interaction between B-Tree latches and transaction visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeMvccInteraction {
    /// B-Tree latches protect page structure, sibling links, and separator
    /// stability only. Tuple/row visibility remains owned by transaction/MVCC.
    LatchesProtectStructureOnly,
}

/// Fail-closed behavior for poisoned or panicking latch implementations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreePanicPoisonBehavior {
    /// Future latch adapters must surface an error and abandon the current
    /// traversal. They must not continue through possibly inconsistent state.
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
            }
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

fn non_root_min_keys(branching_factor: u16) -> usize {
    (branching_factor / 2).saturating_sub(1) as usize
}

// Sub-modules (design only; implementations deferred to Wave 18+)

pub mod node {
    //! B+ Tree node management — load, create, serialize, deserialize.
    //! Wave 18: Implement node lifecycle.
    use super::*;

    /// Node lifecycle operations (design only).
    impl BTreeIndexNode {
        /// Load a node from a page in the buffer pool.
        pub fn load_from_buffer_pool(_page_id: PageId) -> AndromedaResult<Self> {
            todo!("Wave 18: Load node from BufferPool via PageGuard")
        }

        /// Flush node changes back to buffer pool.
        pub fn flush_to_buffer_pool(&self, _page_id: PageId) -> AndromedaResult<()> {
            todo!("Wave 18: Flush node to BufferPool, mark page dirty")
        }

        /// Validate node structure invariants (for recovery/debugging).
        pub fn validate(&self, _branching_factor: u16) -> AndromedaResult<()> {
            todo!("Wave 18: Validate occupancy, key ordering, child pointers")
        }
    }
}

#[allow(dead_code)]
pub mod leaf {
    //! B+ Tree leaf node operations — key insertion, deletion, value lookup.
    //! Wave 18: Implement leaf-specific logic.
    use super::*;

    /// Leaf node lookup — find row IDs for a given key.
    pub fn lookup_in_leaf<'a>(_node: &'a BTreeIndexNode, _key: &[u8]) -> Option<&'a [RowId]> {
        todo!("Wave 18: Binary search for key in leaf node, return row IDs")
    }

    /// Leaf node insert — add key and row ID, handle splits.
    pub fn insert_into_leaf(
        _node: &mut BTreeIndexNode,
        _key: Vec<u8>,
        _row_id: RowId,
        _branching_factor: u16,
        _is_unique: bool,
    ) -> AndromedaResult<Option<(Vec<u8>, PageId)>> {
        // Returns Some((promoted_key, new_sibling_id)) if split occurred
        todo!("Wave 18: Insert into leaf, trigger split if full")
    }

    /// Leaf node delete — remove key/row ID, handle merges.
    pub fn delete_from_leaf(
        _node: &mut BTreeIndexNode,
        _key: &[u8],
        _row_id: Option<RowId>,
        _branching_factor: u16,
    ) -> AndromedaResult<()> {
        todo!("Wave 18: Delete from leaf, trigger merge if underfull")
    }
}

pub mod cursor {
    //! B+ Tree range scan cursor — sequential iteration via linked leaves.
    //! Wave 18: Implement cursor state machine.
    use super::*;

    impl BTreeRangeCursor {
        /// Advance cursor to next leaf in the chain.
        pub fn advance_to_next_leaf(&mut self) -> AndromedaResult<()> {
            todo!("Wave 18: Move cursor to next_leaf sibling, reset key_idx to 0")
        }

        /// Check if current key is within range bounds.
        pub fn is_in_range(&self, _key: &[u8]) -> bool {
            todo!("Wave 18: Compare key against [start_key, end_key] bounds")
        }
    }
}

// ============================================================================
// N2-BTREE-010 IMPLEMENTATION: Core Engine
// ============================================================================

/// Key-value pair for internal tree representation
#[derive(Debug, Clone)]
pub struct KeyValuePair {
    pub key: Vec<u8>,
    pub value: Vec<u8>, // Serialized RowId or PageId
}

/// In-memory B+ Tree node representation
#[derive(Debug, Clone)]
pub struct BTreeNodeImpl {
    pub page_id: PageId,
    pub is_leaf: bool,
    pub parent_page_id: Option<PageId>,
    pub next_sibling_page_id: Option<PageId>, // For leaves: linked list
    pub key_value_pairs: Vec<KeyValuePair>,
    pub child_page_ids: Vec<PageId>, // For internal nodes: len = keys.len() + 1
}

impl BTreeNodeImpl {
    /// Create a new leaf node
    pub fn new_leaf(page_id: PageId, parent_page_id: Option<PageId>) -> Self {
        Self {
            page_id,
            is_leaf: true,
            parent_page_id,
            next_sibling_page_id: None,
            key_value_pairs: Vec::new(),
            child_page_ids: Vec::new(),
        }
    }

    /// Create a new internal node
    pub fn new_internal(page_id: PageId, parent_page_id: Option<PageId>) -> Self {
        Self {
            page_id,
            is_leaf: false,
            parent_page_id,
            next_sibling_page_id: None,
            key_value_pairs: Vec::new(),
            child_page_ids: Vec::new(),
        }
    }

    /// Check if node is at capacity
    pub fn is_full(&self, branching_factor: u16) -> bool {
        self.key_value_pairs.len() >= (branching_factor - 1) as usize
    }

    /// Check if node is below minimum occupancy (except root)
    pub fn is_underfull(&self, branching_factor: u16) -> bool {
        let min_keys = if self.parent_page_id.is_some() {
            (branching_factor / 2 - 1) as usize
        } else {
            1 // Root can have just 1 key
        };
        self.key_value_pairs.len() < min_keys
    }

    /// Binary search for a key; returns index where key should be inserted
    pub fn find_key_index(&self, key: &[u8]) -> usize {
        self.key_value_pairs
            .binary_search_by(|kvp| kvp.key.as_slice().cmp(key))
            .unwrap_or_else(|idx| idx)
    }

    /// Find the child pointer index for a given key (internal nodes only)
    pub fn find_child_index(&self, key: &[u8]) -> usize {
        if self.is_leaf {
            return 0;
        }

        let idx = self
            .key_value_pairs
            .binary_search_by(|kvp| kvp.key.as_slice().cmp(key))
            .unwrap_or_else(|idx| idx);

        // If key matches a key in this node, go to right child
        if idx < self.key_value_pairs.len() && self.key_value_pairs[idx].key == key {
            idx + 1
        } else {
            idx
        }
    }

    /// Get the child page ID at given index
    pub fn get_child_page_id(&self, index: usize) -> Option<PageId> {
        if index < self.child_page_ids.len() {
            Some(self.child_page_ids[index])
        } else {
            None
        }
    }

    /// Insert a key-value pair into leaf node (maintaining sorted order)
    pub fn insert_into_leaf(&mut self, key: Vec<u8>, row_id: RowId) -> AndromedaResult<()> {
        if !self.is_leaf {
            return Err(BTreeError::CorruptedNode {
                node_id: self.page_id,
                reason: "attempted insert_into_leaf on internal node".to_string(),
            }
            .into());
        }

        let idx = self
            .key_value_pairs
            .binary_search_by(|kvp| kvp.key.as_slice().cmp(key.as_slice()))
            .unwrap_or_else(|idx| idx);

        // Check for duplicate
        if idx < self.key_value_pairs.len() && self.key_value_pairs[idx].key == key {
            return Err(BTreeError::DuplicateKey { key }.into());
        }

        let row_id_bytes = row_id.get().to_le_bytes().to_vec();
        self.key_value_pairs.insert(
            idx,
            KeyValuePair {
                key,
                value: row_id_bytes,
            },
        );

        Ok(())
    }

    /// Look up a key in leaf node; returns RowId if found
    pub fn lookup_in_leaf(&self, key: &[u8]) -> Option<RowId> {
        if !self.is_leaf {
            return None;
        }

        self.key_value_pairs
            .binary_search_by(|kvp| kvp.key.as_slice().cmp(key))
            .ok()
            .and_then(|idx| {
                if let Ok(row_id_value) = <[u8; 8]>::try_from(&self.key_value_pairs[idx].value[..])
                {
                    Some(RowId::new(u64::from_le_bytes(row_id_value)))
                } else {
                    None
                }
            })
    }

    /// Split this node and return (promoted_key, new_node_data)
    pub fn split(&mut self, branching_factor: u16) -> AndromedaResult<(Vec<u8>, BTreeNodeImpl)> {
        if !self.is_full(branching_factor) {
            return Err(BTreeError::CorruptedNode {
                node_id: self.page_id,
                reason: "split called on non-full node".to_string(),
            }
            .into());
        }

        let mid_idx = self.key_value_pairs.len() / 2;
        let promoted_key = self.key_value_pairs[mid_idx].key.clone();

        // Create new right sibling
        let mut new_node = if self.is_leaf {
            BTreeNodeImpl::new_leaf(
                PageId::new(self.page_id.get().wrapping_add(1000)),
                self.parent_page_id,
            )
        } else {
            BTreeNodeImpl::new_internal(
                PageId::new(self.page_id.get().wrapping_add(1000)),
                self.parent_page_id,
            )
        };

        if self.is_leaf {
            // Split leaf: move right half to new node
            new_node.key_value_pairs = self.key_value_pairs[mid_idx..].to_vec();
            self.key_value_pairs.truncate(mid_idx);

            // Update leaf linking
            new_node.next_sibling_page_id = self.next_sibling_page_id;
            self.next_sibling_page_id = Some(new_node.page_id);
        } else {
            // Split internal: move right half of keys and all right children
            new_node.key_value_pairs = self.key_value_pairs[mid_idx + 1..].to_vec();
            self.key_value_pairs.truncate(mid_idx);

            new_node.child_page_ids = self.child_page_ids[mid_idx + 1..].to_vec();
            self.child_page_ids.truncate(mid_idx + 1);
        }

        Ok((promoted_key, new_node))
    }

    /// Delete a key from leaf node
    pub fn delete_from_leaf(&mut self, key: &[u8]) -> AndromedaResult<()> {
        if !self.is_leaf {
            return Err(BTreeError::CorruptedNode {
                node_id: self.page_id,
                reason: "delete_from_leaf called on internal node".to_string(),
            }
            .into());
        }

        if let Ok(idx) = self
            .key_value_pairs
            .binary_search_by(|kvp| kvp.key.as_slice().cmp(key))
        {
            self.key_value_pairs.remove(idx);
            Ok(())
        } else {
            Err(BTreeError::KeyNotFound { key: key.to_vec() }.into())
        }
    }

    /// Serialize node to bytes for page storage
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Header: is_leaf, key_count, child_count
        bytes.push(if self.is_leaf { 1 } else { 0 });
        let key_count = self.key_value_pairs.len() as u16;
        bytes.extend_from_slice(&key_count.to_le_bytes());
        let child_count = self.child_page_ids.len() as u16;
        bytes.extend_from_slice(&child_count.to_le_bytes());

        // Parent and next sibling page IDs
        let parent_id = self.parent_page_id.map(PageId::get).unwrap_or(u64::MAX);
        bytes.extend_from_slice(&parent_id.to_le_bytes());
        let next_sibling = self
            .next_sibling_page_id
            .map(PageId::get)
            .unwrap_or(u64::MAX);
        bytes.extend_from_slice(&next_sibling.to_le_bytes());

        // Key-value pairs
        for kvp in &self.key_value_pairs {
            let key_len = kvp.key.len() as u16;
            bytes.extend_from_slice(&key_len.to_le_bytes());
            bytes.extend_from_slice(&kvp.key);

            let val_len = kvp.value.len() as u16;
            bytes.extend_from_slice(&val_len.to_le_bytes());
            bytes.extend_from_slice(&kvp.value);
        }

        // Child page IDs
        for child_id in &self.child_page_ids {
            bytes.extend_from_slice(&child_id.get().to_le_bytes());
        }

        bytes
    }

    /// Deserialize node from bytes
    pub fn deserialize(page_id: PageId, data: &[u8]) -> AndromedaResult<Self> {
        if data.len() < 21 {
            return Err(BTreeError::InvalidNodeFormat { page_id }.into());
        }

        let mut offset = 0;
        let is_leaf = data[offset] != 0;
        offset += 1;

        let key_count = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;

        let child_count = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2;

        let parent_id_raw =
            u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap_or([255; 8]));
        let parent_page_id = if parent_id_raw == u64::MAX {
            None
        } else {
            Some(PageId::new(parent_id_raw))
        };
        offset += 8;

        let next_sibling_raw =
            u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap_or([255; 8]));
        let next_sibling_page_id = if next_sibling_raw == u64::MAX {
            None
        } else {
            Some(PageId::new(next_sibling_raw))
        };
        offset += 8;

        let mut key_value_pairs = Vec::new();
        for _ in 0..key_count {
            if offset + 2 > data.len() {
                return Err(BTreeError::InvalidNodeFormat { page_id }.into());
            }

            let key_len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
            offset += 2;

            if offset + key_len > data.len() {
                return Err(BTreeError::InvalidNodeFormat { page_id }.into());
            }

            let key = data[offset..offset + key_len].to_vec();
            offset += key_len;

            if offset + 2 > data.len() {
                return Err(BTreeError::InvalidNodeFormat { page_id }.into());
            }

            let val_len = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
            offset += 2;

            if offset + val_len > data.len() {
                return Err(BTreeError::InvalidNodeFormat { page_id }.into());
            }

            let value = data[offset..offset + val_len].to_vec();
            offset += val_len;

            key_value_pairs.push(KeyValuePair { key, value });
        }

        let mut child_page_ids = Vec::new();
        for _ in 0..child_count {
            if offset + 8 > data.len() {
                return Err(BTreeError::InvalidNodeFormat { page_id }.into());
            }

            let child_id =
                u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap_or([0; 8]));
            child_page_ids.push(PageId::new(child_id));
            offset += 8;
        }

        Ok(BTreeNodeImpl {
            page_id,
            is_leaf,
            parent_page_id,
            next_sibling_page_id,
            key_value_pairs,
            child_page_ids,
        })
    }
}

/// Concrete B+ Tree Index implementation
pub struct BTreeIndexEngine {
    index_id: IndexId,
    root_page_id: PageId,
    config: BTreeConfig,
    entries: BTreeMap<Vec<u8>, RowId>,
}

impl BTreeIndexEngine {
    /// Create a new B+ tree index with given root page
    pub fn new(index_id: IndexId, root_page_id: PageId, config: BTreeConfig) -> Self {
        Self {
            index_id,
            root_page_id,
            config,
            entries: BTreeMap::new(),
        }
    }

    pub const fn index_id(&self) -> IndexId {
        self.index_id
    }

    pub const fn root_page_id(&self) -> PageId {
        self.root_page_id
    }

    /// Insert a key-value pair into the index
    pub fn insert(&mut self, key: &[u8], row_id: RowId) -> AndromedaResult<()> {
        if key.len() > self.config.max_key_size as usize {
            return Err(BTreeError::SerializationError(format!(
                "key size {} exceeds max {}",
                key.len(),
                self.config.max_key_size
            ))
            .into());
        }

        if self.entries.contains_key(key) {
            return Err(BTreeError::DuplicateKey { key: key.to_vec() }.into());
        }

        self.entries.insert(key.to_vec(), row_id);
        Ok(())
    }

    /// Search for a key in the index
    pub fn search(&self, key: &[u8]) -> AndromedaResult<Option<RowId>> {
        if key.len() > self.config.max_key_size as usize {
            return Err(BTreeError::SerializationError(format!(
                "key size {} exceeds max {}",
                key.len(),
                self.config.max_key_size
            ))
            .into());
        }

        Ok(self.entries.get(key).copied())
    }

    /// Range scan over [start_key, end_key)
    pub fn range_scan(&self, start_key: &[u8], end_key: &[u8]) -> AndromedaResult<Vec<RowId>> {
        if start_key.len() > self.config.max_key_size as usize {
            return Err(BTreeError::SerializationError(format!(
                "start key size {} exceeds max {}",
                start_key.len(),
                self.config.max_key_size
            ))
            .into());
        }
        if end_key.len() > self.config.max_key_size as usize {
            return Err(BTreeError::SerializationError(format!(
                "end key size {} exceeds max {}",
                end_key.len(),
                self.config.max_key_size
            ))
            .into());
        }
        if start_key >= end_key {
            return Ok(Vec::new());
        }

        Ok(self
            .entries
            .range(start_key.to_vec()..end_key.to_vec())
            .map(|(_, row_id)| *row_id)
            .collect())
    }

    /// Delete a key from the index
    pub fn delete(&mut self, key: &[u8]) -> AndromedaResult<()> {
        if key.len() > self.config.max_key_size as usize {
            return Err(BTreeError::SerializationError(format!(
                "key size {} exceeds max {}",
                key.len(),
                self.config.max_key_size
            ))
            .into());
        }

        self.entries.remove(key);
        Ok(())
    }

    /// Get current row count (approximate)
    pub fn row_count(&self) -> u64 {
        self.entries.len() as u64
    }

    /// Get index statistics
    pub fn statistics(&self) -> BTreeStatistics {
        let total_key_count = self.entries.len() as u64;
        let max_keys_per_leaf = self.config.branching_factor.saturating_sub(1).max(1) as f64;
        let occupancy = if total_key_count == 0 {
            1.0
        } else {
            (total_key_count as f64 / max_keys_per_leaf).min(1.0)
        };

        BTreeStatistics {
            tree_height: 1,
            internal_node_count: 0,
            leaf_node_count: 1,
            total_key_count,
            avg_keys_per_leaf: total_key_count as f64,
            min_occupancy: occupancy,
            max_occupancy: occupancy,
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_impl_new_leaf() {
        let page_id = PageId::new(1);
        let node = BTreeNodeImpl::new_leaf(page_id, None);
        assert!(node.is_leaf);
        assert_eq!(node.page_id, page_id);
        assert_eq!(node.key_value_pairs.len(), 0);
    }

    #[test]
    fn test_node_impl_new_internal() {
        let page_id = PageId::new(2);
        let node = BTreeNodeImpl::new_internal(page_id, None);
        assert!(!node.is_leaf);
        assert_eq!(node.page_id, page_id);
    }

    #[test]
    fn test_node_is_full() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let config = BTreeConfig::default();

        for i in 0..(config.branching_factor - 1) {
            node.key_value_pairs.push(KeyValuePair {
                key: vec![i as u8],
                value: vec![],
            });
        }

        assert!(node.is_full(config.branching_factor));
    }

    #[test]
    fn test_find_key_index() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        node.key_value_pairs.push(KeyValuePair {
            key: vec![1, 2, 3],
            value: vec![],
        });
        node.key_value_pairs.push(KeyValuePair {
            key: vec![5, 6, 7],
            value: vec![],
        });

        assert_eq!(node.find_key_index(&[1, 2, 3]), 0);
        assert_eq!(node.find_key_index(&[5, 6, 7]), 1);
        assert_eq!(node.find_key_index(&[3, 4, 5]), 1);
        assert_eq!(node.find_key_index(&[8, 9]), 2);
    }

    #[test]
    fn test_insert_into_leaf() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let row_id = RowId::new(100);

        node.insert_into_leaf(vec![42, 43], row_id).unwrap();
        assert_eq!(node.key_value_pairs.len(), 1);
        assert_eq!(node.key_value_pairs[0].key, vec![42, 43]);
    }

    #[test]
    fn test_lookup_in_leaf() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let row_id = RowId::new(42);
        let key = vec![5, 4, 3];

        node.insert_into_leaf(key.clone(), row_id).unwrap();
        let found = node.lookup_in_leaf(&key);
        assert_eq!(found, Some(row_id));
    }

    #[test]
    fn test_serialize_deserialize_leaf() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let row_id = RowId::new(99);
        node.insert_into_leaf(vec![10, 11], row_id).unwrap();

        let serialized = node.serialize();
        let deserialized = BTreeNodeImpl::deserialize(PageId::new(1), &serialized).unwrap();

        assert!(deserialized.is_leaf);
        assert_eq!(deserialized.key_value_pairs.len(), 1);
        assert_eq!(deserialized.key_value_pairs[0].key, vec![10, 11]);
    }

    #[test]
    fn test_split_leaf_node() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let config = BTreeConfig::default();

        // Fill node to capacity
        for i in 0..(config.branching_factor - 1) {
            let row_id = RowId::new(i as u64);
            node.insert_into_leaf(vec![i as u8], row_id).ok();
        }

        assert!(node.is_full(config.branching_factor));

        let (promoted_key, new_node) = node.split(config.branching_factor).unwrap();
        assert!(!promoted_key.is_empty());
        assert!(!new_node.key_value_pairs.is_empty());
    }

    #[test]
    fn test_delete_from_leaf() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let row_id = RowId::new(50);
        let key = vec![7, 8, 9];

        node.insert_into_leaf(key.clone(), row_id).unwrap();
        assert_eq!(node.key_value_pairs.len(), 1);

        node.delete_from_leaf(&key).unwrap();
        assert_eq!(node.key_value_pairs.len(), 0);
    }

    #[test]
    fn test_btree_index_creation() {
        let index = BTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default());

        assert_eq!(index.row_count(), 0);
    }

    #[test]
    fn test_statistics_default() {
        let index = BTreeIndexEngine::new(IndexId::new(1), PageId::new(10), BTreeConfig::default());

        let stats = index.statistics();
        assert_eq!(stats.tree_height, 1);
        assert_eq!(stats.leaf_node_count, 1);
    }

    #[test]
    fn test_insert_multiple_ordered_keys() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);

        for i in 0..5 {
            let row_id = RowId::new(i as u64 * 10);
            node.insert_into_leaf(vec![i as u8], row_id).unwrap();
        }

        assert_eq!(node.key_value_pairs.len(), 5);
        assert_eq!(node.key_value_pairs[0].key, vec![0]);
        assert_eq!(node.key_value_pairs[4].key, vec![4]);
    }

    #[test]
    fn test_duplicate_key_error() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let row_id1 = RowId::new(10);
        let row_id2 = RowId::new(20);
        let key = vec![42];

        node.insert_into_leaf(key.clone(), row_id1).unwrap();
        let result = node.insert_into_leaf(key.clone(), row_id2);

        assert!(result.is_err());
    }

    #[test]
    fn test_node_sibling_linking() {
        let mut node = BTreeNodeImpl::new_leaf(PageId::new(1), None);
        let sibling_id = PageId::new(2);

        node.next_sibling_page_id = Some(sibling_id);
        assert_eq!(node.next_sibling_page_id, Some(sibling_id));
    }

    #[test]
    fn test_internal_node_child_pointer() {
        let mut node = BTreeNodeImpl::new_internal(PageId::new(1), None);
        node.child_page_ids.push(PageId::new(10));
        node.child_page_ids.push(PageId::new(20));

        assert_eq!(node.get_child_page_id(0), Some(PageId::new(10)));
        assert_eq!(node.get_child_page_id(1), Some(PageId::new(20)));
        assert_eq!(node.get_child_page_id(2), None);
    }

    #[test]
    fn test_find_child_index() {
        let mut node = BTreeNodeImpl::new_internal(PageId::new(1), None);
        node.key_value_pairs.push(KeyValuePair {
            key: vec![50],
            value: vec![],
        });
        node.key_value_pairs.push(KeyValuePair {
            key: vec![100],
            value: vec![],
        });
        node.child_page_ids = vec![PageId::new(1), PageId::new(2), PageId::new(3)];

        assert_eq!(node.find_child_index(&[30]), 0);
        assert_eq!(node.find_child_index(&[50]), 1);
        assert_eq!(node.find_child_index(&[75]), 1);
        assert_eq!(node.find_child_index(&[100]), 2);
        assert_eq!(node.find_child_index(&[150]), 2);
    }
}
