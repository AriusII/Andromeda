# DEC-032: Storage Format Gate for V1 B-Tree Keys and Heap Pages

**Status:** ACCEPTED  
**Date:** Q1 2026  
**Task:** V1 Storage Format Gate  
**Author:** Storage, WAL/Recovery, Binary Format, and Risk Governance Agents  
**Stakeholders:** Storage Engine, WAL/Recovery, Backup/Restore, Catalog, Transaction Kernel, Release Governance

---

## Decision

Andromeda treats the current B-Tree key encoding and heap tuple page layout as a **pre-persistence V1 candidate format**, not as an unconditionally promoted durable format.

The current format can become the initial V1 durable storage format only after the following gates are complete:

1. The B-Tree key codec is locked with an explicit key-format identity, golden byte vectors, fixed-arity composite rules, and rebuild/reject behavior for indexes without matching metadata.
2. The heap page layout is locked with one canonical slot-directory metadata source, one compaction doctrine, explicit page/header/trailer validation, and golden page image vectors.
3. Recovery validates storage format identity before redo. Unknown or unsupported heap/index/page/WAL payload formats must fail fast or open only in `ForensicStart` without replay.
4. Cold snapshot, backup, restore, and manifest paths must reject unsupported page/index formats before publication or restore.

Until these gates are implemented and validated, storage artifacts produced by the current B-Tree and heap code are **prototype/non-release durable artifacts**. They must not be used as a compatibility promise for V1 release readiness.

---

## Context

first hygiene milestone hygiene and remediation found that the working tree contains correctness-motivated changes in storage byte interpretation:

- `btree_key_codec.rs` encodes text keys as self-delimiting escaped UTF-8 bytes instead of the older inferred length-prefixed shape.
- Composite keys encode a column count followed by encoded column keys.
- Heap tuple payloads grow upward from the 96-byte header, while slot metadata is serialized near the trailer.
- `HeapPage::from_image()` and `SlotDirectory::from_page_data()` use incompatible slot-count locations.

These changes may be correct for ordering and page safety, but they affect durable byte interpretation. The project must not silently treat them as stable `V1_0` storage compatibility without explicit gates.

---

## Current candidate formats

### B-Tree KeyV1 candidate

Current candidate key tags:

| Logical key type | Tag |
|---|---:|
| `Null` | `0x00` |
| `Int32` | `0x01` |
| `Int64` | `0x02` |
| `Text` | `0x03` |
| `Bytes` | `0x04` |
| `Composite` | `0x05` |
| Composite-only `Bool` datum | `0x06` |

Candidate scalar encodings:

```text
Null   = 00 00 00
Int32  = 01 04 00 <big-endian i32 with sign bit flipped>
Int64  = 02 08 00 <big-endian i64 with sign bit flipped>
Text   = 03 <UTF-8 bytes, with 00 encoded as 00 ff> 00
Bytes  = 04 <u16 length LE> <payload bytes>
```

Candidate composite encoding:

```text
Composite = 05 <u16 column_count LE> <encoded_column_0> ... <encoded_column_n>
```

Mandatory restrictions:

- Composite keys are valid only for fixed-arity index schemas.
- Prefix and range-bound keys require a separate bound encoding. They must not be represented as ordinary shorter composite keys.
- `decode_composite` must validate schema arity and type compatibility.
- Unsupported datum variants must be rejected for durable index keys. They must not be encoded through debug-string fallback.
- `Bytes` cannot be used as an ordered index key until byte-array ordering is explicitly defined or the encoding is changed to preserve the intended order.
- Each persisted index must record the key-format identity used to build it.
- Mixed key-format versions inside one B-Tree are invalid.
- Indexes without recognized key-format metadata must be rebuilt or rejected.

### HeapPageV1 candidate

The candidate heap layout is:

```text
page[0..96]                  = PageHeaderV1
page[96..free_offset]        = tuple payloads, growing upward
page[free_offset..slot_base) = free space
page[slot_base..meta_offset) = slot entries, 5 bytes each, growing downward
page[meta_offset..meta+4]    = heap slot metadata
page[page_size-48..end]      = PageTrailerV1
```

Where:

```text
page_size       = 16384 or 32768
header_size     = 96
trailer_size    = 48
slot_entry_size = 5
meta_offset     = page_size - trailer_size - 4
slot_base       = meta_offset - slot_count * 5
```

Heap slot metadata:

```text
[slot_count: u16 LE][free_offset: u16 LE]
```

Slot entry:

```text
[offset: u16 LE][length: u16 LE][flags: u8]
```

Candidate deletion behavior:

```text
flags bit 0 set
offset set to 0
length retained
payload bytes stale until compaction or rewrite
```

Mandatory restrictions:

- The project must choose one authoritative slot-count and free-space source before durable promotion.
- `HeapPage::from_image()` must not keep an undocumented alternate slot-count source at byte offset `40`.
- If both header fields and footer heap metadata exist, readers must validate they match.
- Durable heap serialization must produce a validated `PageHeader`, `PageTrailer`, and `PageLayoutContract`.
- Unknown or mismatched heap page layouts must be rejected before redo.
- Slot reuse must be reviewed with transaction visibility, RowId stability, and WAL replay semantics before durable use.

---

## Alternatives considered

### Option A: Revert to the prior inferred format

