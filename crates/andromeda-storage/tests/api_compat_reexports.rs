#![allow(dead_code, deprecated, unused_imports)]

//! Compile-only guard for the C5 storage compatibility surface.
//!
//! The storage compatibility surface keeps storage-owned compatibility paths only. Backup,
//! restore, and HADR contracts are imported directly from their owner crates.

use andromeda_storage::format_version::{FormatVersion, StorageFormatKind};
use andromeda_storage::layout::page as layout_page;
use andromeda_storage::publication::DatabaseManifest as PublicationDatabaseManifest;
use andromeda_storage::write_ahead_log::file::{
    FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransactionReason, recover_from_file_wal,
    report_file_wal_recovery_v0 as module_report_file_wal_recovery_v0,
};
use andromeda_storage::write_ahead_log::{
    validate_manifest_atomic_switch, validate_recovery_floor,
    validate_wal_durability_before_page_flush,
};
use andromeda_storage::{
    AllocationId, BTREE_DURABLE_FORMAT_PROMOTED, BTREE_NODE_V1_FORMAT_VERSION,
    BTREE_NODE_V1_HEADER_LEN, BTREE_NODE_V1_MAGIC, BTreeConfig, BTreeIndexEngine,
    BTreeKeyFormatIdentity, BTreeNodeHeaderV1, BTreeNodeImpl, BTreeNodeKindV1, BTreeNodeV1,
    BTreeOperationType, BufferPoolConfig, BufferPoolError, ColumnDef, ColumnId, DatabaseManifest,
    Datum, DiskManager, DiskManagerError, DiskPageStore, FileDiskManager, FileWalRecoveryReportV0,
    HeapPage, HeapPageInsert, HeapScanIter, HeapVacuumMode, InMemoryBTreeIndexEngine,
    InMemoryPageStore, IndexId, Key, KeyCodec, KeyComparator, KeyV1FormatValidator, KeyValuePair,
    ObjectId, PageHeader, PageId, PageImage, PageIntegrityMode, PageSize, PageStore, PageTrailer,
    PageType, ProductStockHeapInsert, ProductStockRow, RecoveryPlan, ReplayContext, RowEncoder,
    RowId, RowSchema, ScalarType, SegmentDescriptor, SegmentId, StartupMode, encode_catalog_record,
    product_stock_row_encoder, product_stock_row_schema, replay_wal_record,
    report_file_wal_recovery_v0,
};
use andromeda_storage_heap::HeapRowRedoPayloadV1;
use andromeda_wal::{Lsn, WalRecord, WalRecordKind};

type RootManifest = DatabaseManifest;
type NestedManifest = PublicationDatabaseManifest;

#[test]
fn storage_root_and_nested_facade_imports_compile() {
    fn accepts_root_manifest(_: Option<RootManifest>) {}
    fn accepts_nested_manifest(_: Option<NestedManifest>) {}

    let _ = accepts_root_manifest as fn(Option<RootManifest>);
    let _ = accepts_nested_manifest as fn(Option<NestedManifest>);
    let _ = HeapRowRedoPayloadV1::decode;
    let _ = validate_manifest_atomic_switch;
    let _ = validate_recovery_floor;
    let _ = validate_wal_durability_before_page_flush;
    let _ = WalRecordKind::TxBegin;
}
