//! Explicit little-endian binary codec for [`crate::DatabaseManifest`].
//!
//! # On-disk layout — 112 bytes (format version 2)
//!
//! ```text
//! Offset  Size  Field
//!   0      8    magic           = b"DBMNFST\0"
//!   8      8    format_version  = 2 (u64 LE, discriminant for future evolution)
//!  16      8    database_id     (u64 LE)
//!  24      8    manifest_version (u64 LE)
//!  32      8    snapshot_id     (u64 LE)
//!  40      8    base_checkpoint_lsn  (u64 LE)
//!  48      8    required_wal_start_lsn (u64 LE)
//!  56     32    previous_manifest_hash ([u8; 32])
//!  88      4    manifest_crc    (u32 LE — domain CRC, carried through the WAL payload)
//!  92      8    segment_index_file_id  (u64 LE — 0 = no segment index yet)
//! 100      8    btree_root_page_id     (u64 LE — 0 = no root yet)
//! 108      4    payload_crc32c  (u32 LE — CRC32C over bytes [0..108])
//!            = 112 bytes total
//! ```
//!
//! ## Format history
//!
//! | Version | Size    | Change                                       |
//! |---------|---------|----------------------------------------------|
//! | 1 (W3)  | 96 B    | Initial manifest codec                       |
//! | 2 (W4)  | 112 B   | Added `segment_index_file_id` + `btree_root_page_id` |
//!
//! Records with format_version < 2 are rejected at `decode` time (version mismatch).
//! Forward-compatibility: unknown format_version values are always rejected.
//!
//! The `payload_crc32c` field covers all preceding bytes and is the primary
//! corruption-detection layer for on-disk manifest records. The domain
//! `manifest_crc` field is *not* recomputed by the codec — it is carried
//! through as supplied by the caller, consistent with the WAL payload protocol.
//!
//! ## Invariants
//! - No native Rust struct layout is used on disk.
//! - All multi-byte fields are explicit little-endian.
//! - `decode` rejects any record whose `payload_crc32c` does not match.
//! - `decode` rejects unknown magic or unsupported `format_version`.

#![forbid(unsafe_code)]

use andromeda_error::{AndromedaError, AndromedaErrorKind, AndromedaResult};
use andromeda_wal::Lsn;

use crate::DatabaseManifest;

// ─── Format constants ────────────────────────────────────────────────────────

/// Magic sentinel bytes at offset 0 of every encoded `DatabaseManifest` record.
pub const MANIFEST_RECORD_MAGIC: [u8; 8] = *b"DBMNFST\0";

/// Format-version discriminant (at offset 8).  Increment on breaking changes.
///
/// Version 1 (W3): 96-byte record without locator fields.
/// Version 2 (W4): 112-byte record adding `segment_index_file_id` and `btree_root_page_id`.
pub const MANIFEST_FORMAT_VERSION: u64 = 2;

/// Total encoded size of a `DatabaseManifest` record (format version 2).
pub const MANIFEST_ENCODED_SIZE: usize = 112;

// ─── Encode ──────────────────────────────────────────────────────────────────

