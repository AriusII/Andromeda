//! Durable root pointer — dual-file on-disk pointer to the current manifest.
//!
//! # Purpose
//!
//! Recovery needs a durable, always-discoverable file that tells startup which
//! manifest generation is "current" *without* replaying the full WAL.  This
//! module implements a **dual-copy** scheme: two pointer files
//! (`manifest_root_a`, `manifest_root_b`) each hold a [`RootPointerRecord`].
//! The record with the **highest valid generation** wins.
//!
//! # On-disk layout — 52 bytes per file
//!
//! ```text
//! Offset  Size  Field
//!   0      8    magic                 = b"MNFSTPTR"
//!   8      8    generation            (u64 LE — monotonically increasing)
//!  16      8    manifest_version      (u64 LE — matches DatabaseManifest.manifest_version)
//!  24      8    snapshot_id           (u64 LE)
//!  32      8    base_checkpoint_lsn   (u64 LE)
//!  40      8    required_wal_start_lsn (u64 LE)
//!  48      4    crc32c                (u32 LE — CRC32C over bytes [0..48])
//!            = 52 bytes total
//! ```
//!
//! # Write protocol
//!
//! ```text
//! 1. Write new manifest content to durable storage (WAL record / page flush).
//! 2. Identify the inactive pointer file (the one with the lower generation,
//!    or file A if both are absent/invalid).
//! 3. Write the new RootPointerRecord (generation = prev_generation + 1) to
//!    the inactive file.
//! 4. fsync the inactive file.
//!    → Both files now carry valid records; highest generation wins at read.
//! ```
//!
//! # Crash safety argument
//!
//! | Crash point | State after restart | Outcome |
//! |---|---|---|
//! | Before step 3 | Active file = old generation | Old manifest — correct ✓ |
//! | During step 3 (partial write) | Inactive file has corrupt CRC | Old manifest from active file ✓ |
//! | After step 3, before step 4 (unfsynced) | Inactive file may or may not persist | Old or new — both valid ✓ |
//! | After step 4 | Inactive file = new generation, durable | New manifest wins ✓ |
//!
//! ## Invariants
//! - No native Rust struct layout on disk.
//! - All multi-byte fields are explicit little-endian.
//! - CRC32C integrity check on every read; corrupt or truncated files are ignored.
//! - The file with the **highest valid generation** is authoritative.

#![forbid(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::Lsn;

use crate::codec::crc32c;

// ─── Format constants ────────────────────────────────────────────────────────

/// Magic sentinel at offset 0 of every root pointer record.
pub const ROOT_POINTER_MAGIC: [u8; 8] = *b"MNFSTPTR";

/// Encoded size of a root pointer record.
pub const ROOT_POINTER_ENCODED_SIZE: usize = 52;

/// File name for the first root pointer replica.
pub const ROOT_POINTER_FILE_A: &str = "manifest_root_a";

/// File name for the second root pointer replica.
pub const ROOT_POINTER_FILE_B: &str = "manifest_root_b";

// ─── Record type ─────────────────────────────────────────────────────────────

/// An on-disk root pointer record identifying the current manifest.
///
/// Two copies are maintained on disk (`manifest_root_a` and `manifest_root_b`).
/// The copy with the highest valid `generation` is the authoritative pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootPointerRecord {
    /// Monotonically increasing generation counter.  Higher generation wins.
    pub generation: u64,
    /// Manifest version this pointer refers to.
    pub manifest_version: u64,
    /// Cold-snapshot anchor for cross-checking at startup.
    pub snapshot_id: u64,
    /// Last completed checkpoint LSN at the time of the manifest switch.
    pub base_checkpoint_lsn: Lsn,
    /// Recovery floor anchor (minimum WAL start LSN for replay).
    pub required_wal_start_lsn: Lsn,
}

impl RootPointerRecord {
    /// Encode this record to exactly [`ROOT_POINTER_ENCODED_SIZE`] bytes.
    ///
    /// The CRC32C over bytes [0..48] is computed and stored at [48..52].
    #[must_use]
    pub fn encode(&self) -> [u8; ROOT_POINTER_ENCODED_SIZE] {
        let mut buf = [0u8; ROOT_POINTER_ENCODED_SIZE];
        buf[0..8].copy_from_slice(&ROOT_POINTER_MAGIC);
        buf[8..16].copy_from_slice(&self.generation.to_le_bytes());
        buf[16..24].copy_from_slice(&self.manifest_version.to_le_bytes());
        buf[24..32].copy_from_slice(&self.snapshot_id.to_le_bytes());
        buf[32..40].copy_from_slice(&self.base_checkpoint_lsn.get().to_le_bytes());
        buf[40..48].copy_from_slice(&self.required_wal_start_lsn.get().to_le_bytes());
        let crc = crc32c(&buf[..48]);
        buf[48..52].copy_from_slice(&crc.to_le_bytes());
        buf
    }

