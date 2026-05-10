# Specification: SegmentIndex v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `SegmentIndex v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define compact startup index for pages, key ranges, column chunks, and roots.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `SegmentIndex` | Must be represented as an explicit typed structure or canonical descriptor. |
| `PageLocator` | Must be represented as an explicit typed structure or canonical descriptor. |
| `KeyRangeEntry` | Must be represented as an explicit typed structure or canonical descriptor. |
| `IndexRootDirectory` | Must be represented as an explicit typed structure or canonical descriptor. |
| `ColumnChunkDirectory` | Must be represented as an explicit typed structure or canonical descriptor. |
| `BloomDirectory` | Must be represented as an explicit typed structure or canonical descriptor. |
| `SegmentIndexMagic` | Fixed magic value that identifies the SegmentIndex before decode. |
| `SegmentIndexVersion` | Explicit index format version tied to the manifest. |

## Invariants

- Startup does not scan all cold segments.
- SegmentIndex magic and version are validated before locator use.
- PageLocator verifies PageHash.
- KeyRange entries are policy-bound.
- SegmentIndex matches manifest hash.
- SegmentIndex is bound to one DatabaseId, SnapshotId, ManifestVersion, and ParentManifestHash.
- SegmentIndex contains only published cold segment entries in V0.
- Native Rust layout is not a SegmentIndex format.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### SegmentIndexV0 file layout

SegmentIndex files use a 256-byte little-endian header, `entry_count` fixed 160-byte entries, optional extension bytes, and a 96-byte trailer.

| Header offset | Size | Field | Validation |
|---:|---:|---|---|
| 0 | 8 | Magic | Must be `ANDSGIX0`. |
| 8 | 2 | FormatMajor | Must be `1`. |
| 10 | 2 | FormatMinor | Must be `0`. |
| 12 | 2 | ByteOrder | Must be `0x0102`. |
| 14 | 2 | HeaderLength | Must be `256`. |
| 16 | 8 | TotalLength | Must equal header + entries + extension + trailer. |
| 24 | 4 | HeaderCrc32 | CRC32/ISO-HDLC with this field zeroed. |
| 28 | 4 | Flags | Only published-cold, contiguous-ranges, and forensic-hold bits are valid. |
| 32 | 8 | DatabaseId | Must be non-zero. |
| 40 | 8 | SnapshotId | Must be non-zero. |
| 48 | 8 | SegmentIndexId | Must be non-zero. |
| 56 | 8 | ManifestVersion | Must be non-zero. |
| 64 | 8 | BaseCheckpointLsn | Must be non-zero. |
| 72 | 8 | RequiredWalStartLsn | Must be non-zero. |
| 80 | 8 | EntryOffset | Must be `256`. |
| 88 | 8 | EntryCount | Must be between `1` and `16,777,216`. |
| 96 | 4 | EntryLength | Must be `160`. |
| 100 | 4 | PageSizePolicyTag | Must match entry page-size policy. |
| 104 | 8 | ExtensionOffset | Must follow entry table. |
| 112 | 8 | ExtensionLength | Must be <= 1 MiB. |
| 120..176 | fixed | Segment/page/LSN summaries | Must match the entry table. |
| 176 | 32 | ParentManifestHash | Must be non-zero. |
| 208 | 32 | EntryTableSha256 | Must match encoded entries. |
| 240 | 8 | EntryTableCrc64 | Must match encoded entries. |
| 248 | 8 | Reserved | Must be zero. |

