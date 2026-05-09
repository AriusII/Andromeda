//! Compatibility facade for the storage-index owner crate.
//!
//! In-memory B-Tree scaffolding, format contracts, key/value metadata, and
//! concurrency policy contracts now live in `andromeda-storage-index`. Durable
//! page-backed B-Tree mutation remains fail-stop through that owner crate until
//! WAL payloads and recovery promotion land together.

#[allow(deprecated)]
pub use andromeda_storage_index::{
    BTREE_DURABLE_FORMAT_PROMOTED, BTREE_NODE_V1_FORMAT_VERSION, BTREE_NODE_V1_HEADER_LEN,
    BTREE_NODE_V1_MAGIC, BTreeConcurrencyPolicy, BTreeConfig, BTreeError, BTreeIndex,
    BTreeIndexEngine, BTreeIndexMetadata, BTreeIndexNode, BTreeLatchLevel, BTreeLatchMode,
    BTreeLatchTarget, BTreeMvccInteraction, BTreeNodeHeaderV1, BTreeNodeImpl, BTreeNodeKindV1,
    BTreeNodeV1, BTreeOperationKind, BTreePanicPoisonBehavior, BTreeRangeCursor,
    BTreeRestartReason, BTreeScanConsistency, BTreeStatistics, ColumnId, InMemoryBTreeIndexEngine,
    IndexId, KeyValuePair, RowId,
};
