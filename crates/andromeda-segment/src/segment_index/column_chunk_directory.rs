//! `ColumnChunkDirectory` v0 — storage-side codec for columnar chunk metadata.
//!
//! This module defines the durable wire format for column chunk metadata persisted
//! inside a [`super::SegmentIndexV0`] extension record.  The in-memory descriptor
//! surface lives in `andromeda-columnar`; this module owns only the explicit
//! little-endian binary codec that survives restart and crash recovery.
//!
//! # Why a separate codec?
//!
//! `andromeda-segment` is a C5 durable-kernel crate.  It must not import
//! `andromeda-columnar` (analytics/advisory boundary).  The storage layer
//! therefore defines its own wire-format struct — `ColumnChunkDirectoryEntry` —
//! that captures only the fields needed for durable snapshot binding and pruning
//! evidence.
//!
//! # Extension record wire layout
//!
//! The directory is encoded as an **optional** `SegmentIndexV0` extension
//! record (flags `0x0000`).  Older readers that do not recognise extension type
//! `0x0010` will skip the record rather than reject the index.
//!
//! ```text
//! Extension header (8 bytes):
//!   [0..2]   ext_type    u16 LE = 0x0010
//!   [2..4]   ext_flags   u16 LE = 0x0000   (optional; older readers skip)
//!   [4..8]   payload_len u32 LE
//!
//! Payload (4 + chunk_count × 64 bytes):
//!   [0..4]   chunk_count u32 LE  (≥ 1)
//!   [4..]    entries     chunk_count × 64-byte ColumnChunkDirectoryEntry records
//! ```
//!
//! # Per-entry wire layout
//!
//! Each entry is exactly 64 bytes, all fields little-endian:
//!
//! ```text
//! Offset  Size  Field
//! ──────  ────  ─────────────────────────────────────────────────────────
//!  +0      4    ordinal                    u32 LE
//!  +4      4    column_id                  u32 LE
//!  +8      8    offset                     u64 LE
//! +16      4    length                     u32 LE
//! +20      8    min_value_inline_or_offset u64 LE
//! +28      8    max_value_inline_or_offset u64 LE
//! +36      8    bloom_bytes_offset         u64 LE  (0 = absent)
//! +44      4    bloom_bytes_len            u32 LE  (0 = absent)
//! +48      8    source_snapshot_lsn        u64 LE  (must be non-zero)
//! +56      8    reserved                   u64 LE  (must be zero)
//! ```
//!
//! # Version-handling strategy
//!
//! `ColumnChunkDirectory` uses **additive extension** so that existing
//! `SegmentIndexV0` readers remain compatible:
//! - The extension record flags are `0x0000` (optional), so unknown readers
//!   skip rather than reject.
//! - The `SegmentIndexV0` format version (`1.0`) is **not bumped**; only the
//!   extension type tag is new.
//! - Future breaking changes to the directory layout must use a new extension
//!   type tag (e.g., `0x0011`) and bump `SegmentIndexV0` format major version.

use super::error::{SegmentIndexError, SegmentIndexResult};

/// Extension record type tag that identifies a `ColumnChunkDirectory` record
/// inside the `SegmentIndexV0` extension section.
pub const COLUMN_CHUNK_DIRECTORY_EXT_TYPE: u16 = 0x0010;

/// Extension record flags for `ColumnChunkDirectory`.
///
/// `0x0000` means the record is **optional**: older readers that do not
/// recognise `COLUMN_CHUNK_DIRECTORY_EXT_TYPE` will skip it rather than
/// reject the index.  Do not set the required flag (`0x0001`) here without
/// bumping `SegmentIndexV0` format major version.
pub const COLUMN_CHUNK_DIRECTORY_EXT_FLAGS: u16 = 0x0000;

/// Fixed byte length of one encoded [`ColumnChunkDirectoryEntry`] (64 bytes).
pub const COLUMN_CHUNK_DIRECTORY_ENTRY_LEN: usize = 64;