/// Encode a [`DatabaseManifest`] to exactly [`MANIFEST_ENCODED_SIZE`] bytes.
///
/// The `payload_crc32c` field at bytes [108..112] is computed from bytes [0..108]
/// and appended automatically.  The domain `manifest_crc` field at [88..92]
/// is written as supplied — it is the caller's responsibility to ensure it is
/// meaningful before calling `encode`.
#[must_use]
pub fn encode_manifest(manifest: &DatabaseManifest) -> [u8; MANIFEST_ENCODED_SIZE] {
    let mut buf = [0u8; MANIFEST_ENCODED_SIZE];

    // [0..8] magic
    buf[0..8].copy_from_slice(&MANIFEST_RECORD_MAGIC);
    // [8..16] format_version
    buf[8..16].copy_from_slice(&MANIFEST_FORMAT_VERSION.to_le_bytes());
    // [16..24] database_id
    buf[16..24].copy_from_slice(&manifest.database_id.to_le_bytes());
    // [24..32] manifest_version
    buf[24..32].copy_from_slice(&manifest.manifest_version.to_le_bytes());
    // [32..40] snapshot_id
    buf[32..40].copy_from_slice(&manifest.snapshot_id.to_le_bytes());
    // [40..48] base_checkpoint_lsn
    buf[40..48].copy_from_slice(&manifest.base_checkpoint_lsn.get().to_le_bytes());
    // [48..56] required_wal_start_lsn
    buf[48..56].copy_from_slice(&manifest.required_wal_start_lsn.get().to_le_bytes());
    // [56..88] previous_manifest_hash
    buf[56..88].copy_from_slice(&manifest.previous_manifest_hash);
    // [88..92] manifest_crc (domain CRC, caller-supplied)
    buf[88..92].copy_from_slice(&manifest.manifest_crc.to_le_bytes());
    // [92..100] segment_index_file_id (W4 extension — 0 = no segment index)
    buf[92..100].copy_from_slice(&manifest.segment_index_file_id.to_le_bytes());
    // [100..108] btree_root_page_id (W4 extension — 0 = no root yet)
    buf[100..108].copy_from_slice(&manifest.btree_root_page_id.to_le_bytes());
    // [108..112] payload_crc32c — covers bytes [0..108]
    let payload_crc = crc32c(&buf[..108]);
    buf[108..112].copy_from_slice(&payload_crc.to_le_bytes());

    buf
}

// ─── Decode ──────────────────────────────────────────────────────────────────

/// Decode a [`DatabaseManifest`] from exactly [`MANIFEST_ENCODED_SIZE`] bytes.
///
/// # Errors
///
/// Returns a [`AndromedaErrorKind::Storage`] error if:
/// - `bytes.len()` ≠ 112,
/// - magic sentinel does not match,
/// - `format_version` is not `MANIFEST_FORMAT_VERSION`, or
/// - `payload_crc32c` does not match the recomputed CRC32C over bytes [0..108].
pub fn decode_manifest(bytes: &[u8]) -> AndromedaResult<DatabaseManifest> {
    if bytes.len() != MANIFEST_ENCODED_SIZE {
        return Err(codec_error(
            "manifest record must be exactly 112 bytes; got wrong length",
        ));
    }

    // Verify magic.
    if bytes[0..8] != MANIFEST_RECORD_MAGIC {
        return Err(codec_error(
            "manifest record magic mismatch; record may be corrupt or misaligned",
        ));
    }

    // Verify format version.
    let format_version = read_u64_le(bytes, 8);
    if format_version != MANIFEST_FORMAT_VERSION {
        return Err(codec_error(
            "manifest record format version unsupported; upgrade reader",
        ));
    }

    // Verify payload CRC32C (covers bytes [0..108]).
    let stored_crc = read_u32_le(bytes, 108);
    let computed_crc = crc32c(&bytes[..108]);
    if stored_crc != computed_crc {
        return Err(codec_error(
            "manifest record payload CRC32C mismatch; record is corrupt",
        ));
    }

    // Decode fields.
    Ok(DatabaseManifest {
        database_id: read_u64_le(bytes, 16),
        manifest_version: read_u64_le(bytes, 24),
        snapshot_id: read_u64_le(bytes, 32),
        base_checkpoint_lsn: Lsn::new(read_u64_le(bytes, 40)),
        required_wal_start_lsn: Lsn::new(read_u64_le(bytes, 48)),
        previous_manifest_hash: {
            let mut h = [0u8; 32];
            h.copy_from_slice(&bytes[56..88]);
            h
        },
        manifest_crc: read_u32_le(bytes, 88),
        segment_index_file_id: read_u64_le(bytes, 92),
        btree_root_page_id: read_u64_le(bytes, 100),
    })
}

// ─── Field readers (validated length assumed by caller) ──────────────────────

