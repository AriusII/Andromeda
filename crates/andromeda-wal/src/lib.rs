#![forbid(unsafe_code)]
#![doc = r#"
Native WAL type crate for Andromeda.

This crate owns pure WAL primitives, codecs, and the physical file-backed WAL
byte contract. Storage recovery reports, replay planning, manifests, page
integration, and durable visibility decisions remain storage-owned; downstream
crates should import WAL owner types from this crate while storage integration
continues to use storage crate surfaces.
"#]

pub mod file_wal;
pub mod lsn;
pub mod wal_codec;
pub mod wal_segment;
pub mod write_ahead_log;

pub use file_wal::{
    FILE_WAL_HEADER_LEN, FILE_WAL_MAGIC, FILE_WAL_MONO_SEGMENT_ID, FileWal, FileWalDiskScan,
    FileWalHeader, scan_file_wal,
};
pub use lsn::Lsn;
pub use wal_codec::{
    WAL_BYTE_ORDER_LITTLE_ENDIAN, WAL_FORMAT_VERSION, WAL_FORMAT_VERSION_V1, WAL_RECORD_HEADER_LEN,
    WAL_RECORD_MAGIC, WalFrameHeader, WalScanResult, WalScanStop, WalScanStopReason,
    decode_frame_header, decode_wal_record_frame, encode_wal_record, encoded_wal_record_len,
    scan_wal_records, scan_wal_records_from,
};
pub use wal_segment::{WalSegment, WalSegmentDescriptor};
pub use write_ahead_log::{
    ArchiveStatus, CommitLog, CommitLogEntry, CommitLogFacade, DefaultReclaimabilityPolicy,
    DurabilityFenceError, DurableTransactionClassifications, DurableTransactionResume,
    DurableTransactionState, EligibilityResult, GcEligibilityChecker, InMemoryWal,
    IncompleteDurableTransaction, MemoryWal, ReclaimabilityDecision, ReclaimabilityEvidence,
    RetentionBoundaryPolicy, Timestamp, WAL_BATCH_ROW_LIMIT, WAL_RECORD_HEADER_OVERHEAD,
    WAL_RECORD_SIZE_LIMIT, WAL_SEGMENT_BOUNDARY, WalGarbageCollector, WalGcAuditEvent,
    WalGcCandidate, WalGcContext, WalGcScheduler, WalGcSchedulerConfig, WalGcSummary, WalRecord,
    WalRecordHeader, WalRecordKind, WalReplicaSafeLsnBoundaryProvider, WalSegmentReclaimability,
    classify_durable_transactions, incomplete_transactions_from_records, summarize_transaction,
    summarize_transactions_from_records, validate_lsn_continuity, validate_lsn_ordered,
    validate_lsn_strictly_ordered, validate_manifest_atomic_switch, validate_record_size,
    validate_recovery_floor, validate_segment_boundary, validate_transaction_batch_cardinality,
    validate_wal_batch_bounds, validate_wal_durability_before_page_flush,
    validate_wal_record_bounds, wal_record_checksum, wal_record_kind_from_tag, wal_record_kind_tag,
};