/// Storage-side wire-format record for one column chunk persisted in a cold segment.
///
/// This is the **durable encoding** of columnar chunk metadata.  The in-memory
/// descriptor counterpart lives in `andromeda-columnar::ColumnChunkDescriptor`.
/// The storage crate defines this type independently because `andromeda-segment`
/// is a C5 durable-kernel crate that must not depend on analytics/advisory crates.
///
/// # Wire layout
///
/// See the module-level documentation for the exact 64-byte little-endian layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnChunkDirectoryEntry {
    /// Column ordinal (0-based column position within the segment).
    pub ordinal: u32,
    /// Stable catalog column identifier.
    pub column_id: u32,
    /// Byte offset of chunk data within the segment file.
    pub offset: u64,
    /// Byte length of chunk data (must be non-zero).
    pub length: u32,
    /// Inlined minimum value or offset into a side-channel value buffer.
    pub min_value_inline_or_offset: u64,
    /// Inlined maximum value or offset into a side-channel value buffer.
    pub max_value_inline_or_offset: u64,
    /// Byte offset of bloom filter bytes within the segment file (`0` = absent).
    pub bloom_bytes_offset: u64,
    /// Byte length of bloom filter bytes (`0` = absent).
    pub bloom_bytes_len: u32,
    /// LSN of the source snapshot this chunk was built from.
    ///
    /// Must be non-zero.  This field is the crash-recovery anchor: after
    /// replay, the recovered directory must present the same LSN that was
    /// durably written before the crash.
    pub source_snapshot_lsn: u64,
}

/// Storage-side directory of column chunks inside one cold segment.
///
/// A `ColumnChunkDirectory` is encoded as a `SegmentIndexV0` extension record.
/// Older readers (before type `0x0010` was defined) will skip the record and
/// treat the index as having no columnar metadata.
///
/// # Invariants
///
/// - `entries` must contain at least one entry.
/// - Every entry's `source_snapshot_lsn` must be non-zero.
/// - Every entry's `length` must be non-zero.
/// - Reserved bytes are forced to zero on encode and rejected on decode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnChunkDirectory {
    /// Ordered list of chunk entries, one per column chunk in this segment.
    pub entries: Vec<ColumnChunkDirectoryEntry>,
}

impl ColumnChunkDirectory {
    /// Encode this directory as a `SegmentIndexV0` extension record.
    ///
    /// Returns 8 bytes of extension-record header followed by the payload.
    /// Append the returned bytes to any other extension records to form
    /// `extension_bytes` for [`super::SegmentIndexV0::new`].
    ///
    /// # Errors
    ///
    /// Returns an error if the directory is empty, any entry fails validation,
    /// or the total length overflows.
    pub fn encode_as_extension(&self) -> SegmentIndexResult<Vec<u8>> {
        self.validate()?;

        let chunk_count =
            u32::try_from(self.entries.len()).map_err(|_| SegmentIndexError::LengthOverflow {
                field: "column chunk directory entry count",
            })?;

        let entries_byte_len = self
            .entries
            .len()
            .checked_mul(COLUMN_CHUNK_DIRECTORY_ENTRY_LEN)
            .ok_or(SegmentIndexError::LengthOverflow {
                field: "column chunk directory entries byte length",
            })?;
        // payload = 4-byte chunk_count + entries
        let payload_len =
            4_usize
                .checked_add(entries_byte_len)
                .ok_or(SegmentIndexError::LengthOverflow {
                    field: "column chunk directory payload length",
                })?;
        let payload_len_u32 =
            u32::try_from(payload_len).map_err(|_| SegmentIndexError::LengthOverflow {
                field: "column chunk directory payload length (u32)",
            })?;
        // total = 8-byte ext header + payload
        let total_len =
            8_usize
                .checked_add(payload_len)
                .ok_or(SegmentIndexError::LengthOverflow {
                    field: "column chunk directory total extension length",
                })?;

        let mut out = vec![0u8; total_len];

        // Extension header (8 bytes)
        out[0..2].copy_from_slice(&COLUMN_CHUNK_DIRECTORY_EXT_TYPE.to_le_bytes());
        out[2..4].copy_from_slice(&COLUMN_CHUNK_DIRECTORY_EXT_FLAGS.to_le_bytes());
        out[4..8].copy_from_slice(&payload_len_u32.to_le_bytes());

        // Payload: chunk_count at out[8..12]
        out[8..12].copy_from_slice(&chunk_count.to_le_bytes());

        // Payload: entries starting at out[12]
        let mut pos = 12_usize;
        for entry in &self.entries {
            encode_entry_into(&mut out[pos..pos + COLUMN_CHUNK_DIRECTORY_ENTRY_LEN], entry);
            pos += COLUMN_CHUNK_DIRECTORY_ENTRY_LEN;
        }

        Ok(out)
    }

