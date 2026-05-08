use super::{BTreeConfig, BTreeError, BTreeStatistics, IndexId, KeyValuePair, PageId, RowId};
use andromeda_core::AndromedaResult;
use std::collections::BTreeMap;

/// In-memory B-Tree node representation.
#[derive(Debug, Clone)]
pub struct BTreeNodeImpl {
    pub page_id: PageId,
    pub is_leaf: bool,
    pub parent_page_id: Option<PageId>,
    pub next_sibling_page_id: Option<PageId>,
    pub key_value_pairs: Vec<KeyValuePair>,
    pub child_page_ids: Vec<PageId>,
}

/// In-memory B-Tree index prototype backed by [`BTreeMap`].
///
/// This type is intentionally not a durable/page-backed B-Tree engine.
pub struct InMemoryBTreeIndexEngine {
    index_id: IndexId,
    root_page_id: PageId,
    config: BTreeConfig,
    entries: BTreeMap<Vec<u8>, RowId>,
}

impl InMemoryBTreeIndexEngine {
    /// Create a new B-Tree index with a given root page.
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

    /// Insert a key-value pair into the index.
    pub fn insert(&mut self, key: &[u8], row_id: RowId) -> AndromedaResult<()> {
        self.validate_key("key", key)?;

        if self.entries.contains_key(key) {
            return Err(BTreeError::DuplicateKey { key: key.to_vec() }.into());
        }

        self.entries.insert(key.to_vec(), row_id);
        Ok(())
    }

    /// Search for a key in the index.
    pub fn search(&self, key: &[u8]) -> AndromedaResult<Option<RowId>> {
        self.validate_key("key", key)?;
        Ok(self.entries.get(key).copied())
    }

    /// Range scan over [start_key, end_key).
    pub fn range_scan(&self, start_key: &[u8], end_key: &[u8]) -> AndromedaResult<Vec<RowId>> {
        self.validate_key("start key", start_key)?;
        self.validate_key("end key", end_key)?;
        if start_key >= end_key {
            return Ok(Vec::new());
        }

        Ok(self
            .entries
            .range(start_key.to_vec()..end_key.to_vec())
            .map(|(_, row_id)| *row_id)
            .collect())
    }

    /// Delete a key from the index.
    pub fn delete(&mut self, key: &[u8]) -> AndromedaResult<()> {
        self.validate_key("key", key)?;
        self.entries.remove(key);
        Ok(())
    }

    /// Get current row count.
    pub fn row_count(&self) -> u64 {
        self.entries.len() as u64
    }

    /// Get index statistics.
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

    fn validate_key(&self, label: &'static str, key: &[u8]) -> AndromedaResult<()> {
        if key.len() > self.config.max_key_size as usize {
            return Err(BTreeError::SerializationError(format!(
                "{} size {} exceeds max {}",
                label,
                key.len(),
                self.config.max_key_size
            ))
            .into());
        }
        Ok(())
    }
}

#[deprecated(
    since = "0.1.0",
    note = "Use InMemoryBTreeIndexEngine. BTreeIndexEngine is an in-memory BTreeMap-backed prototype and not a durable B-Tree format."
)]
pub type BTreeIndexEngine = InMemoryBTreeIndexEngine;
