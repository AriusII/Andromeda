#![allow(dead_code, deprecated, unused_imports)]

//! Compile-only guard for the C5 storage compatibility facade.
//!
//! Lot 4 splits must keep these root and nested import paths available until
//! downstream crates migrate deliberately. This test must not execute WAL,
//! recovery, codec, HADR, backup, or restore behavior.

use std::any::TypeId;

use andromeda_storage::btree_format_validation::{
    BTreeKeyFormatIdentity, BTreeOperationType, KeyV1FormatValidator,
};
use andromeda_storage::btree_key_codec::{Key as ModuleKey, KeyCodec as ModuleKeyCodec};
use andromeda_storage::disk_manager::PageIntegrityMode;
use andromeda_storage::format_version::{FormatVersion, StorageFormatKind};
use andromeda_storage::hadr::{quorum_runtime, shipping_runtime};
use andromeda_storage::layout::{
    cold as layout_cold, extent as layout_extent, io_budget as layout_io_budget,
    page as layout_page, placement as layout_placement, segment as layout_segment,
};
use andromeda_storage::publication::DatabaseManifest as PublicationDatabaseManifest;
use andromeda_storage::write_ahead_log::codec::{
    WalFrameHeader as ModuleWalFrameHeader, WalScanResult as ModuleWalScanResult,
    WalScanStop as ModuleWalScanStop, WalScanStopReason as ModuleWalScanStopReason,
    decode_frame_header, decode_wal_record_frame as module_decode_wal_record_frame,
    encode_wal_record as module_encode_wal_record,
};
use andromeda_storage::write_ahead_log::file::{
    FileWal as ModuleFileWal, FileWalDiskScan as ModuleFileWalDiskScan,
    FileWalHeader as ModuleFileWalHeader, FileWalRecoveryBoundaryKind,
    FileWalRecoveryIgnoredTransactionReason, recover_from_file_wal,
    report_file_wal_recovery_v0 as module_report_file_wal_recovery_v0,
};
use andromeda_storage::write_ahead_log::record::{
    WalRecord as ModuleWalRecord, WalRecordHeader as ModuleWalRecordHeader,
    WalRecordKind as ModuleWalRecordKind,
};
use andromeda_storage::write_ahead_log::segment::{
    WalSegment as ModuleWalSegment, WalSegmentDescriptor as ModuleWalSegmentDescriptor,
};
use andromeda_storage::write_ahead_log::{
    CommitLog, CommitLogEntry, CommitLogFacade, DurableTransactionResume, HeapRowRedoPayloadV1,
    InMemoryWal as ModuleInMemoryWal, IncompleteDurableTransaction, MemoryWal as ModuleMemoryWal,
    WAL_BATCH_ROW_LIMIT, WAL_RECORD_HEADER_OVERHEAD, WAL_RECORD_SIZE_LIMIT, WAL_SEGMENT_BOUNDARY,
};
use andromeda_storage::{
    AllocationId, BTREE_DURABLE_FORMAT_PROMOTED, BTREE_NODE_V1_FORMAT_VERSION,
    BTREE_NODE_V1_HEADER_LEN, BTREE_NODE_V1_MAGIC, BTreeConfig, BTreeIndexEngine,
    BTreeNodeHeaderV1, BTreeNodeImpl, BTreeNodeKindV1, BTreeNodeV1, BackupExecutionPlan, BackupId,
    BackupManifest, BackupResourceLimits, BufferPoolConfig, BufferPoolError, ColdSnapshotBoundary,
    ColumnDef, ColumnId, DatabaseManifest, Datum, DiskManager, DiskManagerError, DiskPageStore,
    DurableTransactionState, FileBackedBackupArtifactStore, FileBackedHadrMembershipStore,
    FileDiskManager, FileWal, FileWalDiskScan, FileWalHeader, FileWalRecoveryReportV0,
    HadrMembershipRecord, HadrMembershipSnapshot, HadrMembershipStore, HadrNodeId, HadrNodeRole,
    HeapPage, HeapPageInsert, HeapScanIter, HeapVacuumMode, InMemoryBTreeIndexEngine,
    InMemoryPageStore, InMemoryWal, IndexId, Key, KeyCodec, KeyComparator, KeyValuePair, Lsn,
    MemoryWal, ObjectId, PageHeader, PageId, PageImage, PageSize, PageStore, PageTrailer, PageType,
    ProductStockHeapInsert, ProductStockRow, RecoveryPlan, RecoveryStage, ReplayContext,
    RestoreValidationPolicy, RowEncoder, RowId, RowSchema, ScalarType, SegmentDescriptor,
    SegmentId, StartupMode, WAL_FORMAT_VERSION, WalFrameHeader, WalRecord, WalRecordHeader,
    WalRecordKind, WalScanResult, WalScanStop, WalScanStopReason, WalSegment, WalSegmentDescriptor,
    compute_restore_checksum, decode_wal_record_frame, encode_catalog_record, encode_wal_record,
    plan_replay_segments, product_stock_row_encoder, product_stock_row_schema, replay_wal_record,
    report_file_wal_recovery_v0, validate_manifest_atomic_switch, validate_recovery_floor,
    validate_wal_durability_before_page_flush,
};
use andromeda_wal as wal;