    /// Decode a `ColumnChunkDirectory` from `SegmentIndexV0` extension bytes.
    ///
    /// Scans extension records and returns the first record whose type matches
    /// [`COLUMN_CHUNK_DIRECTORY_EXT_TYPE`].  Returns `Ok(None)` when no such
    /// record is present (segment index written by an older writer).
    ///
    /// # Errors
    ///
    /// Returns an error if a matching record is found but its payload is
    /// truncated, malformed, or contains invalid entry data.
    pub fn decode_from_extension_bytes(
        ext: &[u8],
    ) -> SegmentIndexResult<Option<ColumnChunkDirectory>> {
        let mut offset = 0_usize;
        while offset < ext.len() {
            // Extension record header: type(2) + flags(2) + payload_len(4)
            let header_end = offset
                .checked_add(8)
                .ok_or(SegmentIndexError::LengthOverflow {
                    field: "column chunk directory extension header end",
                })?;
            if header_end > ext.len() {
                return Err(SegmentIndexError::Truncated {
                    field: "column chunk directory extension header",
                });
            }

            let ext_type = u16::from_le_bytes([ext[offset], ext[offset + 1]]);
            let payload_len_u32 = u32::from_le_bytes([
                ext[offset + 4],
                ext[offset + 5],
                ext[offset + 6],
                ext[offset + 7],
            ]);
            let payload_len = usize::try_from(payload_len_u32).map_err(|_| {
                SegmentIndexError::LengthOverflow {
                    field: "column chunk directory extension payload length",
                }
            })?;
            let record_end =
                header_end
                    .checked_add(payload_len)
                    .ok_or(SegmentIndexError::LengthOverflow {
                        field: "column chunk directory extension record end",
                    })?;
            if record_end > ext.len() {
                return Err(SegmentIndexError::Truncated {
                    field: "column chunk directory extension payload",
                });
            }

            if ext_type == COLUMN_CHUNK_DIRECTORY_EXT_TYPE {
                let payload = &ext[header_end..record_end];
                return Ok(Some(decode_payload(payload)?));
            }

            offset = record_end;
        }
        Ok(None)
    }

    /// Validate the directory contents.
    fn validate(&self) -> SegmentIndexResult<()> {
        if self.entries.is_empty() {
            return Err(SegmentIndexError::InvalidExtension {
                reason: "column chunk directory must contain at least one entry",
            });
        }
        for entry in &self.entries {
            entry.validate()?;
        }
        Ok(())
    }
}

impl ColumnChunkDirectoryEntry {
    /// Validate this entry's field-level invariants.
    fn validate(&self) -> SegmentIndexResult<()> {
        if self.source_snapshot_lsn == 0 {
            return Err(SegmentIndexError::InvalidExtension {
                reason: "column chunk directory entry source_snapshot_lsn must not be zero",
            });
        }
        if self.length == 0 {
            return Err(SegmentIndexError::InvalidExtension {
                reason: "column chunk directory entry length must not be zero",
            });
        }
        if self.offset.checked_add(u64::from(self.length)).is_none() {
            return Err(SegmentIndexError::InvalidExtension {
                reason: "column chunk directory entry byte range overflows u64",
            });
        }
        Ok(())
    }
}

/// Encode one entry into a 64-byte buffer.
///
/// # Preconditions
///
/// `buf.len()` must equal [`COLUMN_CHUNK_DIRECTORY_ENTRY_LEN`].
fn encode_entry_into(buf: &mut [u8], entry: &ColumnChunkDirectoryEntry) {
    debug_assert_eq!(buf.len(), COLUMN_CHUNK_DIRECTORY_ENTRY_LEN);
    buf[0..4].copy_from_slice(&entry.ordinal.to_le_bytes());
    buf[4..8].copy_from_slice(&entry.column_id.to_le_bytes());
    buf[8..16].copy_from_slice(&entry.offset.to_le_bytes());
    buf[16..20].copy_from_slice(&entry.length.to_le_bytes());
    buf[20..28].copy_from_slice(&entry.min_value_inline_or_offset.to_le_bytes());
    buf[28..36].copy_from_slice(&entry.max_value_inline_or_offset.to_le_bytes());
    buf[36..44].copy_from_slice(&entry.bloom_bytes_offset.to_le_bytes());
    buf[44..48].copy_from_slice(&entry.bloom_bytes_len.to_le_bytes());
    buf[48..56].copy_from_slice(&entry.source_snapshot_lsn.to_le_bytes());
    buf[56..64].copy_from_slice(&0u64.to_le_bytes()); // reserved — always zero
}

