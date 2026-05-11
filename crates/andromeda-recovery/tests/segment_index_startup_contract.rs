//! Segment-index startup contract tests (P05 W4).
//!
//! Invariants under test:
//!
//! 1. Bootstrap mode (`segment_index_bytes = None`) sets `segment_index_validated = true`
//!    without decoding anything — clean-install starts must not block on absent segment index.
//!
//! 2. Corrupt segment-index bytes cause early rejection (`Err`) **before** the WAL file is
//!    opened.  This is demonstrated by using a nonexistent WAL path: if the error comes from
//!    the segment index decode the function returns a Storage error containing "segment index
//!    decode failed before WAL scan"; if it came from WAL I/O the error would mention the
//!    missing file instead.
//!
//! 3. Valid segment-index bytes are decoded successfully and `evidence.segment_index_validated`
//!    is set to `true` in the returned evidence.
//!
//! 4. A segment index carrying entries that reference nonexistent cold-segment file_ids does
//!    NOT cause any cold-file I/O during startup — the bytes are decoded in memory only.
//!    (`no_cold_scan_startup_with_segment_entries_for_nonexistent_files`)

use andromeda_manifest::DatabaseManifest;
use andromeda_recovery::{StartupMode, plan_file_wal_startup_recovery_v0_with_segment_index};
use andromeda_segment::{AllocationId, ExtentId, ObjectId, SegmentId};
use andromeda_segment::{
    PageId, PageSize,
    segment_index::{SegmentIndexBuildContextV0, SegmentIndexEntryV0, SegmentIndexV0},
};
use andromeda_wal::{FileWal, Lsn};
use std::path::PathBuf;

// ── test helpers ──────────────────────────────────────────────────────────────

fn test_wal_path(test_name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "andromeda-recovery-sic-{test_name}-{}.wal",
        std::process::id()
    ))
}

fn startup_manifest() -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: 1,
        snapshot_id: 1,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        previous_manifest_hash: [0; 32],
        manifest_crc: 0xDEAD_BEEF,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    }
}

/// Build a minimal but fully valid `SegmentIndexV0` carrying one entry that
/// references a cold-segment `file_id = 999` (which does not exist on disk).
///
/// This is the key fixture for the no-cold-IO test.
fn minimal_valid_segment_index_bytes() -> Vec<u8> {
    let ctx = SegmentIndexBuildContextV0 {
        database_id: 1,
        snapshot_id: 1,
        segment_index_id: 1,
        manifest_version: 1,
        base_checkpoint_lsn: Lsn::new(1),
        required_wal_start_lsn: Lsn::new(1),
        parent_manifest_hash: [1u8; 32],
    };

    // Entry points at cold segment file_id=999, which does not exist on disk.
    // All checksum fields carry valid non-zero dummy values — the index codec
    // only checks non-zero, not cryptographic correctness.
    let entry = SegmentIndexEntryV0::new(
        SegmentId::new(1),
        ObjectId::new(1),
        AllocationId::new(1),
        ExtentId::new(1),
        1u32, // extent_count
        PageSize::KiB16,
        PageId::new(1),           // first_page_id
        1u32,                     // page_count
        Lsn::new(1),              // min_page_lsn
        Lsn::new(1),              // max_page_lsn
        1u64,                     // snapshot_id (must match ctx.snapshot_id)
        999u64,                   // segment_file_id — nonexistent, proves no cold I/O
        0u64,                     // segment_file_offset
        4096u64,                  // segment_byte_len (non-zero)
        0xDEAD_BEEF_CAFE_BABEu64, // segment_payload_crc64 (non-zero dummy)
        [0xABu8; 32],             // segment_sha256 (non-zero dummy)
        0xDEAD_BEEFu32,           // segment_header_crc32 (non-zero dummy)
        0xCAFE_BABEu32,           // segment_trailer_crc32 (non-zero dummy)
    )
    .expect("minimal segment index entry should be valid");

    let index = SegmentIndexV0::new(ctx, vec![entry], vec![])
        .expect("minimal segment index should be valid");
    index.encode().expect("minimal segment index should encode")
}