    /// Decode a root pointer record from exactly [`ROOT_POINTER_ENCODED_SIZE`] bytes.
    ///
    /// # Errors
    ///
    /// Returns [`AndromedaErrorKind::Storage`] if:
    /// - `bytes.len()` ≠ 52,
    /// - magic does not match, or
    /// - CRC32C over [0..48] does not match stored value at [48..52].
    pub fn decode(bytes: &[u8]) -> AndromedaResult<Self> {
        if bytes.len() != ROOT_POINTER_ENCODED_SIZE {
            return Err(ptr_error("root pointer record must be exactly 52 bytes"));
        }
        if bytes[0..8] != ROOT_POINTER_MAGIC {
            return Err(ptr_error(
                "root pointer magic mismatch; file may be corrupt or belong to another instance",
            ));
        }
        let stored_crc = read_u32_le(bytes, 48);
        let computed_crc = crc32c(&bytes[..48]);
        if stored_crc != computed_crc {
            return Err(ptr_error(
                "root pointer CRC32C mismatch; record is corrupt or truncated",
            ));
        }
        Ok(Self {
            generation: read_u64_le(bytes, 8),
            manifest_version: read_u64_le(bytes, 16),
            snapshot_id: read_u64_le(bytes, 24),
            base_checkpoint_lsn: Lsn::new(read_u64_le(bytes, 32)),
            required_wal_start_lsn: Lsn::new(read_u64_le(bytes, 40)),
        })
    }
}

// ─── Field readers ───────────────────────────────────────────────────────────

#[inline]
fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
    let arr: [u8; 8] = bytes[offset..offset + 8]
        .try_into()
        .expect("slice length validated by caller");
    u64::from_le_bytes(arr)
}

#[inline]
fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    let arr: [u8; 4] = bytes[offset..offset + 4]
        .try_into()
        .expect("slice length validated by caller");
    u32::from_le_bytes(arr)
}

// ─── Selection logic (pure, no I/O) ──────────────────────────────────────────

/// Select the authoritative record from two optional replicas.
///
/// Returns the record with the highest `generation` among valid (non-`None`)
/// inputs.  If both are `None`, returns `None` (no valid pointer exists).
#[must_use]
pub fn select_valid_pointer(
    a: Option<RootPointerRecord>,
    b: Option<RootPointerRecord>,
) -> Option<RootPointerRecord> {
    match (a, b) {
        (Some(ra), Some(rb)) => {
            if ra.generation >= rb.generation {
                Some(ra)
            } else {
                Some(rb)
            }
        },
        (Some(ra), None) => Some(ra),
        (None, Some(rb)) => Some(rb),
        (None, None) => None,
    }
}

// ─── Filesystem helpers ───────────────────────────────────────────────────────

fn pointer_paths(dir: &Path) -> (PathBuf, PathBuf) {
    (dir.join(ROOT_POINTER_FILE_A), dir.join(ROOT_POINTER_FILE_B))
}

fn read_pointer_file(path: &Path) -> Option<RootPointerRecord> {
    let mut file = File::open(path).ok()?;
    let mut buf = [0u8; ROOT_POINTER_ENCODED_SIZE];
    file.read_exact(&mut buf).ok()?;
    RootPointerRecord::decode(&buf).ok()
}

fn write_and_fsync_pointer_file(path: &Path, record: &RootPointerRecord) -> AndromedaResult<()> {
    let buf = record.encode();
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|e| {
            AndromedaError::with_source(
                AndromedaErrorKind::Storage,
                "cannot open root pointer file for write",
                e,
            )
        })?;
    file.write_all(&buf).map_err(|e| {
        AndromedaError::with_source(
            AndromedaErrorKind::Storage,
            "cannot write root pointer record bytes",
            e,
        )
    })?;
    file.flush().map_err(|e| {
        AndromedaError::with_source(
            AndromedaErrorKind::Storage,
            "cannot flush root pointer file",
            e,
        )
    })?;
    file.sync_all().map_err(|e| {
        AndromedaError::with_source(
            AndromedaErrorKind::Storage,
            "cannot fsync root pointer file",
            e,
        )
    })?;
    Ok(())
}

// ─── Public I/O API ───────────────────────────────────────────────────────────

