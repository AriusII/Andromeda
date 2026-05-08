#![forbid(unsafe_code)]

pub mod backup;
mod btree;
pub mod btree_format_validation;
pub mod btree_key_codec;
pub mod buffer_pool;
mod catalog_wal_bridge;
mod cold_store;
pub mod disk_manager;
mod extent;
mod file_wal;
pub mod format_version;
pub mod hadr;
mod heap;
mod heap_row_encoder;
mod lsn;
mod manifest;
mod operational_profile;
mod page;
mod page_codec_v1;
mod placement;
mod recovery;
mod restore_orchestration;
mod segment;
mod segment_index;
mod wal;
mod wal_codec;
mod wal_record_catalog;
mod wal_segment;

pub mod layout;
pub mod publication;
pub mod write_ahead_log;

pub use backup::*;
#[allow(deprecated)]
pub use btree::{
    BTREE_DURABLE_FORMAT_PROMOTED, BTREE_NODE_V1_FORMAT_VERSION, BTREE_NODE_V1_HEADER_LEN,
    BTREE_NODE_V1_MAGIC, BTreeConcurrencyPolicy, BTreeConfig, BTreeError, BTreeIndex,
    BTreeIndexEngine, BTreeIndexMetadata, BTreeIndexNode, BTreeLatchLevel, BTreeLatchMode,
    BTreeLatchTarget, BTreeMvccInteraction, BTreeNodeHeaderV1, BTreeNodeImpl, BTreeNodeKindV1,
    BTreeNodeV1, BTreeOperationKind, BTreePanicPoisonBehavior, BTreeRangeCursor,
    BTreeRestartReason, BTreeScanConsistency, BTreeStatistics, ColumnId, InMemoryBTreeIndexEngine,
    IndexId, KeyValuePair, RowId,
};
pub use btree_format_validation::{
    BTreeKeyFormatIdentity, BTreeOperationType, KeyV1FormatValidator,
};
pub use btree_key_codec::{Key, KeyCodec, KeyComparator};
pub use buffer_pool::{
    BufferFrame, BufferFrameId, BufferFrameState, BufferPool, BufferPoolConfig, BufferPoolError,
    BufferPoolManager, ClockEvictionCandidate, ClockEvictionPolicy, DirtyEntry,
    DirtyFlushCandidate, DirtyTracker, FlushAllDirtyResult, FlushBlockedFrame, FlushError,
    PageGuard, PageGuardMut, TestWalDurabilityObserver, WalDurabilityObserver,
};
pub use catalog_wal_bridge::*;
pub use cold_store::*;
pub use disk_manager::{DiskManager, DiskManagerError, DiskPageStore, FileDiskManager};
pub use extent::*;
pub use file_wal::*;
pub use hadr::*;
pub use heap::{
    HEAP_PAGE_V1_PAYLOAD_OFFSET, HeapPage, HeapPageInsert, HeapScanIter, HeapVacuumMode,
    HeapVacuumPlan, HeapVacuumReport, ProductStockHeapInsert, ProductStockHeapScanIter, SlotEntry,
    slot_directory,
};
pub use heap_row_encoder::{
    ColumnDef, Datum, INVENTORY_PRODUCT_STOCK_TABLE_NAME, PRODUCT_STOCK_PRODUCT_ID_COLUMN,
    PRODUCT_STOCK_QUANTITY_ON_HAND_COLUMN, PRODUCT_STOCK_ROW_ENCODED_LEN, ProductStockRow,
    RowEncoder, RowSchema, ScalarType, product_stock_row_encoder, product_stock_row_schema,
};
pub use lsn::*;
pub use manifest::*;
pub use operational_profile::*;
pub use page::*;
pub use page_codec_v1::*;
pub use placement::*;
pub use recovery::*;
pub use restore_orchestration::*;
pub use segment::*;
pub use segment_index::*;
pub use wal::*;
pub use wal_codec::*;
pub use wal_record_catalog::*;
pub use wal_segment::*;
pub use write_ahead_log::{
    DurabilityFenceError, WAL_BATCH_ROW_LIMIT, WAL_RECORD_HEADER_OVERHEAD, WAL_RECORD_SIZE_LIMIT,
    WAL_SEGMENT_BOUNDARY, validate_lsn_continuity, validate_lsn_ordered,
    validate_lsn_strictly_ordered, validate_manifest_atomic_switch, validate_record_size,
    validate_recovery_floor, validate_segment_boundary, validate_transaction_batch_cardinality,
    validate_wal_batch_bounds, validate_wal_durability_before_page_flush,
    validate_wal_record_bounds,
};
