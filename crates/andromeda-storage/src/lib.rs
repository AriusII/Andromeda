#![forbid(unsafe_code)]

mod btree_format_validation;
mod btree_key_codec;
mod cold_store;
mod file_wal;
pub mod format_version;
mod heap;
mod lsn;
mod manifest;
mod recovery;
mod wal_record_catalog;

pub mod layout;
pub mod publication;
pub mod write_ahead_log;

pub use andromeda_buffer_pool::{
    BufferFrame, BufferFrameId, BufferFrameState, BufferPool, BufferPoolConfig, BufferPoolError,
    BufferPoolManager, ClockEvictionCandidate, ClockEvictionPolicy, DirtyEntry,
    DirtyFlushCandidate, DirtyTracker, FlushAllDirtyResult, FlushBlockedFrame, FlushError,
    PageGuard, PageGuardMut, TestWalDurabilityObserver, WalDurabilityObserver,
};
pub use andromeda_catalog_recovery::{
    CatalogStorageWalDurablePublication as CatalogWalDurablePublication,
    CatalogStorageWalPublicationRecord as CatalogWalPublicationRecord,
    CatalogStorageWalPublicationReplayReport as CatalogWalPublicationReplayReport,
    decode_storage_catalog_record as decode_catalog_record,
    encode_storage_catalog_record as encode_catalog_record,
    replay_storage_catalog_publications_from_wal as replay_catalog_publications_from_wal,
};
pub use andromeda_disk_page_store::{
    DiskManager, DiskManagerError, DiskPageStore, FileDiskManager, PageIntegrityMode,
};
pub use andromeda_segment::segment_index::*;
pub use andromeda_segment::{
    ColdExtentReclaimEvidence, ExtentDescriptor, ExtentFreeRange, ExtentId, ExtentManager,
    ExtentManagerReplayRecord, ExtentState, SegmentDescriptor, SegmentDurabilityBoundary,
    SegmentHeader, SegmentId, SegmentMutation, SegmentState, SegmentTrailer,
    validate_segment_extent_contiguity,
};
pub use andromeda_storage_heap::{
    ColumnDef, Datum, INVENTORY_PRODUCT_STOCK_TABLE_NAME, PRODUCT_STOCK_PRODUCT_ID_COLUMN,
    PRODUCT_STOCK_QUANTITY_ON_HAND_COLUMN, PRODUCT_STOCK_ROW_ENCODED_LEN, ProductStockRow,
    RowEncoder, RowSchema, ScalarType, product_stock_row_encoder, product_stock_row_schema,
};
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
pub use andromeda_storage_page::{
    AllocationId, DecodedPageV1, InMemoryPageStore, ObjectId, PAGE_CODEC_V1_HEADER_LEN,
    PAGE_CODEC_V1_TRAILER_LEN, PageCodecV1, PageFlags, PageHeader, PageId, PageImage,
    PageLayoutContract, PageSize, PageStore, PageTrailer, PageType, integrity_trailer_for_payload,
    payload_crc64, payload_hash, torn_write_guard, validate_payload_integrity,
};
pub use andromeda_wal::{
    DurableTransactionClassifications, DurableTransactionResume, DurableTransactionState,
    InMemoryWal, IncompleteDurableTransaction, MemoryWal, WAL_BYTE_ORDER_LITTLE_ENDIAN,
    WAL_FORMAT_VERSION, WAL_FORMAT_VERSION_V1, WAL_RECORD_HEADER_LEN, WAL_RECORD_MAGIC,
    WalFrameHeader, WalRecord, WalRecordHeader, WalRecordKind, WalScanResult, WalScanStop,
    WalScanStopReason, WalSegment, WalSegmentDescriptor, classify_durable_transactions,
    decode_frame_header, decode_wal_record_frame, encode_wal_record,
    incomplete_transactions_from_records, scan_wal_records, scan_wal_records_from,
    summarize_transaction, summarize_transactions_from_records, wal_record_checksum,
    wal_record_kind_from_tag, wal_record_kind_tag,
};
pub use btree_format_validation::{
    BTreeFormatIdentityError, BTreeKeyFormatIdentity, BTreeOperationType, KeyV1FormatValidator,
};
pub use btree_key_codec::{Key, KeyCodec, KeyComparator};
pub use cold_store::*;
pub use file_wal::*;
pub use heap::{
    HEAP_PAGE_V1_PAYLOAD_OFFSET, HeapPage, HeapPageInsert, HeapScanIter, HeapVacuumMode,
    HeapVacuumPlan, HeapVacuumReport, ProductStockHeapInsert, ProductStockHeapScanIter, SlotEntry,
    slot_directory,
};
pub use lsn::*;
pub use manifest::*;
pub use recovery::{
    CatalogReplayFromLsnReport, CatalogSnapshot, ConceptualRedoPlan, FastStartAcceptance,
    FastStartRejection, ForensicAnomaly, ForensicAnomalyKind, ForensicAnomalyReport,
    ForensicStartAcceptance, HeapRedoPageState, HeapRedoSlotState, IndexRebuildRequiredEvidence,
    LsnBoundCatalogRecord, ObservedBoundary, PreRedoStorageFormatDecision,
    PreRedoStorageFormatGate, PreRedoStorageFormatRejection, RECOVERY_REQUIRED_STORAGE_FORMATS,
    RecoveryPlan, RecoveryTrace, RedoRecordDecision, RedoRecordPlan, ReplayContext, ReplayOutcome,
    ReplayResult, SafeStartAcceptance, SafeStartInvariantReport, SafeStartTailDiscard,
    StartupAcceptance, StartupAuditProjection, StartupDecision, StartupEvidence, StartupMode,
    StartupOutcome, StartupRejectionReason, StorageFormatFingerprint, UndoChain, UndoChainsBuilder,
    UndoOperation, UndoRecord, WalCoverageEvidence, WalReplayReport, decide_startup,
    execute_redo_plan, execute_redo_plan_into_context, fast_start_from_manifest_and_scan,
    forensic_start_from_manifest_and_scan, replay_catalog_from_lsn, replay_catalog_wal_records,
    replay_wal_from_lsn, replay_wal_from_lsn_into_context, replay_wal_record,
    safe_start_from_manifest_and_scan, verify_safe_start_invariants,
};
pub use wal_record_catalog::*;
pub use write_ahead_log::{
    DurabilityFenceError, WAL_BATCH_ROW_LIMIT, WAL_RECORD_HEADER_OVERHEAD, WAL_RECORD_SIZE_LIMIT,
    WAL_SEGMENT_BOUNDARY, validate_lsn_continuity, validate_lsn_ordered,
    validate_lsn_strictly_ordered, validate_manifest_atomic_switch, validate_record_size,
    validate_recovery_floor, validate_segment_boundary, validate_transaction_batch_cardinality,
    validate_wal_batch_bounds, validate_wal_durability_before_page_flush,
    validate_wal_record_bounds,
};