| Entry offset | Size | Field | Validation |
|---:|---:|---|---|
| 0 | 8 | SegmentId | Must be non-zero and strictly increasing. |
| 8 | 8 | ObjectId | Must be non-zero. |
| 16 | 8 | AllocationId | Must be non-zero. |
| 24 | 8 | FirstExtentId | Must be non-zero. |
| 32 | 4 | ExtentCount | Must be non-zero and not overflow extent range. |
| 36 | 2 | PageSizeTag | `1` = 16 KiB, `2` = 32 KiB. |
| 38 | 2 | SegmentStateTag | Must be `3` PublishedCold in V0. |
| 40 | 8 | FirstPageId | Must be non-zero. |
| 48 | 4 | PageCount | Must be non-zero and not overflow page range. |
| 52 | 4 | Reserved | Must be zero. |
| 56 | 8 | MinPageLsn | Must be non-zero. |
| 64 | 8 | MaxPageLsn | Must be >= MinPageLsn. |
| 72 | 8 | SnapshotId | Must match header SnapshotId. |
| 80 | 8 | SegmentFileId | Must be non-zero. |
| 88 | 8 | SegmentFileOffset | Must not overflow with SegmentByteLen. |
| 96 | 8 | SegmentByteLen | Must be non-zero. |
| 104 | 8 | SegmentPayloadCrc64 | Must be non-zero and match the listed segment bytes. |
| 112 | 32 | SegmentSha256 | Must be non-zero and match the listed segment bytes. |
| 144 | 4 | SegmentHeaderCrc32 | Must be non-zero and match the segment header. |
| 148 | 4 | SegmentTrailerCrc32 | Must be non-zero and match the segment trailer. |
| 152 | 4 | EntryFlags | Must be zero in V0. |
| 156 | 4 | EntryCrc32 | CRC32/ISO-HDLC over the 160-byte entry with this field zeroed. |

| Trailer offset | Size | Field | Validation |
|---:|---:|---|---|
| 0 | 8 | EntryTableCrc64Mirror | Must equal header EntryTableCrc64. |
| 8 | 32 | SegmentIndexFileSha256 | SHA-256 over the encoded file with acyclic trailer hash fields zeroed. |
| 40 | 32 | SegmentIndexRootHash | SHA-256 over ParentManifestHash, SegmentIndexId, SnapshotId, EntryTableSha256, and SegmentIndexFileSha256. |
| 72 | 4 | TrailerCrc32 | CRC32/ISO-HDLC over the trailer with this field zeroed. |
| 76 | 4 | TrailerFlags | Must be zero in V0. |
| 80 | 16 | Reserved | Must be zero. |

### Integrity algorithms

`HeaderCrc32`, `EntryCrc32`, and `TrailerCrc32` use CRC32/ISO-HDLC with reflected polynomial `0xEDB88320`, initial value `0xFFFF_FFFF`, final xor `0xFFFF_FFFF`, and the target CRC field zeroed.

`EntryTableCrc64` uses CRC64/ECMA with polynomial `0x42F0E1EBA9EA3693`, initial value `0`, no final xor, and zero normalized to `1`.

`EntryTableSha256`, `SegmentIndexFileSha256`, `SegmentIndexRootHash`, and `SegmentSha256` use SHA-256. All-zero digest values are invalid.

The decoder must validate header magic, version, byte order, header length, total length, entry count, entry length, extension bounds, and header CRC before allocating the entry vector. The decoded file must re-encode to the same bytes for golden vector acceptance.

### Extension policy

Extension bytes are optional and bounded to 1 MiB. Each extension record starts with an 8-byte record header: type `u16`, flags `u16`, and payload length `u32`, followed by payload bytes. Unknown required extension records, identified by flag `0x0001`, are rejected. Unknown optional extension records may be preserved but are not interpreted as startup truth.

### Locator and startup rules

PageLocator resolution must use SegmentIndex entries rather than scanning all cold segments. A locator is valid only when the page id falls within an entry page range, the entry is bound to the accepted manifest, and the segment payload CRC64, segment SHA-256, segment header CRC32, and segment trailer CRC32 all match the referenced bytes.

If SegmentIndex is absent, corrupt, stale, or bound to a different manifest, normal Online startup must reject it and use the RecoveryReport mode selected by recovery. Rebuild is allowed only from manifest-listed durable artifacts and retained WAL evidence; it must not silently promote an unlisted cold scan to normal startup truth.

## State transitions