#[inline]
fn read_u64_le(bytes: &[u8], offset: usize) -> u64 {
    // SAFETY: caller has already checked len == MANIFEST_ENCODED_SIZE (112),
    // and all offsets used internally fit within [0..108].
    let arr: [u8; 8] = bytes[offset..offset + 8]
        .try_into()
        .expect("slice length checked before field reads");
    u64::from_le_bytes(arr)
}

#[inline]
fn read_u32_le(bytes: &[u8], offset: usize) -> u32 {
    let arr: [u8; 4] = bytes[offset..offset + 4]
        .try_into()
        .expect("slice length checked before field reads");
    u32::from_le_bytes(arr)
}

// ─── CRC32C (Castagnoli polynomial) — pure software, no external deps ────────

/// Software CRC32C using the Castagnoli polynomial.
///
/// The lookup table is initialised once via [`std::sync::OnceLock`] and
/// reused for all subsequent calls.  No hardware intrinsics; no `unsafe`.
#[must_use]
pub fn crc32c(bytes: &[u8]) -> u32 {
    /// Castagnoli polynomial in bit-reversed form.
    const POLY: u32 = 0x82F6_3B78;

    static TABLE: std::sync::OnceLock<[u32; 256]> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| {
        let mut t = [0u32; 256];
        for (i, slot) in t.iter_mut().enumerate() {
            let mut v = i as u32;
            for _ in 0..8 {
                v = if v & 1 != 0 { (v >> 1) ^ POLY } else { v >> 1 };
            }
            *slot = v;
        }
        t
    });

    let mut crc: u32 = !0;
    for &b in bytes {
        crc = (crc >> 8) ^ table[((crc ^ u32::from(b)) & 0xFF) as usize];
    }
    !crc
}

// ─── Error helper ─────────────────────────────────────────────────────────────

fn codec_error(message: &'static str) -> AndromedaError {
    AndromedaError::new(AndromedaErrorKind::Storage, message)
}

