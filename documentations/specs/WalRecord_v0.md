# WalRecord v0 Specification

## Purpose

Define the accepted documentation contract for Andromeda WAL record bytes,
single-file WAL header bytes, scan behavior, and recovery-facing validation.

This specification records the current explicit codec evidence for the
`andromeda-wal` frame format. It does not create a new WAL format, does not
promote unimplemented segment chaining, and does not authorize visible mutation
before durable WAL.

## Scope

This specification applies to the current `WalRecord` frame codec and the
current mono-segment file WAL header used by the Rust workspace.

It covers:

- canonical little-endian WAL integer encoding;
- fixed WAL frame header fields and offsets;
- WAL record kind tags;
- record checksums and header checksums;
- LSN, `previous_lsn`, and mono-segment file linkage;
- decode-before-allocate and bounded payload policy;
- corruption and truncation handling;
- recovery validation matrix;
- open durable-format fields that remain pending.

## Non-goals

This specification does not:

- introduce ad hoc SQL, dynamic command text, or untyped Procedure bypasses;
- serialize Rust native structs directly to disk;
- define a network protocol or gRPC surface;
- define catalog, heap, B-Tree, audit, or security payload schemas beyond the
  WAL frame envelope;
- claim a multi-segment WAL archive format beyond the current mono-segment file
  header evidence;
- claim a separate persisted `ChainHash` field, because the current frame
  codec validates chain continuity through `previous_lsn`, LSN ordering, and
  checksums;
