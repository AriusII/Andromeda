use std::collections::BTreeMap;
use std::fmt;

use andromeda_core::AndromedaResult;

use crate::{BTreeConfig, BTreeError, IndexId, PageId, RowId};

#[derive(Debug, Clone)]
pub struct KeyValuePair {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BTreeFormatIdentityError {
    ReservedVersion,
    ZeroMaxKeySize,
}

impl BTreeFormatIdentityError {
    pub const fn message(self) -> &'static str {
        match self {
            Self::ReservedVersion => "B-Tree key format version 0.0 is reserved",
            Self::ZeroMaxKeySize => "B-Tree key format max_key_size must not be zero",
        }
    }
}

impl fmt::Display for BTreeFormatIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for BTreeFormatIdentityError {}

pub fn validate_btree_key_format_identity_parts(
    major: u32,
    minor: u32,
    max_key_size: u16,
) -> Result<(), BTreeFormatIdentityError> {
    if major == 0 && minor == 0 {
        return Err(BTreeFormatIdentityError::ReservedVersion);
    }
    if max_key_size == 0 {
        return Err(BTreeFormatIdentityError::ZeroMaxKeySize);
    }
    Ok(())
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BTreeKeyFormatIdentity {
    pub major: u32,
    pub minor: u32,
    pub codec_version: u8,
    pub max_key_size: u16,
}

impl BTreeKeyFormatIdentity {
    pub const fn new(major: u32, minor: u32, codec_version: u8, max_key_size: u16) -> Self {
        Self {
            major,
            minor,
            codec_version,
            max_key_size,
        }
    }

    pub fn try_new(
        major: u32,
        minor: u32,
        codec_version: u8,
        max_key_size: u16,
    ) -> Result<Self, BTreeFormatIdentityError> {
        validate_btree_key_format_identity_parts(major, minor, max_key_size)?;
        Ok(Self {
            major,
            minor,
            codec_version,
            max_key_size,
        })
    }

    pub const V1_0: Self = Self {
        major: 1,
        minor: 0,
        codec_version: 1,
        max_key_size: 4096,
    };

    pub const fn is_backward_compatible_with(&self, other: BTreeKeyFormatIdentity) -> bool {
        self.major == other.major && self.minor >= other.minor
    }

    pub fn format_name(&self) -> String {
        format!("KeyV{}_Codec{}", self.major, self.codec_version)
    }
}

impl fmt::Display for BTreeKeyFormatIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BTreeKeyFormat(v{}.{}, codec={}, max_key={})",
            self.major, self.minor, self.codec_version, self.max_key_size
        )
    }
}

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
    pub const fn is_read_only(&self) -> bool {
        matches!(self, Self::Lookup | Self::RangeScan)
    }

    pub const fn name(&self) -> &'static str {
        match self {
            Self::Lookup => "lookup",
            Self::RangeScan => "range_scan",
            Self::Insert => "insert",
            Self::Delete => "delete",
            Self::Split => "split",
            Self::Merge => "merge",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BTreeFormatIdentityError, BTreeKeyFormatIdentity, BTreeOperationType,
        validate_btree_key_format_identity_parts,
    };

    #[test]
    fn key_format_identity_rejects_reserved_zero_version() {
        assert_eq!(
            BTreeKeyFormatIdentity::try_new(0, 0, 1, 4096),
            Err(BTreeFormatIdentityError::ReservedVersion)
        );
        assert_eq!(
            validate_btree_key_format_identity_parts(0, 0, 4096),
            Err(BTreeFormatIdentityError::ReservedVersion)
        );
    }

    #[test]
    fn key_format_identity_rejects_zero_key_limit() {
        assert_eq!(
            BTreeKeyFormatIdentity::try_new(1, 0, 1, 0),
            Err(BTreeFormatIdentityError::ZeroMaxKeySize)
        );
        assert_eq!(
            validate_btree_key_format_identity_parts(1, 0, 0),
            Err(BTreeFormatIdentityError::ZeroMaxKeySize)
        );
    }

    #[test]
    fn key_format_identity_preserves_compatibility_rules() {
        let v1_0 = BTreeKeyFormatIdentity::V1_0;
        let v1_1 = BTreeKeyFormatIdentity::new(1, 1, 1, 4096);
        let v2_0 = BTreeKeyFormatIdentity::new(2, 0, 1, 4096);

        assert!(v1_1.is_backward_compatible_with(v1_0));
        assert!(!v1_0.is_backward_compatible_with(v1_1));
        assert!(!v2_0.is_backward_compatible_with(v1_0));
        assert_eq!(v1_0.format_name(), "KeyV1_Codec1");
    }

    #[test]
    fn operation_type_classifies_read_only_operations() {
        assert!(BTreeOperationType::Lookup.is_read_only());
        assert!(BTreeOperationType::RangeScan.is_read_only());
        assert!(!BTreeOperationType::Insert.is_read_only());
        assert_eq!(BTreeOperationType::Merge.name(), "merge");
    }
}
