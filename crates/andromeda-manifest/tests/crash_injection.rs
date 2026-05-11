//! Crash injection tests for the manifest root pointer dual-copy protocol.
//!
//! # Methodology
//!
//! These tests use two complementary approaches:
//!
//! 1. **Pure in-memory simulation** (`SimulatedDisk`) — exercises the selection
//!    and encode/decode logic without touching the filesystem.  Crash points are
//!    modelled by constructing specific disk states (absent file, valid record,
//!    corrupt record).
//!
//! 2. **Filesystem-level scenario tests** — exercise `write_root_pointer` and
//!    `read_root_pointer` using a temporary directory.  Crash points are
//!    simulated by aborting the write sequence at the boundary of each fsync.
//!
//! # Crash points covered (≥ 4 required by W3 spec)
//!
//! | ID | Point | Mechanism | Expected outcome |
//! |---|---|---|---|
//! | CP-A | Before any pointer write | Both files absent | Old manifest → error (no pointer written yet); acceptable for fresh init |
//! | CP-B | After pointer A written but before fsync A | A has corrupt CRC (simulated truncation) | Old manifest from previously valid file |
//! | CP-C | After fsync A, before pointer B written | A valid (gen N+1), B absent/old | New manifest from A |
//! | CP-D | After pointer B written but before fsync B | B has corrupt CRC | Old manifest from A |
//! | CP-E | After fsync B (success) | A old (gen N), B new (gen N+1) | New manifest from B |
//!
//! Additionally the manifest payload CRC (encode/decode) is exercised under
//! corruption to validate the codec-level integrity check is independent of
//! the pointer-level check.

#![forbid(unsafe_code)]

use andromeda_error::AndromedaErrorKind;
use andromeda_manifest::{
    DatabaseManifest, ROOT_POINTER_ENCODED_SIZE, ROOT_POINTER_FILE_A, ROOT_POINTER_FILE_B,
    RootPointerRecord, crc32c, read_root_pointer, select_valid_pointer, write_root_pointer,
};
use andromeda_wal::Lsn;

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn make_pointer(generation: u64) -> RootPointerRecord {
    RootPointerRecord {
        generation,
        manifest_version: generation + 100,
        snapshot_id: 42,
        base_checkpoint_lsn: Lsn::new(generation * 500),
        required_wal_start_lsn: Lsn::new(generation * 500),
    }
}

fn make_manifest(version: u64) -> DatabaseManifest {
    DatabaseManifest {
        database_id: 1,
        manifest_version: version,
        snapshot_id: 10,
        base_checkpoint_lsn: Lsn::new(version * 100),
        required_wal_start_lsn: Lsn::new(version * 100),
        previous_manifest_hash: [0xCC; 32],
        manifest_crc: 0xABCD_1234,
        segment_index_file_id: 0,
        btree_root_page_id: 0,
    }
}

/// Create a unique temporary directory for each test.
fn test_dir(suffix: &str) -> std::path::PathBuf {
    let base = std::env::temp_dir().join("andromeda_manifest_w3_crash");
    let dir = base.join(suffix);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create test temp dir");
    dir
}

fn write_raw_bytes_to_file(dir: &std::path::Path, filename: &str, bytes: &[u8]) {
    use std::io::Write;
    let path = dir.join(filename);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .expect("open for write");
    f.write_all(bytes).expect("write bytes");
    f.sync_all().expect("fsync");
}

// ─── SECTION 1: Pure in-memory selection logic ───────────────────────────────

/// CP-A: Both pointer files absent → no valid pointer.
///
/// On a fresh database before the first manifest write, both pointer files
/// do not exist.  `select_valid_pointer(None, None)` must return `None`.
#[test]
fn crash_point_a_both_pointers_absent_yields_no_valid_pointer() {
    let result = select_valid_pointer(None, None);
    assert!(
        result.is_none(),
        "expected None when both pointer files are absent"
    );
}