// ─── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use andromeda_wal::Lsn;

    fn sample_manifest() -> DatabaseManifest {
        DatabaseManifest {
            database_id: 1,
            manifest_version: 42,
            snapshot_id: 7,
            base_checkpoint_lsn: Lsn::new(1000),
            required_wal_start_lsn: Lsn::new(1000),
            previous_manifest_hash: [0xAB; 32],
            manifest_crc: 0xDEAD_BEEF,
            segment_index_file_id: 0,
            btree_root_page_id: 0,
        }
    }

    fn sample_manifest_with_locators() -> DatabaseManifest {
        DatabaseManifest {
            database_id: 1,
            manifest_version: 42,
            snapshot_id: 7,
            base_checkpoint_lsn: Lsn::new(1000),
            required_wal_start_lsn: Lsn::new(1000),
            previous_manifest_hash: [0xAB; 32],
            manifest_crc: 0xDEAD_BEEF,
            segment_index_file_id: 0xCAFE_1234_5678_9ABC,
            btree_root_page_id: 0xDEAD_BEEF_0000_0001,
        }
    }

    #[test]
    fn codec_round_trip_preserves_all_fields() {
        let original = sample_manifest();
        let bytes = encode_manifest(&original);
        assert_eq!(bytes.len(), MANIFEST_ENCODED_SIZE);

        let decoded = decode_manifest(&bytes).expect("round-trip must succeed");
        assert_eq!(decoded, original);
    }

    #[test]
    fn codec_round_trip_preserves_locator_fields() {
        let original = sample_manifest_with_locators();
        let bytes = encode_manifest(&original);
        let decoded = decode_manifest(&bytes).expect("locator round-trip must succeed");
        assert_eq!(decoded.segment_index_file_id, 0xCAFE_1234_5678_9ABC);
        assert_eq!(decoded.btree_root_page_id, 0xDEAD_BEEF_0000_0001);
        assert_eq!(decoded, original);
    }

    #[test]
    fn codec_encoded_size_is_112_bytes() {
        let bytes = encode_manifest(&sample_manifest());
        assert_eq!(bytes.len(), 112);
    }

    #[test]
    fn codec_magic_is_stable() {
        let bytes = encode_manifest(&sample_manifest());
        assert_eq!(&bytes[0..8], b"DBMNFST\0");
    }

    #[test]
    fn codec_format_version_is_2_stored_le() {
        let bytes = encode_manifest(&sample_manifest());
        let v = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        assert_eq!(v, 2u64);
        assert_eq!(v, MANIFEST_FORMAT_VERSION);
    }

    #[test]
    fn codec_segment_index_file_id_at_offset_92() {
        let manifest = sample_manifest_with_locators();
        let bytes = encode_manifest(&manifest);
        let file_id = u64::from_le_bytes(bytes[92..100].try_into().unwrap());
        assert_eq!(file_id, manifest.segment_index_file_id);
    }

    #[test]
    fn codec_btree_root_page_id_at_offset_100() {
        let manifest = sample_manifest_with_locators();
        let bytes = encode_manifest(&manifest);
        let root_id = u64::from_le_bytes(bytes[100..108].try_into().unwrap());
        assert_eq!(root_id, manifest.btree_root_page_id);
    }

    #[test]
    fn codec_payload_crc32c_at_offset_108() {
        let bytes = encode_manifest(&sample_manifest());
        let stored_crc = u32::from_le_bytes(bytes[108..112].try_into().unwrap());
        let computed = crc32c(&bytes[..108]);
        assert_eq!(stored_crc, computed);
    }

    #[test]
    fn decode_rejects_wrong_magic() {
        let mut bytes = encode_manifest(&sample_manifest());
        bytes[0] = 0xFF;
        assert!(decode_manifest(&bytes).is_err());
        let err = decode_manifest(&bytes).unwrap_err();
        assert_eq!(err.kind(), andromeda_error::AndromedaErrorKind::Storage);
        assert!(err.message().contains("magic"));
    }

    #[test]
    fn decode_rejects_wrong_format_version() {
        let mut bytes = encode_manifest(&sample_manifest());
        // overwrite format_version with 99
        bytes[8..16].copy_from_slice(&99u64.to_le_bytes());
        // recompute CRC so the version check fires (not CRC)
        let crc = crc32c(&bytes[..108]);
        bytes[108..112].copy_from_slice(&crc.to_le_bytes());
        let err = decode_manifest(&bytes).unwrap_err();
        assert!(err.message().contains("format version"));
    }

    #[test]
    fn decode_rejects_payload_crc_mismatch() {
        let mut bytes = encode_manifest(&sample_manifest());
        // Flip one bit in database_id field
        bytes[16] ^= 0x01;
        let err = decode_manifest(&bytes).unwrap_err();
        assert!(err.message().contains("CRC32C"));
    }

    #[test]
    fn decode_rejects_wrong_length() {
        let short = [0u8; 50];
        assert!(decode_manifest(&short).is_err());
    }

    #[test]
    fn decode_rejects_v1_96_byte_record() {
        // Simulate a v1 record (96 bytes) — should fail with wrong-length error.
        let short = [0u8; 96];
        let err = decode_manifest(&short).unwrap_err();
        assert!(err.message().contains("112 bytes"));
    }

    #[test]
    fn crc32c_is_deterministic_and_nonzero_for_nonzero_input() {
        let a = crc32c(b"hello world");
        let b = crc32c(b"hello world");
        assert_eq!(a, b);
        assert_ne!(a, 0);
    }

    #[test]
    fn crc32c_known_vector() {
        // CRC32C (Castagnoli) of b"123456789" is 0xE306_9283 (standard test vector)
        assert_eq!(crc32c(b"123456789"), 0xE306_9283);
    }

    #[test]
    fn crc32c_empty_returns_zero_crc() {
        // CRC32C of empty bytes is !0xFFFF_FFFF = 0
        assert_eq!(crc32c(&[]), 0);
    }
}