/// Decode a `ColumnChunkDirectory` from the raw payload bytes (after the
/// 8-byte extension record header has been stripped).
fn decode_payload(payload: &[u8]) -> SegmentIndexResult<ColumnChunkDirectory> {
    if payload.len() < 4 {
        return Err(SegmentIndexError::Truncated {
            field: "column chunk directory chunk_count",
        });
    }
    let chunk_count = usize::try_from(u32::from_le_bytes([
        payload[0], payload[1], payload[2], payload[3],
    ]))
    .map_err(|_| SegmentIndexError::LengthOverflow {
        field: "column chunk directory chunk_count",
    })?;
    if chunk_count == 0 {
        return Err(SegmentIndexError::InvalidExtension {
            reason: "column chunk directory chunk_count must not be zero",
        });
    }
    let entries_len = chunk_count
        .checked_mul(COLUMN_CHUNK_DIRECTORY_ENTRY_LEN)
        .ok_or(SegmentIndexError::LengthOverflow {
            field: "column chunk directory entries table length",
        })?;
    let expected_payload =
        4_usize
            .checked_add(entries_len)
            .ok_or(SegmentIndexError::LengthOverflow {
                field: "column chunk directory expected payload length",
            })?;
    if payload.len() < expected_payload {
        return Err(SegmentIndexError::Truncated {
            field: "column chunk directory entries",
        });
    }

    let mut entries = Vec::with_capacity(chunk_count);
    for i in 0..chunk_count {
        let start = 4 + i * COLUMN_CHUNK_DIRECTORY_ENTRY_LEN;
        let end = start + COLUMN_CHUNK_DIRECTORY_ENTRY_LEN;
        entries.push(decode_entry(&payload[start..end])?);
    }
    Ok(ColumnChunkDirectory { entries })
}

/// Decode one entry from a 64-byte slice.
///
/// # Preconditions
///
/// `buf.len()` must equal [`COLUMN_CHUNK_DIRECTORY_ENTRY_LEN`].
fn decode_entry(buf: &[u8]) -> SegmentIndexResult<ColumnChunkDirectoryEntry> {
    debug_assert_eq!(buf.len(), COLUMN_CHUNK_DIRECTORY_ENTRY_LEN);
    // Reject non-zero reserved bytes
    let reserved = u64::from_le_bytes([
        buf[56], buf[57], buf[58], buf[59], buf[60], buf[61], buf[62], buf[63],
    ]);
    if reserved != 0 {
        return Err(SegmentIndexError::InvalidExtension {
            reason: "column chunk directory entry reserved bytes must be zero",
        });
    }
    let entry = ColumnChunkDirectoryEntry {
        ordinal: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
        column_id: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
        offset: u64::from_le_bytes([
            buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
        ]),
        length: u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]),
        min_value_inline_or_offset: u64::from_le_bytes([
            buf[20], buf[21], buf[22], buf[23], buf[24], buf[25], buf[26], buf[27],
        ]),
        max_value_inline_or_offset: u64::from_le_bytes([
            buf[28], buf[29], buf[30], buf[31], buf[32], buf[33], buf[34], buf[35],
        ]),
        bloom_bytes_offset: u64::from_le_bytes([
            buf[36], buf[37], buf[38], buf[39], buf[40], buf[41], buf[42], buf[43],
        ]),
        bloom_bytes_len: u32::from_le_bytes([buf[44], buf[45], buf[46], buf[47]]),
        source_snapshot_lsn: u64::from_le_bytes([
            buf[48], buf[49], buf[50], buf[51], buf[52], buf[53], buf[54], buf[55],
        ]),
    };
    entry.validate()?;
    Ok(entry)
}