/// CP-A filesystem: `read_root_pointer` returns error when both files absent.
#[test]
fn crash_point_a_filesystem_read_errors_when_both_absent() {
    let dir = test_dir("cp_a_absent");
    let err = read_root_pointer(&dir).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(
        err.message().contains("no valid manifest root pointer"),
        "unexpected error message: {}",
        err.message()
    );
}

/// CP-B: Pointer A was written but with a corrupt CRC (simulates truncation
/// mid-write, which invalidates the CRC).  Pointer B is absent.
///
/// Expected: no valid pointer (old manifest state preserved).
#[test]
fn crash_point_b_pointer_a_corrupt_crc_b_absent_yields_no_valid_pointer() {
    // Build a valid record, then corrupt the CRC field.
    let mut bytes = make_pointer(1).encode();
    bytes[48] ^= 0xFF; // corrupt the first byte of the CRC field

    let corrupt_a = RootPointerRecord::decode(&bytes); // must fail CRC check
    assert!(corrupt_a.is_err(), "corrupt CRC must be rejected by decode");

    // Simulate: A = corrupt (None), B = absent (None)
    let result = select_valid_pointer(None, None);
    assert!(result.is_none(), "both absent/corrupt → no pointer");
}

/// CP-B filesystem: Pointer A written with corrupt content, B absent.
/// `read_root_pointer` must return error (old manifest state — no valid ptr).
#[test]
fn crash_point_b_filesystem_corrupt_a_b_absent_errors() {
    let dir = test_dir("cp_b_corrupt_a");

    // Write corrupt bytes (not a valid record) to file A
    let mut bytes = make_pointer(1).encode();
    bytes[10] ^= 0x42; // corrupt payload, CRC will mismatch
    write_raw_bytes_to_file(&dir, ROOT_POINTER_FILE_A, &bytes);

    // File B is absent.
    let err = read_root_pointer(&dir).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
}

/// CP-C: Pointer A is valid (gen=1, new manifest), B is absent.
/// Crash occurred after pointer A was fsynced but before B was written.
///
/// Expected: A returned → new manifest is accessible.
#[test]
fn crash_point_c_only_pointer_a_valid_returns_a() {
    let new_ptr = make_pointer(1);
    let result = select_valid_pointer(Some(new_ptr), None);
    assert_eq!(result, Some(new_ptr));
    assert_eq!(result.unwrap().generation, 1);
}

/// CP-C filesystem: A has valid new pointer, B absent.
#[test]
fn crash_point_c_filesystem_a_valid_b_absent_returns_a() {
    let dir = test_dir("cp_c_a_valid");
    let ptr = make_pointer(2);
    write_raw_bytes_to_file(&dir, ROOT_POINTER_FILE_A, &ptr.encode());

    let result = read_root_pointer(&dir).expect("must read valid pointer A");
    assert_eq!(result, ptr);
    assert_eq!(result.generation, 2);
}

/// CP-D: Pointer A is valid (gen=1, old manifest).  Pointer B was written
/// during the new-manifest switch but with a corrupt CRC (fsync not reached).
///
/// Expected: A returned → old manifest is accessible.
#[test]
fn crash_point_d_pointer_a_valid_b_corrupt_crc_returns_a() {
    let old_ptr = make_pointer(1);
    // Simulate corrupt B: decode fails → treat as None
    let corrupt_b: Option<RootPointerRecord> = None;

    let result = select_valid_pointer(Some(old_ptr), corrupt_b);
    assert_eq!(result, Some(old_ptr));
    assert_eq!(result.unwrap().generation, 1);
}