State transitions must be explicit. Invalid transitions return typed errors and emit trace evidence when they affect execution, storage, security, or recovery.

## Error model

| Error family | Use |
|---|---|
| ContractError | Invalid shape, incompatible hash, missing contract field. |
| PermissionError | Principal lacks required permission or surface scope. |
| ResourceError | Budget, quota, backpressure, or timeout failure. |
| TransactionError | Isolation, rollback, commit, or serialization failure. |
| StorageError | WAL, page, segment, manifest, or corruption failure. |
| SystemError | Internal condition requiring poison, rollback, forensic, or restore path. |

## Security model

Security-sensitive operations require admission through identity, principal, permission, policy, and audit checks before durable mutation or transaction creation.

## Observability

At minimum, implementations must emit trace evidence with:

```text
TraceId
InvocationId when applicable
CatalogVersion when applicable
PolicyVersion when applicable
Result
ErrorKind when applicable
```

## Recovery behavior

Recovery validates SegmentIndex after the DatabaseManifest root is accepted and before using locators for page, key range, column chunk, or root access. A stale or corrupt SegmentIndex cannot be trusted as durable truth. Recovery may rebuild an index only from manifest-listed immutable cold segments plus durable WAL evidence, and RecoveryReport must record whether the accepted index was loaded, rebuilt, rejected, or unavailable.

## Compatibility

Changes are classified as:

| Change | Default status |
|---|---|
| Add optional field with explicit default | Additive |
| Add required field | Breaking |
| Change type or cardinality | Breaking |
| Change security requirement | Security-impact |
| Change recovery behavior | Breaking unless explicitly versioned |

## Tests

- startup no-full-scan test.
- bad magic and unsupported version tests.
- locator hash tests.
- range prune tests.
- invalid index fallback tests.
- 256-byte header, 160-byte entry, and 96-byte trailer golden vector tests.
- stale manifest hash fallback tests.
- entry table SHA-256 and CRC64 mismatch tests.
- header, entry, and trailer CRC32 mismatch tests.
- reserved header, entry, and trailer byte rejection tests.
- extension required-flag rejection tests.
- manifest-bound locator hash and range tests.
- absent, corrupt, stale, and mismatched SegmentIndex startup-mode tests.
- malformed corpus coverage under `tests/fuzzing/corpus/segment_index_decode`.

## Rejection criteria

- Reject `full cold scan required for normal startup`.
- Reject `bad SegmentIndex magic`.
- Reject `unsupported SegmentIndex version`.
- Reject `locator without hash`.
- Reject `index not tied to manifest`.
- Reject `SegmentIndex native Rust layout`.
- Reject `SegmentIndex header CRC mismatch`.
- Reject `SegmentIndex entry CRC mismatch`.
- Reject `SegmentIndex trailer CRC mismatch`.
- Reject `EntryTableSha256 mismatch`.
- Reject `EntryTableCrc64 mismatch`.
- Reject `SegmentIndexRootHash mismatch`.
- Reject `nonzero SegmentIndex reserved bytes`.
- Reject `PublishedCold-only violation`.
- Reject `unknown required SegmentIndex extension`.

## Acceptance summary

Owner: Personne 09 owns SegmentIndex V0 with the `andromeda-segment`, `andromeda-manifest`, and `andromeda-recovery` owner crates.

Evidence: acceptance requires `crates/andromeda-segment/tests/segment_index_contract.rs`, malformed corpus coverage under `tests/fuzzing/corpus/segment_index_decode`, startup-mode evidence that does not scan all cold segments for normal Online startup, and golden vectors for the 256-byte header, 160-byte entry, 96-byte trailer, EntryTableSha256, EntryTableCrc64, and SegmentIndexRootHash.

Reject: reviewers must reject implementations that persist native Rust layout, omit magic/version/byte-order checks, accept bad CRC or hash evidence, use a SegmentIndex not tied to the accepted manifest, trust an unlisted cold scan as normal startup truth, or skip the required fallback and RecoveryReport evidence.
