# Specification: PageHeader and PageTrailer v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `PageHeader and PageTrailer v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define persisted page format.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `PageHeader` | Must be represented as an explicit typed structure or canonical descriptor. |
| `PageTrailer` | Must be represented as an explicit typed structure or canonical descriptor. |
| `PageId` | Must be represented as an explicit typed structure or canonical descriptor. |
| `PageLsn` | Must be represented as an explicit typed structure or canonical descriptor. |
| `SlotDirectory` | Must be represented as an explicit typed structure or canonical descriptor. |
| `PayloadCrc64` | Must be represented as an explicit typed structure or canonical descriptor. |
| `PageHash` | Must be represented as an explicit typed structure or canonical descriptor. |
| `TornWriteGuard` | Must be represented as an explicit typed structure or canonical descriptor. |
| `PageMagic` | Fixed magic value that identifies the page family before decode. |
| `FormatVersion` | Fixed-width format version used for compatibility and rejection. |

## Invariants

- No Rust-native struct serialization.
- Page magic is validated before field decode.
- FormatVersion is validated before payload interpretation.
- PageId is logical, not physical offset.
- PageLsn indicates WAL coverage.
- Torn writes are detectable.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### PageCodecV1 layout

Page images use a 112-byte little-endian header, the payload, then a 48-byte trailer.

| Header offset | Size | Field | Validation |
|---:|---:|---|---|
| 0 | 4 | PageMagic | Must be `0x414E4452`. |
| 4 | 2 | FormatVersion | Must be `1`. |
| 6 | 2 | PageSize tag | `1` = 16 KiB, `2` = 32 KiB. |
| 8 | 2 | PageType tag | `1` fixed row, `2` hybrid row, `3` manifest, `4` free. |
| 10 | 2 | Flags | Only known previous, next, and cold-immutable bits are valid. |
| 12 | 8 | PageId | Must be non-zero. |
| 20 | 8 | ObjectId | Must be non-zero for non-free pages. |
| 28 | 8 | AllocationId | Must be non-zero for non-free pages. |
| 36 | 8 | PageLsn | Must be non-zero and covered by durable WAL before flush. |
| 44 | 8 | PageEpoch | Must be non-zero. |
| 52 | 8 | PreviousPageId | Zero means absent; must match flag. |
| 60 | 8 | NextPageId | Zero means absent; must match flag. |
| 68 | 2 | HeaderLength | Must be `112`. |
| 72 | 4 | PayloadOffset | Must be `112`. |
| 76 | 4 | PayloadLength | Must not overlap trailer. |
| 80 | 4 | FreeStart | Must be within payload. |
| 84 | 4 | FreeEnd | Must be within payload and >= FreeStart. |
| 88 | 4 | FreeBytes | Must equal `FreeEnd - FreeStart`. |
| 92 | 2 | SlotCount | Must be >= row count. |
| 96 | 4 | RowCount | Must be zero on free pages. |
| 100 | 4 | HeaderCrc | Must be non-zero. |
| 104 | 4 | HeaderIntegrityCrc | CRC32/ISO-HDLC over the header with this field zeroed. |
| 108 | 4 | Reserved | Must remain zero. |

| Trailer offset | Size | Field | Validation |
|---:|---:|---|---|
| 0 | 8 | PayloadCrc64 | Must be non-zero and match the payload. |
| 8 | 32 | PageHash | Must be non-zero and match the payload hash. |
| 40 | 8 | TornWriteGuard | Must be non-zero and not merely the PageId. |

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

- page codec tests.
- bad magic and unsupported version tests.
- free-space accounting tests.
- torn write tests.
- fuzz page_header.
- 112-byte header and 48-byte trailer golden vector tests.
- PageLsn WAL fence tests.

## Rejection criteria

- Reject `repr(Rust) persistence`.
- Reject `bad page magic`.
- Reject `unsupported page format version`.
- Reject `usize in persisted format`.
- Reject `page without PageLsn`.
- Reject `unchecked slot offset`.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove the listed invariants without hidden defaults.
