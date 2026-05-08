# SegmentIndex v0 Specification

## Purpose

Define the accepted documentation contract for `SegmentIndex v0`, the durable
cold snapshot index that maps published segment ranges to immutable ColdStore
segment artifacts and provides recovery-time validation evidence for contiguous
page ranges, extent ranges, page LSN bounds, and segment hashes.

`SegmentIndex v0` is persisted storage metadata. It is not a Rust native struct
layout, not a rebuildable cache by default, and not a substitute for page,
segment, manifest, or WAL validation.

## Scope

This specification applies to the physical segment index artifact referenced by
`DatabaseManifest v0` during startup, recovery, backup, restore, scrub,
forensic inspection, and cold snapshot publication.

It covers:

- explicit little-endian segment index file fields;
- segment index versioning and compatibility rules;
- fixed segment index entry layout;
- page, extent, object, allocation, and LSN range validation;
- checksums, hashes, and decode-before-allocate behavior;
- recovery behavior for missing, corrupt, stale, or incompatible segment
  indexes;
- fuzz, golden-vector, and crash validation gates.

## Current Implementation Status

The Rust workspace currently has `SegmentDescriptor`, `SegmentHeader`,
`SegmentTrailer`, `PublishedColdSegment`, and segment extent contiguity
validation. Those types validate segment identity, page ranges, LSN bounds,
header and trailer evidence, and ColdStore immutability after publication.

The workspace does not yet expose a release-promoted `SegmentIndex v0` disk
codec. This specification defines the durable byte contract that future codec,
publication, recovery, backup, restore, and fuzz work must implement or update
before segment index bytes are treated as a compatibility promise.

## Non-goals

This specification does not:

- introduce ad hoc SQL, gRPC, runtime JSON defaults, or untyped Procedure
  bypasses;
- serialize Rust native structs, enums, padding, `Vec`, `Option`, or pointer
  layouts to disk;
- define the full page, heap, B-Tree, WAL frame, backup artifact, or database
  manifest file format;
- permit ColdStore mutation after publication;
- permit segment split, rewrite, or page update-in-place for published cold
  segments;
- make RAM, temp files, HotStore staging state, GPU output, benchmark output,
  audit records, or trace records database truth;
- authorize recovery replay when manifest, segment index, segment, page, heap,
  B-Tree, or WAL payload formats are unknown;
- expose recovery, restore, scrub, or forensic controls through the Application
  surface.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `AGENTS.md` for Andromeda storage, WAL, recovery, and explicit binary-codec
  invariants.
- `documentations/ROADMAP_IMPLEMENTATION_2026.md` for durable storage V0 and
  cold snapshot publication priorities.
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md` for HotStore to
  ColdStore publication, segment contiguity, and recovery truth.
- `documentations/governance/decisions/DEC-032-storage-format-gate.md` for
  storage format rejection and forensic handling of unknown artifacts.
- `documentations/specs/DatabaseManifest_v0.md` for manifest references to
  segment index roots.
- `documentations/specs/PageHeader_PageTrailer_v0.md` for page checksum and
  page LSN validation.
- `documentations/specs/WalRecord_v0.md` for durable WAL and recovery replay
  boundaries.
- `crates/andromeda-storage/src/segment.rs` for current segment identity,
  header, trailer, and mutation validation.
- `crates/andromeda-storage/src/extent/segment_contiguity.rs` for contiguous
  extent coverage validation.
- `crates/andromeda-storage/src/cold_store.rs` for published cold segment
  immutability.
- `crates/andromeda-storage/src/manifest/cold_publication.rs` for ColdStore
  publication placement and GPU exclusion.

## Procedure

### Ownership

`andromeda-storage` owns the durable segment index format and explicit segment
index codec.

Other crates may consume segment index evidence through typed storage APIs.
They must not define their own segment index byte layout or persist the Rust
`SegmentDescriptor` memory layout.

### Encoding policy

All multi-byte integer fields in `SegmentIndex v0` use little-endian encoding.
Fixed hashes are byte arrays and are copied exactly as bytes.

The segment index codec must write and read fields by explicit offsets. It must
not derive bytes from Rust native struct layout, `bincode`, `serde` defaults,
debug output, pointer layout, platform alignment, or host endianness.

The segment index file has this top-level shape:

```text
SegmentIndexFileV0 =
    SegmentIndexHeaderV0
    SegmentIndexEntryV0[entry_count]
    ExtensionBytes[extension_len]
    SegmentIndexTrailerV0
