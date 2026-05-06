//! B+ Tree index contracts and the current in-memory engine.
//!
//! The promoted implementation in this module is `InMemoryBTreeIndexEngine`, a
//! bounded `BTreeMap`-backed engine used for contract tests and non-durable
//! execution paths. Page-backed durable B-Tree node mutation remains explicitly
//! fail-stop until the on-page node format, WAL payloads, and recovery handlers
//! are promoted together.
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

/// Unique identifier for a row, comprising a compact heap locator.
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

/// B+ Tree index trait: key-value lookup, mutation, and range scan operations.
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

/// Page-backed B+ Tree node shape.
///
/// The structure is available for invariant checks and tests. Durable
/// serialization/mutation functions below fail-stop until the format is
/// promoted.
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

fn deferred_btree_result<T>(operation: &'static str) -> AndromedaResult<T> {
    use andromeda_core::AndromedaErrorKind;

    Err(AndromedaError::new(
        AndromedaErrorKind::Storage,
        format!(
            "B-Tree durable operation '{operation}' is not promoted; page-backed node format, WAL payload decoding, and idempotent recovery must be implemented before this path can run"
        ),
    ))
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

/// DEC-032/DEC-038 guardrail for page-backed B-Tree durability.
///
/// This remains `false` until the node image, WAL mutation payloads, recovery
/// replay, crash tests, golden vectors, and fuzz coverage are promoted as one
/// contract. Validating `BTreeNodeV1` decode is necessary but not sufficient:
/// visible durable mutation still requires WAL-covered split/merge/insert/delete
/// semantics and idempotent recovery before this gate can open.
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

// Page-backed adapters remain fail-stop until durable B-Tree promotion.

pub mod node;
pub mod node_format_v1;

#[allow(dead_code)]
pub mod leaf;

pub mod cursor;

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

/// In-memory B-Tree index prototype backed by [`BTreeMap`].
///
/// This type is intentionally **not** a durable/page-backed B-Tree engine.
pub struct InMemoryBTreeIndexEngine {
    index_id: IndexId,
    root_page_id: PageId,
    config: BTreeConfig,
    entries: BTreeMap<Vec<u8>, RowId>,
}

impl InMemoryBTreeIndexEngine {
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

#[deprecated(
    since = "0.1.0",
    note = "Use InMemoryBTreeIndexEngine. BTreeIndexEngine is an in-memory BTreeMap-backed prototype and not a durable B-Tree format."
)]
pub type BTreeIndexEngine = InMemoryBTreeIndexEngine;

#[cfg(test)]
mod tests;
