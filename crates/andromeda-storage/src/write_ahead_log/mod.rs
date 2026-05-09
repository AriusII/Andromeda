//! Write-ahead log domain re-export surface: records, transaction tracking, in-memory WAL,
//! file-backed WAL, codec, segment value types, garbage collection, and compaction.
//!
//! Canonical ownership lives in single-source modules:
//!
//! | Type / item                                  | Canonical module                            |
//! |----------------------------------------------|---------------------------------------------|
//! | `WalRecord`, `WalRecordHeader`, `WalRecordKind`, checksum/tag helpers | `andromeda_wal::write_ahead_log::record` |
//! | `InMemoryWal` and durable-LSN tracking       | `andromeda_wal::write_ahead_log::manager`   |
//! | Transaction classification helpers           | `andromeda_wal::write_ahead_log::transaction` |
//! | `WalSegment`, `WalSegmentDescriptor`         | `andromeda_wal::wal_segment`                |
//! | WAL frame typed wrappers                     | `andromeda_wal::wal_codec` over `andromeda_wal_codec` |
//! | `FileWal`, `FileWalHeader`, file scan types  | `andromeda_wal::file_wal`                   |
//! | Storage startup recovery report              | `crate::file_wal`                           |
//! | WAL GC: candidates, archive verification    | `andromeda_wal::write_ahead_log::gc`        |
//! | WAL Compaction: fragmentation, scheduling   | `andromeda_wal::write_ahead_log::compaction` |
//! | WAL shipping contract types                 | `andromeda_hadr::shipping_contract`        |
//! | Heap row redo payload envelope              | `andromeda_storage_heap`                   |
//! | Segment reclaimability policy               | `andromeda_wal::write_ahead_log::segment_reclaimability` |
//! | CommitLogEntry and CommitLog persistence    | `andromeda_wal::write_ahead_log::commit_log_entry` |
//!
//! The pure WAL items below are compatibility aliases for owner crates. New
//! callers should import pure WAL types from `andromeda_wal`, HADR shipping
//! contracts from `andromeda_hadr`, and heap redo payloads from
//! `andromeda_storage_heap`.
//!
//! Doctrine reminders enforced by the items re-exported here:
//! * `visible commit == durable WAL` — frames are flushed before commit
//!   acknowledgement.
//! * `RAM is never truth` — recovery rebuilds state from the on-disk WAL alone.
//! * No unsafe code, no ad-hoc SQL, no runtime JSON normative protocol, no
//!   gRPC/tonic transport.

pub mod durability_fence;
pub mod file {
    //! Re-export surface for file-backed WAL ownership and storage recovery.
    //!
    //! File WAL storage primitives are owned by `andromeda_wal`; storage keeps
    //! only the manifest-aware startup recovery and forensic report projection.
    pub use crate::{
        FileWalRecoveryBoundaryKind, FileWalRecoveryIgnoredTransaction,
        FileWalRecoveryIgnoredTransactionReason, FileWalRecoveryReplayRecord,
        FileWalRecoveryReportV0, FileWalStartupRecoveryV0, plan_file_wal_startup_recovery_v0,
        recover_from_file_wal, report_file_wal_recovery_v0,
    };
    pub use andromeda_wal::{
        FILE_WAL_HEADER_LEN, FILE_WAL_MAGIC, FILE_WAL_MONO_SEGMENT_ID, FileWal, FileWalDiskScan,
        FileWalHeader, scan_file_wal,
    };
}

pub mod record {
    //! Narrow compatibility alias for storage recovery tests that still import
    //! WAL records through storage while recovery/file_wal ownership is split.

    pub use andromeda_wal::{
        WalRecord, WalRecordHeader, WalRecordKind, wal_record_checksum, wal_record_kind_from_tag,
        wal_record_kind_tag,
    };
}

pub use andromeda_hadr::shipping_contract::{
    WalNodeIdentity, WalNodeRole, WalReplicaExpectation, WalReplicaSafeLsnTracker,
    WalShipmentAccepted, WalShipmentBatch, WalShipmentRange, WalShipmentRejection, WalShippingAck,
};
pub use andromeda_storage_heap::{
    HEAP_ROW_REDO_HEADER_LEN, HEAP_ROW_REDO_NONE_SLOT_ID, HEAP_ROW_REDO_PAYLOAD_MAGIC,
    HEAP_ROW_REDO_PAYLOAD_VERSION, HeapRowRedoOperation, HeapRowRedoPayloadError,
    HeapRowRedoPayloadV1,
};
pub use andromeda_wal::{
    ArchiveStatus, CommitLog, CommitLogEntry, CommitLogFacade, CompactionContext, CompactionResult,
    DefaultReclaimabilityPolicy, DurableTransactionClassifications, DurableTransactionResume,
    DurableTransactionState, EligibilityResult, FragmentationMetrics, GcEligibilityChecker,
    InMemoryWal, IncompleteDurableTransaction, MemoryWal, ReclaimabilityDecision,
    ReclaimabilityEvidence, RetentionBoundaryPolicy, Timestamp, WAL_BATCH_ROW_LIMIT,
    WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION, WAL_FORMAT_VERSION_V1, WAL_RECORD_HEADER_LEN,
    WAL_RECORD_HEADER_OVERHEAD, WAL_RECORD_MAGIC, WAL_RECORD_SIZE_LIMIT, WAL_SEGMENT_BOUNDARY,
    WalCompactionAuditEvent, WalCompactionScheduler, WalCompactionSchedulerConfig,
    WalCompactionSummary, WalFrameHeader, WalGarbageCollector, WalGcAuditEvent, WalGcCandidate,
    WalGcContext, WalGcScheduler, WalGcSchedulerConfig, WalGcSummary, WalRecord, WalRecordHeader,
    WalRecordKind, WalReplicaSafeLsnBoundaryProvider, WalScanResult, WalScanStop,
    WalScanStopReason, WalSegment, WalSegmentDescriptor, WalSegmentReclaimability,
    classify_durable_transactions, compact_segment, decode_frame_header, decode_wal_record_frame,
    encode_wal_record, encoded_wal_record_len, identify_compaction_candidates,
    incomplete_transactions_from_records, scan_wal_records, scan_wal_records_from,
    summarize_transaction, summarize_transactions_from_records, validate_lsn_continuity,
    validate_record_size, validate_segment_boundary, validate_transaction_batch_cardinality,
    validate_wal_batch_bounds, validate_wal_record_bounds, wal_record_checksum,
    wal_record_kind_from_tag, wal_record_kind_tag,
};
pub use durability_fence::*;
pub use file::*;
