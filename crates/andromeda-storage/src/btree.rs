//! B-Tree index contracts and the current in-memory engine.
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
use andromeda_core::AndromedaResult;

mod concurrency;
mod config;
mod contract;
mod cursor;
mod engine;
mod error;
mod identity;
mod leaf;

mod node;
pub(crate) mod node_format_v1;

pub(crate) use concurrency::non_root_min_keys;
pub use concurrency::{
    BTreeConcurrencyPolicy, BTreeLatchLevel, BTreeLatchMode, BTreeLatchTarget,
    BTreeMvccInteraction, BTreeOperationKind, BTreePanicPoisonBehavior, BTreeRestartReason,
    BTreeScanConsistency,
};
pub use config::{BTREE_DURABLE_FORMAT_PROMOTED, BTreeConfig};
pub use contract::{
    BTreeIndex, BTreeIndexMetadata, BTreeIndexNode, BTreeRangeCursor, BTreeStatistics,
};
#[allow(deprecated)]
pub use engine::{BTreeIndexEngine, BTreeNodeImpl, InMemoryBTreeIndexEngine, KeyValuePair};
pub use error::BTreeError;
pub(crate) use error::deferred_btree_result;
pub use identity::{ColumnId, IndexId, RowId};
pub use node_format_v1::{
    BTREE_NODE_V1_FORMAT_VERSION, BTREE_NODE_V1_HEADER_LEN, BTREE_NODE_V1_MAGIC, BTreeNodeHeaderV1,
    BTreeNodeKindV1, BTreeNodeV1,
};

#[cfg(test)]
mod tests;
