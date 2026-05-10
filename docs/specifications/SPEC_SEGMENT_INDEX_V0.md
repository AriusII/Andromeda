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

Each entry is 160 bytes and ends with `EntryCrc32` at offset 156. The trailer mirrors the entry-table CRC64 at offset 0, stores the file SHA256 at offset 8, root hash at offset 40, trailer CRC32 at offset 72, flags at offset 76, and 16 zero reserved bytes at offset 80.

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

If this specification affects durable state, it must define how recovery replays, validates, rebuilds, or rejects the affected state.

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

## Rejection criteria

- Reject `full cold scan required for normal startup`.
- Reject `bad SegmentIndex magic`.
- Reject `unsupported SegmentIndex version`.
- Reject `locator without hash`.
- Reject `index not tied to manifest`.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove the listed invariants without hidden defaults.