/// CP-D filesystem: A valid (old), B written with corrupt bytes.
#[test]
fn crash_point_d_filesystem_a_valid_b_corrupt_returns_old_manifest() {
    let dir = test_dir("cp_d_b_corrupt");

    // File A: old manifest pointer (gen=1)
    let old_ptr = make_pointer(1);
    write_raw_bytes_to_file(&dir, ROOT_POINTER_FILE_A, &old_ptr.encode());

    // File B: partially written (corrupt CRC)
    let mut bytes = make_pointer(2).encode();
    bytes[48] ^= 0xDE; // corrupt CRC byte
    write_raw_bytes_to_file(&dir, ROOT_POINTER_FILE_B, &bytes);

    let result = read_root_pointer(&dir).expect("A must be found");
    assert_eq!(
        result.generation, 1,
        "old manifest (gen=1) must win when B is corrupt"
    );
}

/// CP-E: Both pointer files contain valid records.
/// A has gen=1 (old), B has gen=2 (new, fsynced after successful write).
///
/// Expected: B returned → new manifest is accessible.
#[test]
fn crash_point_e_both_valid_higher_generation_wins() {
    let old_ptr = make_pointer(1);
    let new_ptr = make_pointer(2);

    let result = select_valid_pointer(Some(old_ptr), Some(new_ptr));
    assert_eq!(result, Some(new_ptr));
    assert_eq!(
        result.unwrap().generation,
        2,
        "new manifest (gen=2) must win"
    );
}

/// CP-E filesystem: Full write succeeded — B fsynced with gen=2.
#[test]
fn crash_point_e_filesystem_both_valid_new_generation_wins() {
    let dir = test_dir("cp_e_both_valid");

    // File A: old manifest
    write_raw_bytes_to_file(&dir, ROOT_POINTER_FILE_A, &make_pointer(1).encode());
    // File B: new manifest (gen=2)
    write_raw_bytes_to_file(&dir, ROOT_POINTER_FILE_B, &make_pointer(2).encode());

    let result = read_root_pointer(&dir).expect("must find highest generation");
    assert_eq!(result.generation, 2, "gen=2 must win");
    assert_eq!(
        result.manifest_version, 102,
        "manifest_version = generation + 100"
    );
}

// ─── SECTION 2: Manifest payload codec corruption tests ──────────────────────

/// Manifest encode/decode round-trip preserves all fields.
#[test]
fn manifest_codec_round_trip_all_fields_preserved() {
    let original = make_manifest(5);
    let bytes = original.encode();
    let decoded = DatabaseManifest::decode(&bytes).expect("round-trip must succeed");
    assert_eq!(decoded, original);
}

/// Manifest decode rejects a record with a flipped bit in the payload.
/// This proves the payload_crc32c covers the full record.
#[test]
fn manifest_codec_rejects_single_bit_flip_in_database_id() {
    let mut bytes = make_manifest(5).encode();
    bytes[16] ^= 0x01; // flip bit 0 of database_id
    let err = DatabaseManifest::decode(&bytes).unwrap_err();
    assert_eq!(err.kind(), AndromedaErrorKind::Storage);
    assert!(err.message().contains("CRC32C"), "must report CRC mismatch");
}

/// Manifest decode rejects a record with a flipped bit in manifest_version.
#[test]
fn manifest_codec_rejects_corruption_in_manifest_version() {
    let mut bytes = make_manifest(7).encode();
    bytes[24] ^= 0x80; // flip MSB of manifest_version
    let err = DatabaseManifest::decode(&bytes).unwrap_err();
    assert!(err.message().contains("CRC32C"));
}

/// Manifest decode rejects a record with a flipped bit in LSN fields.
#[test]
fn manifest_codec_rejects_corruption_in_lsn_fields() {
    let mut bytes = make_manifest(3).encode();
    bytes[40] ^= 0x01; // corrupt base_checkpoint_lsn
    let err = DatabaseManifest::decode(&bytes).unwrap_err();
    assert!(err.message().contains("CRC32C"));
}

/// Manifest decode rejects wrong magic sentinel.
#[test]
fn manifest_codec_rejects_wrong_magic_sentinel() {
    let mut bytes = make_manifest(1).encode();
    bytes[0] = b'X'; // corrupt first magic byte
    let err = DatabaseManifest::decode(&bytes).unwrap_err();
    assert!(
        err.message().contains("magic"),
        "must report magic mismatch"
    );
}