Rejected as the default path. The prior inferred text key format appears length-prefixed, which can break bytewise text ordering. A blind rollback may preserve older bytes while retaining correctness defects. It also does not resolve the conflicting heap layout readers.

### Option B: Version and migrate every older artifact

Deferred. This is required if non-rebuildable release-bearing storage artifacts exist, but it is larger than the current V1 pre-persistence gate. It requires old readers, migration records, crash-safe migration, and recovery evidence.

### Option C: Rebuild or reject older artifacts

Accepted as the default fallback for pre-release artifacts. Indexes without recognized key-format metadata are derived structures and must be rebuilt or rejected. Heap pages without recognized layout identity must be rejected unless an old-layout migration reader is explicitly implemented.

### Option D: Accept the current candidate as V1 pre-persistence format

Accepted conditionally. The current candidate can be locked as the initial V1 durable format only if release governance confirms that no external, non-rebuildable, release-bearing storage artifacts rely on an older layout.

---

## Consequences

### Positive

- Prevents silent durable format drift.
- Allows correctness-motivated storage fixes before the V1 durable lock.
- Requires explicit tests for key byte ordering, heap page contiguity, and recovery compatibility.
- Clarifies that B-Tree indexes are rebuildable unless their key-format metadata is supported.
- Forces recovery to validate storage formats before replay.

### Negative

- Blocks V1 storage promotion until additional format gates are implemented.
- Requires new golden vectors and negative compatibility tests.
- May require manifest and page metadata extensions.
- May require old artifact rejection or migration decisions if release-bearing artifacts exist.

---

## Validation gates

### B-Tree key gates

Required tests:

1. Golden bytes for `Null`, `Int32`, `Int64`, `Text`, `Bytes`, and fixed-arity `Composite`.
2. Text terminator tests for empty text, prefix cases, embedded `0x00`, invalid UTF-8, missing terminator, and trailing bytes.
3. Ordering property tests proving encoded order equals logical order for supported ordered key types.
4. Composite schema validation for arity, type mismatch, nested composites, and unsupported datum variants.
5. Mixed-version and missing-version rejection tests for persisted indexes.
6. Index rebuild tests for incompatible or missing key-format metadata.

### Heap page gates

Required tests:

1. Golden layout tests for 16 KiB and 32 KiB pages.
2. Multi-tuple placement tests proving payloads grow upward from offset `96`.
3. Slot directory placement tests proving metadata starts at `page_size - 52`.
4. Deserialize/serialize round-trip through one canonical heap reader and writer.
5. Delete and compaction tests for the chosen compaction doctrine.
6. Header/trailer validation tests.
7. Legacy rejection tests for pages that only expose slot count at header offset `40`.
8. Fuzz/property tests for insert, delete, read, compact, malformed slot entries, and out-of-bounds offsets.

### Recovery gates

Required tests:

1. Startup rejects unknown heap layout before redo.
2. Startup rejects unknown B-Tree key/node format before index use.
3. `FastStart` and normal `SafeStart` require known compatible storage formats.
4. `ForensicStart` can open unknown/drifted storage artifacts read-only with replay disabled and a forensic report.
5. WAL payloads that mutate heap or B-Tree state must bind to compatible storage subformats or be rejected before mutation.
6. Backup/restore manifests must reject unsupported storage format fingerprints before publication or restore.

---

## Rollback and migration policy

If release-bearing artifacts are found in an older format, this decision does not authorize silent acceptance. The project must choose one of these follow-up paths:

1. Add old-format readers and a crash-safe migration path.
2. Rebuild derived indexes and reject incompatible heap pages.
3. Reject the artifact at startup and require forensic/operator intervention.

Any migration path must preserve WAL-before-visible-commit, WAL-before-page-flush, page checksums, RowId semantics, and auditability.

---

## Open questions

1. Has any prior heap page, B-Tree index, cold snapshot, backup, or manifest artifact been preserved outside test/dev fixtures?
2. Should storage format identity remain under `V1_0` until first release, or should the project add explicit subformat versions such as `HeapPageV1` and `BTreeKeyV1` immediately?
3. Is `Bytes` intended to support ordered indexes?
4. Are composite indexes always fixed-arity?
5. Is text ordering strictly UTF-8 byte lexical order, or will future collation rules change index order?
6. Should heap slot metadata be authoritative in the footer, the header, or both with redundancy checks?
7. Can heap pages be rebuilt from WAL/catalog data, or are they non-rebuildable truth after checkpoint?

---

## Required follow-up work

1. Add B-Tree key-format metadata and incompatible-index rebuild/reject gates.
2. Add B-Tree key golden vectors and ordering/property tests.
3. Remove or gate unsupported datum debug-string fallback for durable index keys.
4. Choose one canonical heap page reader/writer and remove undocumented slot-count ambiguity.
5. Add heap page golden vectors and legacy rejection tests.
6. Add manifest or page/index storage format fingerprints.
7. Add recovery pre-redo storage format validation.
8. Add backup/restore format rejection tests.

---

## Doctrine checks

This decision does not introduce SQL, gRPC, JSON runtime wire formats, GPU durability behavior, or unsafe Rust. It reinforces the durability and recovery doctrine that persistent bytes must be versioned, validated, and recoverable or explicitly rejected.