```

All offset and length fields are absolute byte offsets from the start of the
segment index file. Readers must use checked arithmetic before converting
lengths to allocation sizes.

### Constants

| Constant | Value | Rule |
| --- | ---: | --- |
| `SEGMENT_INDEX_MAGIC` | `41 4e 44 53 47 49 58 30` | Literal bytes `ANDSGIX0`. |
| `SEGMENT_INDEX_FORMAT_MAJOR` | `1` | Initial durable segment index major version. |
| `SEGMENT_INDEX_FORMAT_MINOR` | `0` | Initial durable segment index minor version. |
| `SEGMENT_INDEX_BYTE_ORDER` | `0x0102` | Required little-endian byte-order marker. |
| `SEGMENT_INDEX_HEADER_LEN` | `256` | Fixed header length. |
| `SEGMENT_INDEX_ENTRY_LEN` | `160` | Fixed entry length. |
| `SEGMENT_INDEX_TRAILER_LEN` | `96` | Fixed trailer length. |
| `SEGMENT_INDEX_MAX_ENTRIES` | `16,777,216` | Decode and validation bound. |
| `SEGMENT_INDEX_MAX_EXTENSION_BYTES` | `1 MiB` | Extension decode bound. |
| `SEGMENT_STATE_PUBLISHED_COLD` | `3` | Only accepted state for v0 manifest-referenced indexes. |

The file name uses `v0` because this is the first accepted documentation
contract. The encoded durable format version uses major `1`, minor `0` to
match the existing `FormatVersion::V1_0` convention.

### Checksum and hash algorithms

`SegmentIndex v0` uses these integrity algorithms:

| Evidence | Algorithm | Normalization |
| --- | --- | --- |
| `header_crc32`, `entry_crc32`, `trailer_crc32` | CRC-32/ISO-HDLC, reflected polynomial `0xedb88320`, initial value `0xffff_ffff`, final xor `0xffff_ffff` | If the computed value is zero, encode `1` and require readers to apply the same normalization. |
| `entry_table_crc64`, `segment_payload_crc64` | CRC-64/ECMA-182, polynomial `0x42f0_e1eb_a9ea_3693`, initial value `0`, final xor `0` | If the computed value is zero, encode `1` and require readers to apply the same normalization. |
| SHA-256 fields | SHA-256 over the canonical byte ranges named by each field | Required hashes must not be all zero. |

Changing any checksum, hash, normalization, or canonical input rule is an
incompatible segment index format change unless a future extension explicitly
version-gates both old and new algorithms.

### Segment index header layout

The encoded `SegmentIndexHeaderV0` is fixed at 256 bytes.

| Offset | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 8 | `magic` | Must equal `SEGMENT_INDEX_MAGIC`. |
| 8 | 2 | `format_major` | Must equal `1` for this spec. |
| 10 | 2 | `format_minor` | Must equal `0` unless an accepted compatibility rule allows a lower required feature set. |
| 12 | 2 | `byte_order` | Must equal `0x0102`. |
| 14 | 2 | `header_len` | Must equal `256`. |
| 16 | 8 | `total_len` | Total segment index file length including trailer. |
| 24 | 4 | `header_crc32` | CRC-32 over the 256-byte header with bytes 24 through 27 zeroed. |
| 28 | 4 | `flags` | Only known bits are valid. |
| 32 | 8 | `database_id` | Must be nonzero and match the parent manifest. |
| 40 | 8 | `snapshot_id` | Must be nonzero and match the parent manifest and every entry. |
| 48 | 8 | `segment_index_id` | Must be nonzero and match the parent manifest root entry. |
| 56 | 8 | `manifest_version` | Must match the parent manifest. |
| 64 | 8 | `base_checkpoint_lsn` | Must match the parent manifest snapshot base. |
| 72 | 8 | `required_wal_start_lsn` | Must match the parent manifest recovery floor. |
| 80 | 8 | `entry_offset` | Must equal `256` in v0. |
| 88 | 8 | `entry_count` | Must be nonzero and bounded by `SEGMENT_INDEX_MAX_ENTRIES`. |
| 96 | 4 | `entry_len` | Must equal `160`. |
| 100 | 4 | `page_size_policy_tag` | `0` for mixed by entry, `1` for 16 KiB only, `2` for 32 KiB only. |
| 104 | 8 | `extension_offset` | Must follow the entry table exactly. |
| 112 | 8 | `extension_len` | May be zero. Must be bounded. |
| 120 | 8 | `first_segment_id` | Must equal the first entry `segment_id`. |
| 128 | 8 | `last_segment_id` | Must equal the last entry `segment_id`. |
| 136 | 8 | `first_page_id` | Must equal the lowest entry `first_page_id`. |
| 144 | 8 | `last_page_id` | Must equal the highest covered page id. |
| 152 | 8 | `min_page_lsn` | Must equal the minimum entry `min_page_lsn`. |
| 160 | 8 | `max_page_lsn` | Must equal the maximum entry `max_page_lsn`. |
| 168 | 8 | `total_page_count` | Sum of entry `page_count`; must be nonzero and must not overflow. |
| 176 | 32 | `parent_manifest_hash` | SHA-256 hash of the parent manifest file. |
| 208 | 32 | `entry_table_sha256` | SHA-256 hash over the encoded entry table. |
| 240 | 8 | `entry_table_crc64` | CRC-64 over the encoded entry table. |
| 248 | 8 | `reserved_248` | Must be zero. |

The header range fields summarize the entry table. Readers must recompute the
summary and reject mismatches.

### Segment index flag bits

| Bit | Name | Rule |
| ---: | --- | --- |
| `0x0000_0001` | `PUBLISHED_COLD_ONLY` | Must be set for indexes referenced by `DatabaseManifest v0`. |
| `0x0000_0002` | `CONTIGUOUS_PAGE_RANGES` | Must be set when entries exactly cover one page-id interval. |
| `0x0000_0004` | `ALLOW_OBJECT_SHARDS` | Reserved for future object-sharded indexes. Must be clear in v0. |
| `0x0000_0008` | `FORENSIC_HOLD` | Marks an index retained for forensic policy. It does not authorize mutation. |

Unknown flag bits must be rejected by normal startup, backup, restore, and
publication readers. `ForensicStart` may report unknown bits read-only with
replay disabled.

### Page size tags

| Tag | Page size | Rule |
| ---: | --- | --- |
| `0` | Mixed by entry | Header policy only; each entry still carries a page size tag. |
| `1` | 16 KiB | Must match `PageHeader_PageTrailer v0`. |
| `2` | 32 KiB | Must match `PageHeader_PageTrailer v0`. |

Unknown page size tags must be rejected.

### Segment state tags

| Tag | State | Rule |
| ---: | --- | --- |
| `1` | `BuildingHotSnapshot` | Must not appear in a manifest-referenced `SegmentIndex v0`. |
| `2` | `Sealed` | Must not appear in a manifest-referenced `SegmentIndex v0`. |
| `3` | `PublishedCold` | Required for every v0 entry referenced by a database manifest. |

The segment index records published cold truth. Staging or sealed HotStore
segments must not be accepted as cold snapshot truth through this format.

### Segment index entry layout

Each `SegmentIndexEntryV0` is fixed at 160 bytes.

| Offset in entry | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 8 | `segment_id` | Must be nonzero and strictly increasing. |
| 8 | 8 | `object_id` | Must be nonzero. |
| 16 | 8 | `allocation_id` | Must be nonzero. |
| 24 | 8 | `first_extent_id` | Must be nonzero. |
| 32 | 4 | `extent_count` | Must be nonzero. |
| 36 | 2 | `page_size_tag` | Must be `1` or `2`. |
| 38 | 2 | `segment_state_tag` | Must be `3` for manifest-referenced indexes. |
| 40 | 8 | `first_page_id` | Must be nonzero. |
| 48 | 4 | `page_count` | Must be nonzero. |
| 52 | 4 | `reserved_52` | Must be zero. |
| 56 | 8 | `min_page_lsn` | Must be nonzero. |
| 64 | 8 | `max_page_lsn` | Must be nonzero and greater than or equal to `min_page_lsn`. |
| 72 | 8 | `snapshot_id` | Must equal the header `snapshot_id`. |
| 80 | 8 | `segment_file_id` | Must be nonzero and stable within the snapshot. |
| 88 | 8 | `segment_file_offset` | Absolute offset of the segment artifact within its containing file, or zero when the segment is stored as a standalone file. |
| 96 | 8 | `segment_byte_len` | Must be nonzero and match the referenced segment artifact length. |
| 104 | 8 | `segment_payload_crc64` | CRC-64 over segment payload bytes; must be nonzero. |
| 112 | 32 | `segment_sha256` | SHA-256 over the referenced segment artifact bytes; must be nonzero. |
| 144 | 4 | `segment_header_crc32` | Must match the referenced segment header. |
| 148 | 4 | `segment_trailer_crc32` | Must match the referenced segment trailer. |
| 152 | 4 | `entry_flags` | Must be zero in v0. |
| 156 | 4 | `entry_crc32` | CRC-32 over this 160-byte entry with bytes 156 through 159 zeroed. |

Readers must reject entries with unknown flags, zero required fields, invalid
page size tags, invalid state tags, range overflows, checksum mismatches, or
hash mismatches.

### Segment artifact binding

The segment index entry is not a replacement for segment or page validation. It
is a recovery index that points to immutable segment artifacts and records the
evidence needed to find and validate them.

For each entry, readers must validate that:

- the referenced segment artifact exists at `segment_file_id` plus
  `segment_file_offset`, according to the storage container policy;
- the artifact byte length equals `segment_byte_len`;
- the artifact SHA-256 equals `segment_sha256`;
- the segment header identity matches `segment_id`, `object_id`,
  `allocation_id`, `first_page_id`, `page_count`, `min_page_lsn`, and
  `max_page_lsn`;
- the segment header CRC equals `segment_header_crc32`;
- the segment trailer CRC equals `segment_trailer_crc32`;
- the segment payload CRC equals `segment_payload_crc64`;
- page headers and trailers inside the segment validate according to
  `PageHeader_PageTrailer v0` before pages become recovery state.

When a segment is represented as a standalone file, `segment_file_id` still
must be nonzero and must identify the storage catalog entry for that file.
`segment_file_offset` must be zero.

### Ordering and range validation

The v0 entry table must be sorted by `segment_id` in strictly increasing order.
It must also be ordered by `first_page_id` unless a future compatible extension
sets an accepted object-shard policy.

For v0, readers must reject:

- duplicate `segment_id` values;
- descending `segment_id` values;
- duplicate or overlapping page ranges;
- page range arithmetic overflow;
- extent range arithmetic overflow;
- gaps when `CONTIGUOUS_PAGE_RANGES` is set;
- entries whose `snapshot_id` differs from the header;
- entries whose state is not `PublishedCold`;
- entries whose page size conflicts with a nonzero header
  `page_size_policy_tag`;
- aggregate summary mismatches against header range fields.

If `CONTIGUOUS_PAGE_RANGES` is clear, entries may be sparse, but they still
must not overlap. Sparse indexes must be accepted only when the parent manifest
and catalog root can prove that the missing ranges are intentionally absent.

### Extension bytes

`extension_len` may be zero. When nonzero, extension bytes must use a
type-length-value encoding:

```text
ExtensionRecord =
    kind_tag: u16 little-endian
    flags: u16 little-endian
    length: u32 little-endian
    payload: u8[length]