type RootLsn = Lsn;
type RootWalRecord = WalRecord;
type NestedWalRecord = ModuleWalRecord;
type RootWalKind = WalRecordKind;
type NestedWalKind = ModuleWalRecordKind;
type RootManifest = DatabaseManifest;
type NestedManifest = PublicationDatabaseManifest;
type RootFileWal = FileWal;
type NestedFileWal = ModuleFileWal;

fn assert_same_type<T: 'static, U: 'static>() {
    assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
}

#[test]
fn storage_root_and_nested_facade_imports_compile() {
    fn accepts_root_record(_: Option<RootWalRecord>) {}
    fn accepts_nested_record(_: Option<NestedWalRecord>) {}
    fn accepts_root_manifest(_: Option<RootManifest>) {}
    fn accepts_nested_manifest(_: Option<NestedManifest>) {}

    let _ = accepts_root_record as fn(Option<RootWalRecord>);
    let _ = accepts_nested_record as fn(Option<NestedWalRecord>);
    let _ = accepts_root_manifest as fn(Option<RootManifest>);
    let _ = accepts_nested_manifest as fn(Option<NestedManifest>);
    let _ = WAL_FORMAT_VERSION;
    let _ = WAL_BATCH_ROW_LIMIT;
}

#[test]
fn pure_wal_storage_reexports_match_andromeda_wal_types() {
    assert_same_type::<Lsn, wal::Lsn>();

    assert_same_type::<WalRecordKind, wal::WalRecordKind>();
    assert_same_type::<WalRecordHeader, wal::WalRecordHeader>();
    assert_same_type::<WalRecord, wal::WalRecord>();
    assert_same_type::<ModuleWalRecordKind, wal::write_ahead_log::record::WalRecordKind>();
    assert_same_type::<ModuleWalRecordHeader, wal::write_ahead_log::record::WalRecordHeader>();
    assert_same_type::<ModuleWalRecord, wal::write_ahead_log::record::WalRecord>();

    assert_same_type::<WalSegmentDescriptor, wal::WalSegmentDescriptor>();
    assert_same_type::<WalSegment, wal::WalSegment>();
    assert_same_type::<
        ModuleWalSegmentDescriptor,
        wal::write_ahead_log::segment::WalSegmentDescriptor,
    >();
    assert_same_type::<ModuleWalSegment, wal::write_ahead_log::segment::WalSegment>();

    assert_same_type::<InMemoryWal, wal::InMemoryWal>();
    assert_same_type::<MemoryWal, wal::MemoryWal>();
    assert_same_type::<ModuleInMemoryWal, wal::write_ahead_log::manager::InMemoryWal>();
    assert_same_type::<ModuleMemoryWal, wal::write_ahead_log::manager::MemoryWal>();

    assert_same_type::<WalFrameHeader, wal::WalFrameHeader>();
    assert_same_type::<WalScanResult, wal::WalScanResult>();
    assert_same_type::<WalScanStop, wal::WalScanStop>();
    assert_same_type::<WalScanStopReason, wal::WalScanStopReason>();
    assert_same_type::<ModuleWalFrameHeader, wal::write_ahead_log::codec::WalFrameHeader>();
    assert_same_type::<ModuleWalScanResult, wal::write_ahead_log::codec::WalScanResult>();
    assert_same_type::<ModuleWalScanStop, wal::write_ahead_log::codec::WalScanStop>();
    assert_same_type::<ModuleWalScanStopReason, wal::write_ahead_log::codec::WalScanStopReason>();

    assert_same_type::<FileWal, wal::FileWal>();
    assert_same_type::<ModuleFileWal, wal::write_ahead_log::file::FileWal>();
    assert_same_type::<FileWalHeader, wal::FileWalHeader>();
    assert_same_type::<ModuleFileWalHeader, wal::write_ahead_log::file::FileWalHeader>();
    assert_same_type::<FileWalDiskScan, wal::FileWalDiskScan>();
    assert_same_type::<ModuleFileWalDiskScan, wal::write_ahead_log::file::FileWalDiskScan>();
}