/// Write a root pointer record to the database directory using the dual-copy
/// protocol.
///
/// The write targets the *inactive* pointer file (the one with the lower
/// generation, or `manifest_root_a` if both files are absent/invalid).
/// After this call returns `Ok`, at least one durable pointer file carries the
/// new record.
///
/// # Protocol
///
/// ```text
/// 1. Read both pointer files to identify the current active one.
/// 2. Select the inactive file (opposite of the active one).
/// 3. Write `record` to the inactive file.
/// 4. fsync the inactive file.
/// ```
///
/// # Errors
///
/// Returns a [`AndromedaErrorKind::Storage`] error if the write or fsync fails.
pub fn write_root_pointer(dir: &Path, record: &RootPointerRecord) -> AndromedaResult<()> {
    let (path_a, path_b) = pointer_paths(dir);
    let current_a = read_pointer_file(&path_a);
    let current_b = read_pointer_file(&path_b);

    // Write to the inactive (lower-generation) file.
    //  - Both absent  → write to A
    //  - Only A valid → write to B
    //  - Only B valid → write to A
    //  - Both valid   → write to whichever has the lower generation
    let write_to_b = match (current_a, current_b) {
        (Some(a), Some(b)) => a.generation > b.generation, // A is active → write to B
        (Some(_), None) => true,                           // A is active → write to B
        (None, Some(_)) => false,                          // B is active → write to A
        (None, None) => false,                             // Both absent → write to A first
    };

    let target = if write_to_b { &path_b } else { &path_a };
    write_and_fsync_pointer_file(target, record)
}

/// Read the highest-generation valid root pointer from the database directory.
///
/// Reads both [`ROOT_POINTER_FILE_A`] and [`ROOT_POINTER_FILE_B`].  Corrupt or
/// absent files are silently ignored.  Returns the record with the highest
/// valid `generation`.
///
/// # Errors
///
/// Returns a [`AndromedaErrorKind::Storage`] error if *no* valid pointer file
/// exists in `dir`.
pub fn read_root_pointer(dir: &Path) -> AndromedaResult<RootPointerRecord> {
    let (path_a, path_b) = pointer_paths(dir);
    let record_a = read_pointer_file(&path_a);
    let record_b = read_pointer_file(&path_b);

    select_valid_pointer(record_a, record_b)
        .ok_or_else(|| ptr_error("no valid manifest root pointer found in database directory"))
}

// ─── Error helper ─────────────────────────────────────────────────────────────

fn ptr_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_wal::Lsn;

    fn sample_record(generation: u64) -> RootPointerRecord {
        RootPointerRecord {
            generation,
            manifest_version: generation + 10,
            snapshot_id: 99,
            base_checkpoint_lsn: Lsn::new(500),
            required_wal_start_lsn: Lsn::new(500),
        }
    }

    #[test]
    fn root_pointer_round_trip_preserves_all_fields() {
        let original = sample_record(7);
        let bytes = original.encode();
        assert_eq!(bytes.len(), ROOT_POINTER_ENCODED_SIZE);
        let decoded = RootPointerRecord::decode(&bytes).expect("must decode");
        assert_eq!(decoded, original);
    }

    #[test]
    fn root_pointer_magic_is_stable() {
        let bytes = sample_record(1).encode();
        assert_eq!(&bytes[0..8], b"MNFSTPTR");
    }

    #[test]
    fn root_pointer_decode_rejects_wrong_magic() {
        let mut bytes = sample_record(1).encode();
        bytes[0] = 0xFF;
        let err = RootPointerRecord::decode(&bytes).unwrap_err();
        assert_eq!(err.kind(), AndromedaErrorKind::Storage);
        assert!(err.message().contains("magic"));
    }

    #[test]
    fn root_pointer_decode_rejects_corrupt_crc() {
        let mut bytes = sample_record(1).encode();
        bytes[8] ^= 0x01; // corrupt generation field
        let err = RootPointerRecord::decode(&bytes).unwrap_err();
        assert!(err.message().contains("CRC32C"));
    }

    #[test]
    fn root_pointer_decode_rejects_wrong_length() {
        assert!(RootPointerRecord::decode(&[0u8; 10]).is_err());
    }

    #[test]
    fn select_valid_pointer_highest_generation_wins() {
        let old = sample_record(1);
        let new = sample_record(2);

        assert_eq!(select_valid_pointer(Some(old), Some(new)), Some(new));
        assert_eq!(select_valid_pointer(Some(new), Some(old)), Some(new));
    }

    #[test]
    fn select_valid_pointer_prefers_single_valid() {
        let r = sample_record(5);
        assert_eq!(select_valid_pointer(Some(r), None), Some(r));
        assert_eq!(select_valid_pointer(None, Some(r)), Some(r));
    }

    #[test]
    fn select_valid_pointer_none_when_both_absent() {
        assert_eq!(select_valid_pointer(None, None), None);
    }

    #[test]
    fn select_valid_pointer_tie_goes_to_a() {
        let r = sample_record(5); // both same generation
        assert_eq!(select_valid_pointer(Some(r), Some(r)), Some(r));
    }
}