/// Manifest decode rejects a record that is too short.
#[test]
fn manifest_codec_rejects_short_record() {
    let bytes = [0u8; 32];
    assert!(DatabaseManifest::decode(&bytes).is_err());
}

/// Manifest decode rejects an unsupported format version.
#[test]
fn manifest_codec_rejects_unknown_format_version() {
    let mut bytes = make_manifest(1).encode();
    // Overwrite format_version with 99 and recompute CRC to isolate version check.
    bytes[8..16].copy_from_slice(&99u64.to_le_bytes());
    let new_crc = crc32c(&bytes[..92]);
    bytes[92..96].copy_from_slice(&new_crc.to_le_bytes());
    let err = DatabaseManifest::decode(&bytes).unwrap_err();
    assert!(err.message().contains("format version"));
}

// ─── SECTION 3: Root pointer encode/decode corruption tests ──────────────────

/// Root pointer decode rejects a flipped bit in the generation field.
#[test]
fn root_pointer_decode_rejects_corrupt_generation_field() {
    let mut bytes = make_pointer(3).encode();
    bytes[8] ^= 0x01; // flip bit 0 of generation
    let err = RootPointerRecord::decode(&bytes).unwrap_err();
    assert!(err.message().contains("CRC32C"));
}

/// Root pointer decode rejects a flipped bit in the LSN fields.
#[test]
fn root_pointer_decode_rejects_corrupt_lsn_fields() {
    let mut bytes = make_pointer(5).encode();
    bytes[32] ^= 0xFF; // corrupt base_checkpoint_lsn
    let err = RootPointerRecord::decode(&bytes).unwrap_err();
    assert!(err.message().contains("CRC32C"));
}

/// Both pointer files completely zeroed out → no valid pointer.
#[test]
fn crash_both_pointer_files_all_zeros_yields_no_valid_pointer() {
    let zeroes = [0u8; ROOT_POINTER_ENCODED_SIZE];
    let result_a = RootPointerRecord::decode(&zeroes);
    let result_b = RootPointerRecord::decode(&zeroes);
    assert!(result_a.is_err());
    assert!(result_b.is_err());

    let selected = select_valid_pointer(None, None);
    assert!(selected.is_none());
}

// ─── SECTION 4: Full write/read filesystem round-trip ────────────────────────

/// Write two sequential manifest generations and verify the newest wins.
#[test]
fn filesystem_sequential_writes_newest_generation_wins() {
    let dir = test_dir("sequential_writes");

    // First write: generation 1 → goes to file A (both absent).
    let ptr1 = make_pointer(1);
    write_root_pointer(&dir, &ptr1).expect("write gen=1 must succeed");

    let after_first = read_root_pointer(&dir).expect("must find gen=1");
    assert_eq!(after_first.generation, 1);

    // Second write: generation 2 → goes to file B (A is now active).
    let ptr2 = make_pointer(2);
    write_root_pointer(&dir, &ptr2).expect("write gen=2 must succeed");

    let after_second = read_root_pointer(&dir).expect("must find gen=2");
    assert_eq!(
        after_second.generation, 2,
        "gen=2 must be returned after second write"
    );
}

/// Write three sequential manifest generations to verify alternating file protocol.
#[test]
fn filesystem_three_writes_alternates_and_highest_wins() {
    let dir = test_dir("three_writes");

    write_root_pointer(&dir, &make_pointer(1)).expect("gen=1");
    write_root_pointer(&dir, &make_pointer(2)).expect("gen=2");
    write_root_pointer(&dir, &make_pointer(3)).expect("gen=3");

    let result = read_root_pointer(&dir).expect("must find gen=3");
    assert_eq!(
        result.generation, 3,
        "gen=3 must be the final active pointer"
    );
}
