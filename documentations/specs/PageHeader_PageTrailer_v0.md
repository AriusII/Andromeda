# PageHeader and PageTrailer v0 Specification

## Purpose

Define the accepted documentation contract for Andromeda page header and page
trailer bytes used by the current page codec, heap page candidate format, and
recovery format gates.

This specification records current code evidence. It does not promote every
prototype heap or B-Tree storage artifact to release durability, and it does
not authorize page flush before durable WAL coverage.

## Scope

This specification applies to the current `PageHeader`, `PageTrailer`,
`PageLayoutContract`, and `PageCodecV1` encoding in `andromeda-storage`.

It covers:

- page size and page type tags;
- explicit little-endian header and trailer codecs;
- normative header and trailer fields at the current codec level;
- checksum, hash, LSN, and linked-page evidence;
- decode-before-allocate page image handling;
- heap footer relationship where it affects header/trailer validation;
- corruption handling and recovery format gate policy;
- validation matrix and open fields.

## Non-goals

This specification does not:

- introduce SQL, gRPC, runtime JSON defaults, or Procedure contract bypasses;
- serialize Rust native structs directly to disk;
- define a full heap tuple, B-Tree node, manifest, cold snapshot, or WAL payload
  format;
- claim older raw heap fixtures are compatible release-bearing artifacts;
- define exact offsets for future fields that the current codec does not
  encode;
- claim a second torn-write guard field exists in the current trailer codec;
- make RAM, BufferPool state, temp storage, GPU output, or benchmark output a
  source of truth.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `documentations/ROADMAP_IMPLEMENTATION_2026.md` for the current durable
  storage priorities.
