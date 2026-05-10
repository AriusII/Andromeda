#![allow(dead_code, deprecated, unused_imports)]

//! Compile-only guard for the C5 storage owner-crate surface.
//!
//! Storage-family APIs should compile when imported directly from their owner crates. The
//! `andromeda-storage` crate remains a composition/compatibility surface, but this guard avoids
//! relying on facade re-exports for storage-family contracts.

use andromeda_buffer_pool::{BufferPoolConfig, BufferPoolError};
use andromeda_catalog_recovery::{
    CatalogStorageWalRecord, CatalogStorageWalRecordVersion, decode_storage_catalog_record,
    encode_storage_catalog_record,
};
use andromeda_disk_page_store::{
    DiskManager, DiskManagerError, DiskPageStore, FileDiskManager, PageIntegrityMode,
};
use andromeda_manifest::{
    DatabaseManifest, DatabaseSnapshotPublication, FormatVersion, PublishedColdSegment,
    StorageFormatKind, validate_manifest_atomic_switch, validate_recovery_floor,
};
use andromeda_recovery::{
    FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransactionReason, FileWalRecoveryReportV0,
    RecoveryPlan, ReplayContext, StartupMode, recover_from_file_wal, replay_wal_record,
    report_file_wal_recovery_v0 as module_report_file_wal_recovery_v0,
};
use andromeda_segment::{SegmentDescriptor, SegmentId};
use andromeda_storage_heap::{
    ColumnDef, Datum, HeapPage, HeapPageInsert, HeapRowRedoPayloadV1, HeapScanIter, HeapVacuumMode,
    ProductStockHeapInsert, ProductStockRow, RowEncoder, RowSchema, ScalarType,
    product_stock_row_encoder, product_stock_row_schema,
};
use andromeda_storage_index::{
    BTREE_DURABLE_FORMAT_PROMOTED, BTREE_NODE_V1_FORMAT_VERSION, BTREE_NODE_V1_HEADER_LEN,
    BTREE_NODE_V1_MAGIC, BTreeConfig, BTreeIndexEngine, BTreeKeyFormatIdentity, BTreeNodeHeaderV1,
    BTreeNodeImpl, BTreeNodeKindV1, BTreeNodeV1, BTreeOperationType, ColumnId,
    InMemoryBTreeIndexEngine, IndexId, Key, KeyCodec, KeyComparator, KeyV1FormatValidator,
    KeyValuePair, RowId,
};
use andromeda_storage_page::{
    AllocationId, InMemoryPageStore, ObjectId, PageHeader, PageId, PageImage, PageSize, PageStore,
    PageTrailer, PageType, validate_wal_durability_before_page_flush,
};
use andromeda_wal::{Lsn, WalRecord, WalRecordKind, validate_wal_record_bounds};

type RootManifest = DatabaseManifest;
type SnapshotPublication = DatabaseSnapshotPublication;

#[test]
fn storage_family_owner_crate_imports_compile() {
    fn accepts_root_manifest(_: Option<RootManifest>) {}
    fn accepts_snapshot_publication(_: Option<SnapshotPublication>) {}

    let _ = accepts_root_manifest as fn(Option<RootManifest>);
    let _ = accepts_snapshot_publication as fn(Option<SnapshotPublication>);
    let _ = HeapRowRedoPayloadV1::decode;
    let _ = validate_manifest_atomic_switch;
    let _ = validate_recovery_floor;
    let _ = validate_wal_durability_before_page_flush;
    let _ = validate_wal_record_bounds;
    let _ = encode_storage_catalog_record;
    let _ = decode_storage_catalog_record;
    let _ = WalRecordKind::TxBegin;
}