- change the WAL-before-visible-commit invariant.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- `documentations/ROADMAP_IMPLEMENTATION_2026.md` for the durable vertical path
  and WAL-before-visible-commit priorities.
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md` for conceptual
  WAL and recovery intent.
- `crates/andromeda-wal/src/wal_codec.rs` for byte-format constants.
- `crates/andromeda-wal/src/wal_codec/frame.rs` for frame header decode and
  validation.
- `crates/andromeda-wal/src/wal_codec/record.rs` for frame encode/decode.
- `crates/andromeda-wal/src/wal_codec/checksum.rs` for header checksum input.
- `crates/andromeda-wal/src/wal_codec/scan.rs` for scan stop behavior.
- `crates/andromeda-wal/src/write_ahead_log/record.rs` for `WalRecordKind`,
  `WalRecordHeader`, and record checksum rules.
- `crates/andromeda-wal/src/file_wal/header.rs` and
  `crates/andromeda-wal/src/file_wal/format.rs` for current file WAL header
  evidence.
- `crates/andromeda-wal/tests/wal_codec_contract.rs` and
  `crates/andromeda-wal/tests/file_wal_contract.rs` for executable contract
  coverage.

## Procedure

### Ownership

`andromeda-wal` owns the canonical WAL frame codec, file WAL header codec, WAL
scan engine, and public WAL byte-format constants.

Other crates may consume the WAL domain model and encoded bytes. They must not
redefine the WAL frame header or rely on Rust struct memory layout as a durable
format.

### Encoding policy

All multi-byte integer fields in the current WAL frame and file WAL header use
little-endian encoding through explicit codecs.

The Rust `WalRecord`, `WalRecordHeader`, and `WalFrameHeader` structs are domain
models. They are not the disk layout. Encoding and decoding must go through the
explicit codec functions.

The current WAL format constants are:

| Constant | Value | Rule |
| --- | ---: | --- |
| `WAL_FORMAT_VERSION` | `1` | Any incompatible frame change requires a version bump. |
| `WAL_BYTE_ORDER_LITTLE_ENDIAN` | `0x0102` | File WAL header must carry this marker. |
| `WAL_RECORD_MAGIC` | `0x414e44524f57414c` | Stored as little-endian bytes in the frame. |
| `WAL_RECORD_HEADER_LEN` | `72` | Fixed frame header length. |
| `WAL_RECORD_SIZE_LIMIT` | `1 MiB` | Maximum payload size before append. |
| `WAL_SEGMENT_BOUNDARY` | `4 MiB` | Current cumulative batch boundary for segment accounting. |
| `WAL_BATCH_ROW_LIMIT` | `256` | Row-operation count limit per transaction batch. |

### WAL frame layout

The encoded WAL frame header is fixed at 72 bytes. The payload begins
immediately after byte 71 and has the exact `payload_length` declared in the
header.

| Offset | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 8 | `magic` | Must equal `WAL_RECORD_MAGIC`. |
| 8 | 2 | `format_version` | Must equal `1`. |
| 10 | 2 | `header_length` | Must equal `72`. |
| 12 | 8 | `total_length` | Must be `header_length + payload_length`. |
| 20 | 2 | `kind_tag` | Must map to a known `WalRecordKind`. |
| 22 | 2 | `flags` | Only bit `0x0001` and bit `0x0002` are valid. |
| 24 | 8 | `lsn` | Must be nonzero at record validation. |
| 32 | 8 | `previous_lsn_value` | Zero when `flags & 0x0001 == 0`; otherwise the previous LSN. |
| 40 | 8 | `transaction_id_value` | Zero when `flags & 0x0002 == 0`; otherwise the transaction id. |
| 48 | 8 | `payload_length` | Must match the payload byte count. |
| 56 | 8 | `record_checksum` | Nonzero FNV-1a 64-bit checksum over record semantics and payload. |
| 64 | 8 | `header_checksum` | Nonzero FNV-1a 64-bit checksum over header fields excluding this field. |

Flag meanings are:

| Bit | Name | Rule |
| ---: | --- | --- |
| `0x0001` | `HAS_PREVIOUS_LSN` | If clear, bytes 32 through 39 must be zero. If set, the decoded record carries `previous_lsn`. |
| `0x0002` | `HAS_TRANSACTION_ID` | If clear, bytes 40 through 47 must be zero. If set, the decoded record carries `transaction_id`. |

Unknown flag bits must be rejected.

### Record kind tags

The current frame uses `u16`-sized tags on disk. Tags must map to the
`WalRecordKind` table below.

| Tag | Record kind |
| ---: | --- |
| 1 | `TxBegin` |
| 2 | `TxCommit` |
| 3 | `TxRollback` |
| 4 | `PageAllocate` |
| 5 | `PageFormat` |
| 6 | `RowInsert` |
| 7 | `RowUpdate` |
| 8 | `RowDelete` |
| 9 | `IndexInsert` |
| 10 | `IndexDelete` |
| 11 | `MvccVersionCreate` |
| 12 | `MvccVersionClose` |
| 13 | `MapDeltaAppend` |
| 14 | `CheckpointBegin` |
| 15 | `CheckpointEnd` |
| 16 | `SnapshotBegin` |
| 17 | `SnapshotEnd` |
| 18 | `ManifestSwitch` |
| 19 | `CatalogChangeBegin` |
| 20 | `CatalogChangeApply` |
| 21 | `CatalogChangeCommit` |
| 22 | `SecurityAuditAppend` |
| 23 | `BTreeInsert` |
| 24 | `BTreeDelete` |
| 25 | `BTreeSplit` |
| 26 | `BTreeMerge` |

New tags require a format compatibility review and tests for encode, decode,
scan, recovery routing, and unknown-tag rejection.

### Record checksum

`record_checksum` is a nonzero FNV-1a 64-bit checksum over:

1. record kind tag as a `u64`;
2. `lsn`;
3. `previous_lsn`, or zero when absent;
4. `transaction_id`, or zero when absent;
5. payload length as a `u64`;
6. payload bytes.

All scalar values are folded in little-endian order. A computed zero checksum is
normalized to `1`.

The checksum is integrity evidence. It is not a cryptographic authenticity
mechanism and must not replace manifest hashes, storage format fingerprints,
audit evidence, or operator forensic reports.

### Header checksum

`header_checksum` is a nonzero FNV-1a 64-bit checksum over the encoded header
fields from offset `0` through offset `63`, excluding the `header_checksum`
field itself. The input includes `record_checksum`.

Decoders must reject frames where recomputed header checksum evidence differs
from the encoded value.

### LSN and chain policy

LSN zero is invalid for a persisted record. The first record in a standalone
mono-segment WAL starts at LSN `1` with no `previous_lsn`.

The scan engine validates an exact chain:

1. The first expected LSN is supplied by the caller. Current full-file scan uses
   LSN `1`.
2. Each record's LSN must equal the expected LSN.
3. Each record's `previous_lsn` must equal the previous accepted record LSN, or
   the supplied base previous LSN.
4. The next expected LSN is `lsn + 1`; overflow stops future advancement.

This chain policy provides current segment linkage evidence. A separate
persisted `ChainHash` field remains pending and must not be documented as
implemented until code adds it.

### File WAL header layout

The current file WAL header is fixed at 80 bytes. WAL frame bytes start at file
offset `80`.

The current file WAL constants are:

| Constant | Value | Rule |
| --- | ---: | --- |
| `FILE_WAL_MAGIC` | `0x314c415752444e41` | Required little-endian file WAL magic. |
| `FILE_WAL_HEADER_LEN` | `80` | Fixed file WAL header length. |
| `FILE_WAL_MONO_SEGMENT_ID` | `1` | Current single-segment file identity. |

| Offset | Size | Field | Rule |
| ---: | ---: | --- | --- |
| 0 | 8 | `magic` | Must equal `FILE_WAL_MAGIC`. |
| 8 | 2 | `format_version` | Must equal `WAL_FORMAT_VERSION`. |
| 10 | 2 | `byte_order` | Must equal `0x0102`. |
| 12 | 4 | `header_length` | Must equal `80`. |
| 16 | 8 | `segment_id` | Current mono-segment value must equal `1`. |
| 24 | 8 | `first_lsn` | Current mono-segment value must equal `1`. |
| 32 | 8 | `base_previous_lsn` | Current mono-segment value must be zero. |
| 40 | 8 | `durable_lsn` | Last durable LSN, or zero for an empty WAL. |
| 48 | 8 | `durable_bytes` | Durable frame byte prefix after the file header. |
| 56 | 8 | `durable_record_count` | Number of durable records in the durable prefix. |
| 64 | 8 | `header_checksum` | Nonzero FNV-1a 64-bit checksum over the header excluding this field. |
| 72 | 8 | `reserved` | Must be zero. |

The current file WAL is explicitly mono-segment. Multi-segment archive identity,
cross-segment `base_previous_lsn`, and segment hash chaining are open fields for
future HA/DR and PITR work.

### Decode-before-allocate

WAL decoders and scanners must avoid unbounded allocation from untrusted length
fields.

Required order:

1. Verify at least 72 bytes are available before reading a frame header.
2. Decode and validate `magic`, `format_version`, `header_length`,
   `total_length`, `payload_length`, known kind tag, known flags, flag-field
   consistency, and header checksum.
3. Convert `total_length` to `usize` only after the header is structurally
   valid.
4. Verify the input contains the complete frame before allocating the payload
   vector.
5. Enforce append-time record payload bounds before writing records to a WAL.

Implementations that ingest external WAL bytes must keep the durable-prefix
scan model: inspect a valid prefix, stop at corruption or truncation, and never
trust trailing bytes as durable truth.

### Corruption handling

Frame decode is fail-closed. A single-frame decode rejects:

- truncated headers;
- invalid magic;
- unsupported format version;
- header length mismatch;
- total length smaller than header length;
- payload length mismatch;
- unknown kind tag;
- unknown flags;
- unflagged nonzero `previous_lsn` bytes;
- unflagged nonzero transaction id bytes;
- header checksum mismatch;
- truncated payload;
- record checksum mismatch;
- missing required transaction id for record kinds that require one;
- nonzero payload length mismatch.

Sequential scans return the last valid durable prefix and a stop reason:

| Stop reason | Meaning | Recovery use |
| --- | --- | --- |
| `TruncatedHeader` | Remaining bytes cannot contain a full header. | Keep prior valid prefix; treat tail as not durable. |
| `TruncatedRecord` | Header is present but full frame bytes are missing. | Keep prior valid prefix; treat tail as not durable. |
| `CorruptHeader` | Header validation failed. | Stop before the corrupt frame and preserve forensic evidence. |
| `CorruptRecord` | Header passed, but record decode or checksum failed. | Stop before the corrupt frame and preserve forensic evidence. |
| `LsnGap` | LSN skipped the expected value. | Reject normal open; forensic inspection may report the valid prefix. |
| `DuplicateOrReorderedLsn` | LSN repeated or moved backward. | Reject normal open; forensic inspection may report the valid prefix. |
| `PreviousLsnMismatch` | `previous_lsn` does not match the accepted chain. | Reject normal open; forensic inspection may report the valid prefix. |

Recovery must not replay records after the first invalid frame or chain break.

### Recovery relationship

WAL records are necessary but not sufficient for database visibility. Recovery
must also validate storage format fingerprints, manifests, transaction terminal
state, and payload-specific compatibility before replay mutates state.

Current recovery-facing rules:

- durable WAL precedes visible commit;
- duplicate, reordered, gap, or broken-previous-LSN sequences fail before
  visibility;
- conflicting terminal transaction records fail recovery closed;
- access-path mutation records are redo-relevant but require rebuild or
  quarantine gates until the durable B-Tree replay policy is promoted;
- unknown storage subformats must be rejected before WAL redo, except
  read-only forensic startup where replay is disabled.

## Validation

Documentation acceptance checks:

- The spec states explicit little-endian codecs and rejects native Rust struct
  layout as a durable format.
- The spec names the 72-byte frame header and 80-byte file WAL header only
  because code evidence supports those lengths.
- The spec marks a separate persisted `ChainHash` and multi-segment chain hash
  as pending.
- The spec preserves WAL-before-visible-commit.
- The spec does not introduce SQL, gRPC, runtime JSON, or Procedure bypasses.

Existing code evidence to use when code validation is allowed:

```powershell
cargo test -p andromeda-wal --test wal_codec_contract
cargo test -p andromeda-wal --test file_wal_contract
cargo test -p andromeda-storage --test wal_record_bounds_contract
cargo test -p andromeda-storage --test wal_scan_recovery_contract
cargo test -p andromeda-storage --test recovery_completeness_contract
```

### Validation matrix

| Area | Positive case | Negative case | Required outcome |
| --- | --- | --- | --- |
| Frame magic | `WAL_RECORD_MAGIC` | Any other value | Reject before payload decode. |
| Version | `1` | Unknown version | Reject as unsupported WAL format. |
| Header length | `72` | Smaller, larger, or inconsistent | Reject before payload allocation. |
| Total length | `72 + payload_length` | Header/payload mismatch or overflow | Reject. |
| Flags | Known bits only | Unknown bit or unflagged nonzero optional field | Reject. |
| Kind tag | 1 through 26 | Unknown tag | Reject. |
| Payload checksum | Recomputed value matches | Payload bit flip or checksum bit flip | Reject. |
| Header checksum | Recomputed value matches | Header bit flip | Reject. |
| LSN continuity | Exact sequential LSNs | gap, duplicate, reorder | Stop scan and reject normal replay. |
| Previous LSN | Matches predecessor | mismatch | Stop scan and reject normal replay. |
| File durable prefix | Header durable bytes match valid prefix | truncated physical tail | Reopen truncates or scans only durable prefix. |
| Segment accounting | Batch at or below 4 MiB | Batch over 4 MiB | Reject batch append. |
| Record size | Payload at or below 1 MiB | Payload above 1 MiB | Reject append. |
| Transaction rows | 256 row operations | 257 row operations | Reject batch cardinality. |
| Recovery format gate | Known storage fingerprints | unknown or unsupported format | Reject before redo, or forensic read-only with replay disabled. |

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| WAL frame magic mismatch | Bytes are not an Andromeda WAL frame or endian handling is wrong. | Reject the frame and inspect the containing file header. |
| Header checksum mismatch | Header bytes changed after encoding or checksum input drifted. | Recompute using the explicit header checksum algorithm and verify offsets. |
| Record checksum mismatch | Payload or semantic header fields changed. | Stop at the corrupt frame and keep only the prior valid prefix. |
| Truncated record after a valid prefix | Crash or incomplete write after durable header update. | Reopen using the scanned valid prefix and preserve recovery evidence. |
| Previous LSN mismatch | Missing, reordered, or spliced record sequence. | Reject normal open; use forensic reporting if required. |
| Record kind requires a transaction id | A transactional mutation was encoded without `transaction_id`. | Reject the record and fix the producer. |
| Documentation mentions `ChainHash` as implemented | Conceptual field drifted ahead of code. | Mark it pending until the codec adds a versioned field. |
| Multi-segment WAL text claims archive support | Current file WAL header is mono-segment. | Reword as future HA/DR/PITR work or cite implementing code evidence. |

## References

- `documentations/ROADMAP_IMPLEMENTATION_2026.md`
- `documentations/03_TRANSACTION_WAL_MVCC_STORAGE_RECOVERY.md`
- `documentations/05_OPTIMIZER_STATS_ANALYTICS_HARDWARE_ROADMAP_SOURCES.md`
- `crates/andromeda-wal/src/wal_codec.rs`
- `crates/andromeda-wal/src/wal_codec/binary.rs`
- `crates/andromeda-wal/src/wal_codec/checksum.rs`
- `crates/andromeda-wal/src/wal_codec/frame.rs`
- `crates/andromeda-wal/src/wal_codec/record.rs`
- `crates/andromeda-wal/src/wal_codec/scan.rs`
- `crates/andromeda-wal/src/write_ahead_log/record.rs`
- `crates/andromeda-wal/src/write_ahead_log/record_bounds.rs`
- `crates/andromeda-wal/src/file_wal/header.rs`
- `crates/andromeda-wal/src/file_wal/format.rs`
- `crates/andromeda-wal/tests/wal_codec_contract.rs`
- `crates/andromeda-wal/tests/file_wal_contract.rs`
- `crates/andromeda-storage/tests/recovery_contract/format_gate.rs`
