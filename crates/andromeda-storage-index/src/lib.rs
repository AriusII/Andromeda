#![forbid(unsafe_code)]
#![doc = r#"
Storage index ownership boundary for Andromeda.

This crate starts the extraction of storage-index-owned identity and
configuration contracts from `andromeda-storage`. Runtime B-Tree mutation and
page-backed node persistence remain in `andromeda-storage` until their WAL,
recovery, and crash-validation contracts are promoted together.
"#]

mod concurrency;
mod config;
mod contract;
mod cursor;
mod engine;
mod error;
mod format_validation;
mod identity;
mod key_codec;
mod key_comparator;
mod leaf;
mod node;
mod node_format_v1;

pub use andromeda_storage_page::PageId;
pub use concurrency::{
    BTreeConcurrencyPolicy, BTreeLatchLevel, BTreeLatchMode, BTreeLatchTarget,
    BTreeMvccInteraction, BTreeOperationKind, BTreePanicPoisonBehavior, BTreeRestartReason,
    BTreeScanConsistency,
};
pub use config::{BTREE_DURABLE_FORMAT_PROMOTED, BTreeConfig};
pub use contract::{BTreeIndex, BTreeIndexMetadata, BTreeIndexNode, BTreeRangeCursor};
#[allow(deprecated)]
pub use engine::{
    BTreeFormatIdentityError, BTreeIndexEngine, BTreeKeyFormatIdentity, BTreeNodeImpl,
    BTreeOperationType, BTreeStatistics, InMemoryBTreeIndexEngine, KeyValuePair,
    validate_btree_key_format_identity_parts,
};
pub use error::BTreeError;
pub use format_validation::{KeyV1FormatGate, KeyV1FormatValidator};
pub use identity::{ColumnId, IndexId, RowId};
pub use key_codec::{Key, KeyCodec, KeyDatum, KeyScalarType};
pub use key_comparator::KeyComparator;
pub use node_format_v1::{
    BTREE_NODE_V1_FORMAT_VERSION, BTREE_NODE_V1_HEADER_LEN, BTREE_NODE_V1_MAGIC, BTreeNodeHeaderV1,
    BTreeNodeKindV1, BTreeNodeV1,
};

pub(crate) use concurrency::non_root_min_keys;
pub(crate) use error::deferred_btree_result;

#[cfg(test)]
mod tests;
