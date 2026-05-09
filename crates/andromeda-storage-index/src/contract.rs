use super::{BTreeStatistics, ColumnId, IndexId, PageId, RowId};
use andromeda_error::AndromedaResult;

/// B-Tree index trait: key-value lookup, mutation, and range scan operations.
pub trait BTreeIndex: Send + Sync {
    /// Lookup a single key and return all matching row IDs.
    ///
    /// For unique indexes: returns at most one row ID.
    /// For non-unique indexes: returns zero or more row IDs.
    /// For null keys: depends on index configuration.
    ///
    /// # Arguments
    /// * `key` - Serialized key bytes with length prefix.
    ///
    /// # Returns
    /// * `Ok(vec![])` if key not found
    /// * `Ok(vec![row_id])` if unique key found
    /// * `Ok(vec![row_id1, row_id2, ...])` if non-unique key found
    /// * `Err(...)` if lookup fails due to corruption, IO, or validation.
    fn lookup(&self, key: &[u8]) -> AndromedaResult<Vec<RowId>>;

    /// Insert a key-value pair into the index.
    fn insert(&mut self, key: &[u8], row_id: RowId) -> AndromedaResult<()>;

    /// Delete a key or key-value pair from the index.
    fn delete(&mut self, key: &[u8], row_id: Option<RowId>) -> AndromedaResult<()>;

    /// Range scan over keys in [start_key, end_key) or [start_key, end_key].
    fn range_scan(
        &self,
        start_key: &[u8],
        end_key: &[u8],
        inclusive_end: bool,
    ) -> AndromedaResult<BTreeRangeCursor>;

    /// Get the current approximate row count.
    fn row_count(&self) -> u64;

    /// Get index statistics for monitoring and optimization.
    fn statistics(&self) -> BTreeStatistics;
}

/// Page-backed B-Tree node shape.
///
/// The structure is available for invariant checks and tests. Durable
/// serialization/mutation functions below fail-stop until the format is
/// promoted.
#[derive(Debug)]
pub struct BTreeIndexNode {
    pub node_id: PageId,
    pub is_leaf: bool,
    pub key_count: u16,
    pub parent_id: PageId,
    pub keys: Vec<Vec<u8>>,
    pub children: Vec<PageId>,
    pub values: Vec<Vec<RowId>>,
}

/// B-Tree index metadata.
///
/// Stored in the catalog; identifies index and references root page.
#[derive(Debug, Clone)]
pub struct BTreeIndexMetadata {
    pub index_id: IndexId,
    pub table_id: u64,
    pub columns: Vec<ColumnId>,
    pub is_unique: bool,
    pub created_lsn: u64,
    pub root_page_id: PageId,
    pub branching_factor: u16,
}

/// B-Tree range scan cursor.
///
/// Maintains state for iterating over a range of keys via linked leaf nodes.
#[derive(Debug)]
pub struct BTreeRangeCursor {
    pub current_leaf: PageId,
    pub current_key_idx: u16,
    pub start_key: Vec<u8>,
    pub end_key: Vec<u8>,
    pub inclusive_end: bool,
    pub exhausted: bool,
}