- `documentations/governance/decisions/DEC-032-storage-format-gate.md` for the
  storage format gate and pre-persistence V1 candidate status.
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md` for page and
  recovery concepts.
- `crates/andromeda-storage/src/page/layout.rs` for `PageHeader`,
  `PageTrailer`, page sizes, page types, and validation.
- `crates/andromeda-storage/src/page_codec_v1/format.rs` for fixed header and
  trailer lengths.
- `crates/andromeda-storage/src/page_codec_v1/codec.rs` for header/trailer
  offsets and decode order.
- `crates/andromeda-storage/src/page_codec_v1/integrity.rs` for payload CRC,
  hash, and torn-write guard computation.
- `crates/andromeda-storage/src/heap/format_v1.rs` for heap metadata placement
  and redundant slot-count validation.
- `crates/andromeda-storage/HEAP_PAGE_FORMAT_DESIGN.md` for heap page candidate
  intent and remaining compatibility caveats.
- `crates/andromeda-storage/tests/recovery_contract/format_gate.rs` for
  pre-redo storage format gate behavior.
- `crates/andromeda-storage/src/page_codec_v1/tests.rs` for executable golden
  page image evidence.

## Procedure

### Ownership

`andromeda-storage` owns the page domain model and explicit page codec.
Persistent page bytes must be produced and consumed through explicit codecs.
No caller may treat the Rust `PageHeader`, `PageTrailer`, or
`PageLayoutContract` memory layout as a durable page layout.

### Encoding policy

All multi-byte integer fields in the current `PageCodecV1` page header and page
trailer use little-endian encoding.

The current page format constants are:

| Constant | Value | Rule |
| --- | ---: | --- |
| `PageHeader::MAGIC` | `0x414e4452` | Required little-endian page magic. |
| `PageHeader::FORMAT_VERSION_V0` | `1` | Current page header version value. |
| `PageHeader::MIN_HEADER_LEN_V0` | `96` | Minimum conceptual header length retained by the domain model. |
| `PAGE_CODEC_V1_HEADER_LEN` | `112` | Fixed encoded header length used by current page codec. |
| `PAGE_CODEC_V1_TRAILER_LEN` | `48` | Fixed encoded trailer length. |
| `PAGE_CODEC_V1_HEADER_INTEGRITY_OFFSET` | `104` | Header integrity CRC field offset. |
| `PageTrailer::V0_LEN` | `48` | Trailer length in the domain model. |
| Heap payload offset | `112` | First byte heap tuple payload may occupy in current HeapPageV1 candidate. |

The file name uses `v0` because this is the current documentation acceptance
contract for the page header and trailer domain model. The current Rust codec
module is named `PageCodecV1`, and its encoded fixed header length is 112
bytes. Both facts must remain visible to avoid confusing the 96-byte minimum
domain header with the 112-byte codec header image.

### Page sizes

The current accepted page sizes are:

| Tag | Page size |
| ---: | --- |
| 1 | 16 KiB |
| 2 | 32 KiB |

Unknown page size tags must be rejected.

### Page types

The current accepted page types are:

| Tag | Page type |
| ---: | --- |
| 1 | `FixedRow` |
| 2 | `HybridRow` |
| 3 | `Manifest` |
| 4 | `Free` |

Unknown page type tags must be rejected.

### Page flags

The current accepted page flag bits are:

| Bit | Flag | Rule |
| ---: | --- | --- |
| `0x0001` | `HAS_PREVIOUS` | Must match whether `previous_page_id` is present. |
| `0x0002` | `HAS_NEXT` | Must match whether `next_page_id` is present. |
| `0x0004` | `COLD_IMMUTABLE_IMAGE` | Marks immutable cold image semantics where supported by higher layers. |

Unknown flag bits must be rejected.

### Encoded page header layout

The current `PageCodecV1` encoded page header is fixed at 112 bytes.

| Offset | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 4 | `magic` | Must equal `PageHeader::MAGIC`. |
| 4 | 2 | `format_version` | Must equal `1`. |
| 6 | 2 | `page_size_tag` | Must be a known page size tag. |
| 8 | 2 | `page_type_tag` | Must be a known page type tag. |
| 10 | 2 | `flags` | Only known page flag bits are valid. |
| 12 | 8 | `page_id` | Must be nonzero. |
| 20 | 8 | `object_id` | Must be nonzero for non-free pages. |
| 28 | 8 | `allocation_id` | Must be nonzero for non-free pages. |
| 36 | 8 | `page_lsn` | Must be nonzero. |
| 44 | 8 | `page_epoch` | Must be nonzero. |
| 52 | 8 | `previous_page_id` | Zero means absent; nonzero must not equal `page_id`. |
| 60 | 8 | `next_page_id` | Zero means absent; nonzero must not equal `page_id`. |
| 68 | 2 | `header_len` | Must equal `112` in `PageCodecV1`. |
| 72 | 4 | `payload_offset` | Must equal `112` in `PageCodecV1`. |
| 76 | 4 | `payload_len` | Number of payload bytes between header and trailer. |
| 80 | 4 | `free_start` | Must be within the payload region. |
| 84 | 4 | `free_end` | Must be greater than or equal to `free_start`. |
| 88 | 4 | `free_bytes` | Must equal `free_end - free_start`. |
| 92 | 2 | `slot_count` | Must be greater than or equal to `row_count` after conversion rules. |
| 96 | 4 | `row_count` | Must not exceed `slot_count`. |
| 100 | 4 | `header_crc` | Domain header CRC field; must be nonzero. |
| 104 | 4 | `header_integrity_crc` | CRC-32 over the fixed 112-byte encoded header with bytes 104 through 107 zeroed. |
| 108 | 4 | reserved/pending | Current encoder writes zero. Future use requires compatibility review. |

The `PageHeader` domain model retains `MIN_HEADER_LEN_V0 = 96`, but current
`PageCodecV1` requires `header_len = 112` and `payload_offset = 112`. Readers
must reject images that claim the fixed codec identity but do not use the fixed
length and payload offset.

### Encoded page trailer layout

The current encoded page trailer is fixed at 48 bytes and is placed immediately
after the payload for `PageCodecV1` images.

| Offset in trailer | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 8 | `payload_crc64` | Nonzero FNV-1a 64-bit checksum of payload bytes. |
| 8 | 32 | `page_hash` | Nonzero SHA-256 digest of payload bytes. |
| 40 | 8 | `torn_write_guard` | Nonzero FNV-derived guard over header identity, LSN, epoch, payload offset, payload length, and payload CRC. |

The conceptual storage notes mention two torn-write guards, but the current code
encodes one `torn_write_guard`. A second guard is pending and must not be
documented as implemented without a versioned codec change.

### Header integrity CRC

The header integrity CRC is a CRC-32 over the 112-byte encoded header with the
4-byte integrity field zeroed during calculation. A computed zero value is
normalized to `1`.

The decoder must:

1. require exactly 112 bytes for standalone header decode;
2. validate magic, format version, fixed `header_len`, and fixed
   `payload_offset`;
3. decode known tags and fields;
4. run `PageHeader::validate`;
5. reject a zero integrity CRC;
6. recompute the integrity CRC and reject mismatches.

### Payload integrity trailer

The trailer validates the payload region only, not the full page image.

Trailer evidence is computed from:

- `payload_crc64(payload)`;
- `payload_hash(payload)`;
- `torn_write_guard(header, payload_crc64)`.

The torn-write guard folds these header values in little-endian order:

- `magic`;
- `format_version`;
- `page_id`;
- `object_id`;
- `allocation_id`;
- `page_lsn`;
- `page_epoch`;
- `payload_crc64`;
- `payload_offset`;
- `payload_len`.

If the computed guard is zero or equals `page_id`, the current implementation
xors it with a fixed nonzero constant. Readers must reject a zero
`payload_crc64`, a zero `page_hash`, a zero `torn_write_guard`, or a
`torn_write_guard` that equals only the page id.

### Page LSN and WAL relationship

`page_lsn` is the page's durable recovery evidence. It must be nonzero.

The storage engine must preserve these invariants:

- a page mutation must be covered by durable WAL before the corresponding
  mutation is visible;
- a dirty page must not be flushed unless WAL durability covers the page LSN;
- recovery must validate supported storage subformats before WAL redo;
- normal startup must reject unsupported page, heap, B-Tree, or WAL payload
  subformats before mutation replay;
- `ForensicStart` may inspect unknown or drifted storage artifacts read-only
  with replay disabled.

This spec does not define the full WAL payload schema that produced a page LSN.
That relationship belongs to WAL payload and recovery specs.

### Linked-page policy

`previous_page_id` and `next_page_id` use zero as the absent marker in the
current header codec. When present:

- neither link may point to the page itself;
- previous and next links must differ when both are present;
- `HAS_PREVIOUS` and `HAS_NEXT` flag bits must match link presence.

The current header does not encode file id, segment id, or extent id. Those are
open fields for future manifest and allocation specifications.

### Heap footer relationship

The heap page candidate format uses the current 112-byte payload offset, a
48-byte trailer, and a 4-byte heap slot metadata footer immediately before the
trailer:

```text
metadata_offset = page_size - 48 - 4
slot_base = metadata_offset - slot_count * 5
```

The heap slot metadata is:

| Offset from metadata start | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 2 | `slot_count` | Little-endian authoritative footer slot count. |
| 2 | 2 | `free_offset` | Little-endian free-space boundary. |

This footer is not part of `PageTrailer`. It is named here because current heap
validation compares footer slot count with redundant persisted header slot
count when a recognizable header exists. If both sources exist and disagree,
the page must be rejected.

Legacy raw heap images may have optional legacy slot-count bytes. They are not
release compatibility evidence. DEC-032 requires explicit migration, rebuild,
or rejection policy before durable promotion.

### Decode-before-allocate

Page decoders must avoid unbounded allocation from untrusted page bytes.

Required order:

1. Verify the page image is at least `112 + 48` bytes before decoding a full
   `PageCodecV1` image.
2. Decode the fixed 112-byte header and validate magic, version, lengths, tags,
   flags, identifiers, LSN, free-space offsets, row/slot counts, and header
   integrity CRC.
3. Convert `payload_len` to `usize` only after header validation.
4. Compute `payload_end = 112 + payload_len` with checked arithmetic.
5. Require the image length to equal `payload_end + 48`.
6. Decode exactly 48 trailer bytes.
7. Validate payload CRC, payload hash, and torn-write guard before exposing the
   decoded payload as accepted page state.

Heap-specific readers must additionally validate page size, footer metadata,
slot directory bounds, free offset bounds, known slot flags, tuple ranges, and
non-overlap before returning live tuples.

### Corruption handling

Page decode is fail-closed. Readers must reject:

- unknown magic;
- unsupported format version;
- unknown page size tag;
- unknown page type tag;
- unknown page flags;
- zero page id;
- zero page LSN;
- zero page epoch;
- non-free page with zero object id or allocation id;
- free page that advertises slots or rows;
- self-referential or contradictory page links;
- fixed header length mismatch;
- fixed payload offset mismatch;
- payload region that overlaps the trailer or exceeds page size;
- invalid free-space offsets or inconsistent free byte count;
- row count greater than slot count;
- zero domain header CRC;
- zero or mismatched header integrity CRC;
- incorrect page image length;
- zero or mismatched payload CRC;
- zero or mismatched payload hash;
- zero or mismatched torn-write guard;
- heap footer/header slot-count ambiguity;
- heap slot directory overlap with payload;
- heap free offset outside the allowed free-space bounds;
- heap slot flags outside the known mask;
- heap tuple ranges outside bounds or overlapping.

Rejected pages must not be made visible as recovered state. Startup must either
reject normal open or, where explicitly supported, enter read-only forensic
inspection with replay disabled.

## Validation

Documentation acceptance checks:

- The spec distinguishes `PageHeader::MIN_HEADER_LEN_V0 = 96` from current
  `PageCodecV1` encoded header length `112`.
- The spec names only offsets supported by current codec evidence.
- The spec states that multi-byte page header/trailer integers use explicit
  little-endian codecs.
- The spec states that a second torn-write guard is pending and not currently
  encoded.
- The spec preserves WAL-before-page-flush and WAL-before-visible-commit.
- The spec does not promote DEC-032 pre-persistence candidate artifacts to
  release compatibility.
- The spec does not introduce SQL, gRPC, runtime JSON, or native Rust struct
  layout persistence.

Existing code evidence to use when code validation is allowed:

```powershell
cargo test -p andromeda-storage page_codec_v1
cargo test -p andromeda-storage --test recovery_contract
cargo test -p andromeda-storage --test buffer_pool_wal_fence_contract
cargo test -p andromeda-storage --test disk_manager_wal_integration
```

### Validation matrix

| Area | Positive case | Negative case | Required outcome |
| --- | --- | --- | --- |
| Page magic | `0x414e4452` | Any other value | Reject before payload decode. |
| Version | `1` | Unknown version | Reject as unsupported page format. |
| Page size tag | `1` or `2` | Unknown tag | Reject. |
| Page type tag | `1` through `4` | Unknown tag | Reject. |
| Header length | `112` for `PageCodecV1` | Any other value | Reject. |
| Payload offset | `112` for `PageCodecV1` | Any other value | Reject. |
| Header integrity | Recomputed CRC matches | CRC zero or mismatch | Reject. |
| Page identity | Nonzero page id, valid object/allocation ids | zero ids on non-free page | Reject. |
| LSN | Nonzero `page_lsn` | zero LSN | Reject. |
| Page links | Flags match optional links | self-link or flag mismatch | Reject. |
| Free space | `free_bytes == free_end - free_start` | invalid offsets or mismatch | Reject. |
| Payload length | Image length equals `112 + payload_len + 48` | truncated or extra bytes | Reject. |
| Trailer CRC | Recomputed payload CRC matches | payload or CRC changed | Reject. |
| Payload hash | SHA-256 payload hash matches | payload or hash changed | Reject. |
| Torn-write guard | Recomputed guard matches | guard changed or zero | Reject. |
| Heap footer | Footer slot count matches recognized header count | ambiguity or mismatch | Reject. |
| Heap slots | Known flags and in-bounds ranges | unknown flags, overlap, out of bounds | Reject. |
| Recovery format gate | Known storage fingerprints | unknown or unsupported subformat | Reject before redo, or forensic read-only with replay disabled. |
| WAL fence | Durable WAL covers page LSN | dirty page LSN beyond durable WAL | Reject flush or publication. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Header says 96 bytes but codec expects 112 | Conceptual minimum header was confused with current fixed codec header. | Use `PageHeader::MIN_HEADER_LEN_V0` only as the domain minimum and `PAGE_CODEC_V1_HEADER_LEN` for encoded images. |
| Header integrity CRC mismatch | Header bytes changed or CRC was computed without zeroing bytes 104 through 107. | Recompute CRC-32 over the fixed 112-byte header with the integrity field zeroed. |
| Payload CRC or hash mismatch | Payload bytes changed after trailer generation. | Reject the page and recover from a valid snapshot plus durable WAL. |
| Torn-write guard mismatch | Header identity, LSN, epoch, payload length, or payload CRC differs from the trailer evidence. | Treat as torn or mixed page image and reject normal startup. |
| Slot count ambiguity | Heap footer and recognized header slot-count source disagree. | Reject the page; require migration, rebuild, or forensic inspection. |
| Recovery accepts unknown page format | Format gate is bypassed. | Fail before redo except read-only `ForensicStart` with replay disabled. |
| Documentation mentions two trailer guards | Conceptual notes drifted ahead of current codec. | Mark the second guard pending until a versioned codec encodes it. |
| Page flush occurs beyond durable WAL | WAL-before-page-flush invariant was violated. | Reject flush/publication and preserve recovery evidence. |

## References

- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `documentations/governance/decisions/DEC-032-storage-format-gate.md`
- `crates/andromeda-storage/HEAP_PAGE_FORMAT_DESIGN.md`
- `crates/andromeda-storage/src/page/layout.rs`
- `crates/andromeda-storage/src/page_codec_v1.rs`
- `crates/andromeda-storage/src/page_codec_v1/binary.rs`
- `crates/andromeda-storage/src/page_codec_v1/codec.rs`
- `crates/andromeda-storage/src/page_codec_v1/format.rs`
- `crates/andromeda-storage/src/page_codec_v1/integrity.rs`
- `crates/andromeda-storage/src/heap/format_v1.rs`
- `crates/andromeda-storage/src/heap/validation.rs`
- `crates/andromeda-storage/src/buffer_pool/wal_durability.rs`
- `crates/andromeda-storage/src/disk_manager/page_store.rs`
- `crates/andromeda-storage/tests/recovery_contract/format_gate.rs`
- `crates/andromeda-storage/src/page_codec_v1/tests.rs`