/// Create a WAL file with one record so that startup can scan it without
/// hitting a "WAL file is empty" or other early scan rejection.
fn write_minimal_wal(path: &PathBuf) {
    use andromeda_types::TransactionId;

    std::fs::remove_file(path).ok();
    let mut wal = FileWal::open(path).unwrap();
    let tx = TransactionId::new(1);
    wal.append_tx_begin(tx).unwrap();
    wal.append_tx_commit(tx).unwrap();
    wal.flush_all().unwrap();
}

// ── tests ─────────────────────────────────────────────────────────────────────

/// Invariant 1: bootstrap mode (None) sets `segment_index_validated = true`
/// without decoding any bytes.
#[test]
fn startup_segment_index_none_yields_validated_true() {
    let path = test_wal_path("none-bootstrap");
    write_minimal_wal(&path);

    let result = plan_file_wal_startup_recovery_v0_with_segment_index(
        &startup_manifest(),
        StartupMode::SafeStart,
        &path,
        None,
        false,
    )
    .expect("bootstrap startup with no segment index should succeed");

    assert!(
        result.evidence.segment_index_validated,
        "bootstrap mode must set segment_index_validated = true"
    );

    std::fs::remove_file(&path).ok();
}

/// Invariant 2: corrupt bytes cause early rejection BEFORE WAL I/O.
///
/// Proof: the WAL path is nonexistent.  If the segment-index error fires first
/// the function returns an error mentioning "segment index decode failed before
/// WAL scan".  If WAL I/O ran first the error would mention the missing file.
#[test]
fn startup_segment_index_corrupt_bytes_rejected_before_wal_scan() {
    // Use a WAL path that definitely does not exist.
    let nonexistent_wal =
        std::env::temp_dir().join(format!("andromeda-nonexistent-{}.wal", std::process::id()));

    let corrupt_bytes: &[u8] = b"this is definitely not a valid segment index";

    let err = plan_file_wal_startup_recovery_v0_with_segment_index(
        &startup_manifest(),
        StartupMode::SafeStart,
        &nonexistent_wal,
        Some(corrupt_bytes),
        false,
    )
    .expect_err("corrupt segment index bytes must cause early rejection");

    assert!(
        err.message()
            .contains("segment index decode failed before WAL scan"),
        "error must name the segment index as the failure site (ordering proof); got: {}",
        err.message()
    );
}

/// Invariant 3: valid segment-index bytes are decoded successfully and the
/// `segment_index_validated` flag is `true` in the returned evidence.
#[test]
fn startup_segment_index_valid_bytes_yields_validated_true() {
    let path = test_wal_path("valid-segment-bytes");
    write_minimal_wal(&path);

    let bytes = minimal_valid_segment_index_bytes();

    let result = plan_file_wal_startup_recovery_v0_with_segment_index(
        &startup_manifest(),
        StartupMode::SafeStart,
        &path,
        Some(&bytes),
        false,
    )
    .expect("startup with valid segment index bytes should succeed");

    assert!(
        result.evidence.segment_index_validated,
        "valid segment index bytes must set segment_index_validated = true"
    );

    std::fs::remove_file(&path).ok();
}

/// Invariant 4: no cold-segment file I/O.
///
/// The segment index entry references `segment_file_id = 999` — a file that
/// does not exist on disk.  The startup function only decodes the bytes in
/// memory; it never opens segment files.  If the test passes, no segment-file
/// I/O occurred (otherwise the OS would surface an error when trying to open
/// the nonexistent file).
#[test]
fn no_cold_scan_startup_with_segment_entries_for_nonexistent_files() {
    let path = test_wal_path("no-cold-io");
    write_minimal_wal(&path);

    let bytes = minimal_valid_segment_index_bytes();

    // The entry in `bytes` points to file_id=999 which does not exist.
    // The startup function must NOT attempt to open it.  If it did, we would
    // get an I/O error here instead of a successful decode.
    let result = plan_file_wal_startup_recovery_v0_with_segment_index(
        &startup_manifest(),
        StartupMode::SafeStart,
        &path,
        Some(&bytes),
        false,
    )
    .expect("startup must not open cold segment files; evidence is bytes-only");

    assert!(
        result.evidence.segment_index_validated,
        "segment_index_validated must be true (bytes decoded in memory, no cold I/O)"
    );

    std::fs::remove_file(&path).ok();
}