```

Unknown extension records with the required flag set must be rejected. Unknown
extension records without the required flag may be skipped only after their
length is checked against `SEGMENT_INDEX_MAX_EXTENSION_BYTES`.

Extensions are covered by trailer hash and trailer CRC. No extension may
override fixed header fields or entry fields.

### Segment index trailer layout

The encoded `SegmentIndexTrailerV0` is fixed at 96 bytes and is placed at
`total_len - 96`.

| Offset in trailer | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 8 | `entry_table_crc64_mirror` | Must equal header `entry_table_crc64`. |
| 8 | 32 | `segment_index_file_sha256` | SHA-256 over the full file with this field zeroed. |
| 40 | 32 | `segment_index_root_hash` | SHA-256 over parent manifest hash, segment index id, snapshot id, entry table hash, and file hash. |
| 72 | 4 | `trailer_crc32` | CRC-32 over this 96-byte trailer with bytes 72 through 75 zeroed. |
| 76 | 4 | `trailer_flags` | Must be zero in v0. |
| 80 | 16 | `reserved_80` | Must be zero. |

Readers must reject a zero `entry_table_crc64_mirror`, zero
`segment_index_file_sha256`, zero `segment_index_root_hash`, or zero
`trailer_crc32`.

### Decode-before-allocate

Segment index decoders must avoid unbounded allocation from untrusted length
fields.

Required order:

1. Verify that at least `SEGMENT_INDEX_HEADER_LEN + SEGMENT_INDEX_TRAILER_LEN`
   bytes are available.
2. Decode the fixed 256-byte header.
3. Validate magic, version, byte-order marker, fixed header length, flags,
   identity fields, entry offset, entry length, entry count bound, extension
   bound, range summaries, and header CRC.
4. Use checked arithmetic to compute the entry table, extension, and trailer
   ranges.
5. Require `total_len` to equal the input length when decoding a standalone
   segment index file.
6. Decode entries without allocating more than `SEGMENT_INDEX_MAX_ENTRIES`.
7. Validate per-entry CRCs, ordering, uniqueness, range arithmetic, page size
   policy, state tags, snapshot identity, LSN bounds, and nonzero digests.
8. Recompute `entry_table_crc64`, `entry_table_sha256`, trailer CRC,
   `segment_index_file_sha256`, and `segment_index_root_hash`.
9. Validate against the parent `DatabaseManifest v0` root entry by
   `segment_index_id`, `snapshot_id`, byte length, page range, and SHA-256.
10. Validate referenced segment artifacts and page trailers before exposing
    pages as recovered state.

No caller may expose a decoded segment index as accepted recovery state before
all required checks pass.

### Publication behavior

Segment index publication is part of cold snapshot publication. It must never
modify an already published cold segment in place.

The required order is:

1. Build segment artifacts under staging paths.
2. Validate each segment header, trailer, payload checksum, page range, extent
   range, page LSN bounds, and page trailer.
3. Build the segment index entry table from validated segment descriptors.
4. Write and fsync the segment index artifact.
5. Write and fsync the parent `DatabaseManifest v0`.
6. Append and durably flush the WAL `ManifestSwitch` record, or an equivalent
   durable manifest switch record accepted by the WAL specification.
7. Atomically switch the active manifest root pointer.
8. Defer cleanup until recovery, backup, replica, and forensic retention
   policies allow it.

A crash must leave either the old manifest and old segment index active, or
the new manifest and new segment index active. It must not leave a partial
segment index as the active recovery index.

### Recovery behavior

Normal startup and `SafeStart` must:

- validate the parent `DatabaseManifest v0` first;
- validate the manifest's segment index root entry by byte length and SHA-256;
- decode and validate the full `SegmentIndex v0` before WAL redo;
- validate storage format fingerprints before interpreting segment or page
  payloads;
- verify that every entry is `PublishedCold` and references the manifest
  snapshot id;
- validate segment artifact hashes and page trailers before exposing pages as
  recovered state;
- require durable WAL coverage from the manifest recovery floor before making
  recovered state visible.

If a segment index is missing, corrupt, stale, or incompatible, normal startup
must not use it as truth. Startup may try a previous manifest and segment index
only when the manifest chain, retention policy, and operator mode allow it.
Otherwise startup must refuse or enter read-only `ForensicStart` according to
policy.

`ForensicStart` may inspect unknown or corrupt segment indexes read-only. It
must not append WAL, rewrite segment indexes, repair segments in place, publish
a manifest, perform redo, perform undo, rebuild indexes as accepted state, or
allow Application-surface traffic.

### Compatibility rules

The v0 reader accepts only:

- `format_major == 1`;
- `format_minor == 0`;
- `byte_order == 0x0102`;
- fixed header, entry, and trailer lengths from this spec;
- known required flag bits;
- known page size tags;
- `PublishedCold` segment state for manifest-referenced indexes;
- zero reserved fields.

A future minor version may be backward-compatible only if:

- `format_major` remains `1`;
- every new fixed field is appended through extension records or reserved
  fields that v0 readers already require to be zero;
- v0 readers can skip non-required extension records after validating length;
- hashes, CRCs, page-range semantics, LSN semantics, and parent manifest
  binding remain canonical.

Any major version change, fixed-offset reinterpretation, endian change, entry
length change, checksum algorithm change, hash input change, segment state
semantic change, or page-range semantic change is incompatible and requires a
new segment index format spec plus migration, dual-read, or reject policy.

Pre-persistence fixtures without this format must be rejected by release
readers unless an explicit migration reader is implemented and crash-tested.

## Validation

Documentation acceptance checks:

- The spec defines explicit little-endian fields and fixed offsets.
- The spec states that Rust native struct serialization is forbidden.
- The spec preserves ColdStore immutability after publication.
- The spec requires manifest binding before segment index acceptance.
- The spec requires storage format validation before WAL redo.
- The spec requires segment hashes, page trailers, and LSN bounds before
  recovered pages become visible.
- The spec defines checksum, hash, compatibility, recovery, fuzz, and crash
  gates.
- The spec does not introduce SQL, gRPC, runtime JSON defaults, GPU critical
  path behavior, or Application-surface administration.

Future implementation work should add or keep owner tests for:

```powershell
cargo test -p andromeda-storage segment
cargo test -p andromeda-storage --test core_io_gates
cargo test -p andromeda-storage --test layout_publication_contract
cargo test -p andromeda-storage --test storage_hotcold_pipeline_e2e
cargo test -p andromeda-storage --test wal_scan_recovery_contract
```

Future byte-codec work must add:

```powershell
cargo test -p andromeda-storage --test segment_index_golden_vectors
cargo test -p andromeda-storage --test segment_index_corruption_contract
cargo check --manifest-path fuzz/Cargo.toml --bin segment_index_decode --locked
```

Sustained fuzz evidence for `segment_index_decode` must include empty input,
truncated headers, valid empty-prohibited indexes, valid single-entry indexes,
valid multi-entry indexes, bad versions, unsupported flags, length overflow,
descending segment ids, overlapping page ranges, extent overflow, bad entry
CRC, bad table hash, bad trailer, unknown page size tags, non-published states,
snapshot mismatch, and corrupt segment hash evidence.

### Validation matrix

| Area | Positive case | Negative case | Required outcome |
| --- | --- | --- | --- |
| Magic | `ANDSGIX0` | Any other bytes | Reject before entry decode. |
| Version | Major 1, minor 0 | Unknown major or unsupported minor | Reject normal startup. |
| Byte order | `0x0102` | Any other marker | Reject. |
| Header length | 256 | Any other value | Reject. |
| Entry length | 160 | Any other value | Reject. |
| Trailer length | 96 by `total_len` placement | Truncated or extra bytes | Reject. |
| Parent binding | Manifest root byte length and SHA-256 match | Manifest points elsewhere or hash mismatch | Reject. |
| Entry identity | Nonzero segment, object, allocation, extent, page ids | Any required zero identity | Reject. |
| State | `PublishedCold` | `BuildingHotSnapshot`, `Sealed`, unknown | Reject for manifest-referenced index. |
| Page ranges | Strictly ordered and non-overlapping | Duplicate, overlap, descending, overflow | Reject. |
| LSN bounds | Nonzero `min_page_lsn <= max_page_lsn` | Zero or inverted bounds | Reject. |
| Page size | Known tag and policy match | Unknown tag or policy mismatch | Reject. |
| Entry CRC | Recomputed CRC matches | Entry bit flip | Reject. |
| Table hashes | Recomputed table CRC/hash match | Table corruption | Reject. |
| Segment hash | Segment artifact SHA-256 matches | Segment bytes changed | Reject normal recovery. |
| Page trailer | Page payload integrity validates | Torn or corrupt page | Reject page; recover only from valid WAL/snapshot evidence. |
| Recovery | Manifest plus index plus segments plus WAL coverage valid | Missing index, unknown format, or WAL gap | Reject normal replay; forensic read-only only. |

### Crash validation gates

Segment index publication is not release-ready until deterministic crash tests
cover:

| Crash point | Required result |
| --- | --- |
| Before any segment artifact fsync | Old manifest and old segment index remain active. |
| After segment artifact fsync, before segment index fsync | Old manifest remains active; staged segments are ignored. |
| During segment index write | Partial segment index is rejected by length, CRC, or hash. |
| After segment index fsync, before manifest fsync | Old manifest remains active; new index is unreachable. |
| After manifest fsync, before durable `ManifestSwitch` WAL | Old manifest remains active unless explicit recovery policy proves otherwise. |
| After durable `ManifestSwitch` WAL, before root pointer switch | Recovery may complete the switch only if manifest and segment index validate. |
| During root pointer switch | Old or new manifest/index pair is selected; never a mixed pair. |
| Segment file corrupted after publication | Segment index detects hash or CRC mismatch; recovery uses WAL, previous snapshot, restore, or forensic mode. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Decoder accepts a segment index by transmuting `SegmentDescriptor`. | Native layout was treated as durable layout. | Replace with explicit offset-based little-endian codec. |
| Startup trusts a segment index not referenced by the manifest. | Parent manifest binding was skipped. | Validate byte length, SHA-256, snapshot id, and segment index id against `DatabaseManifest v0`. |
| Published cold index contains `Sealed` entries. | Staging state leaked into the published artifact. | Reject the index and rebuild publication from validated `PublishedCold` descriptors. |
| Page ranges overlap. | Entry sort or extent/page range validation is missing. | Reject the index; do not choose a winner entry. |
| Header summary differs from entry table. | Header range fields drifted or entry table changed. | Recompute and reject on mismatch. |
| Segment SHA-256 mismatch. | Segment artifact changed after publication or the index points to the wrong file. | Reject normal startup and use a valid prior manifest, restore, or forensic inspection. |
| ColdStore mutation is accepted after publication. | Publication boundary was bypassed. | Reject mutation and preserve ColdStore immutability. |
| `ForensicStart` repairs a segment index in place. | Forensic mode mutated inspected truth. | Reject behavior; forensic inspection must be read-only. |

## References

- `AGENTS.md`
- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `documentations/governance/decisions/DEC-032-storage-format-gate.md`
- `documentations/specs/DatabaseManifest_v0.md`
- `documentations/specs/PageHeader_PageTrailer_v0.md`
- `documentations/specs/WalRecord_v0.md`
- `documentations/specs/RecoveryReport_v0.md`
- `fuzz/ROADMAP_FUZZ_GAPS_2026.md`
- `fuzz/VALIDATION_MATRIX.md`
- `crates/andromeda-storage/src/segment.rs`
- `crates/andromeda-storage/src/extent/segment_contiguity.rs`
- `crates/andromeda-storage/src/cold_store.rs`
- `crates/andromeda-storage/src/manifest/cold_publication.rs`
- `crates/andromeda-storage/tests/core_io_gates.rs`
- `crates/andromeda-storage/tests/layout_publication_contract.rs`
- `crates/andromeda-storage/tests/storage_hotcold_pipeline_e2e.rs`
