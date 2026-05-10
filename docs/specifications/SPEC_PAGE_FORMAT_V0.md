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
- PageLsn must be less than or equal to the durable WAL LSN before page flush.
- Recovery redo applies a WAL record to a page only when the record LSN is newer than the page PageLsn.
- Torn writes are detectable.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### PageCodecV1 layout

Page images use a 112-byte little-endian header, the payload, then a 48-byte trailer.
All integer fields are unsigned and little-endian. Missing, padded, or reserved bytes are part of the canonical image and must be zero.

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
| 70 | 2 | ReservedHeaderPad0 | Must be zero. |
| 72 | 4 | PayloadOffset | Must be `112`. |
| 76 | 4 | PayloadLength | Must not overlap trailer. |
| 80 | 4 | FreeStart | Must be within payload. |
| 84 | 4 | FreeEnd | Must be within payload and >= FreeStart. |
| 88 | 4 | FreeBytes | Must equal `FreeEnd - FreeStart`. |
| 92 | 2 | SlotCount | Must be >= row count. |
| 94 | 2 | ReservedHeaderPad1 | Must be zero. |
| 96 | 4 | RowCount | Must be zero on free pages. |
| 100 | 4 | HeaderCrc | Non-zero owner checksum evidence. It is not the V1 header integrity authority. |
| 104 | 4 | HeaderIntegrityCrc | CRC32/ISO-HDLC over the header with this field zeroed. |
| 108 | 4 | Reserved | Must remain zero. |

| Trailer offset | Size | Field | Validation |
|---:|---:|---|---|
| 0 | 8 | PayloadCrc64 | Must be non-zero and match the payload. |
| 8 | 32 | PageHash | Must be non-zero and match the payload hash. |
| 40 | 8 | TornWriteGuard | Must be non-zero and not merely the PageId. |

### Integrity algorithms

The decoder must validate the header before allocating payload memory:

1. Require an input image at least `112 + 48` bytes.
2. Read and validate PageMagic, FormatVersion, HeaderLength, PayloadOffset, and page size tag.
3. Validate all identity, flag, link, free-space, row-count, and reserved-byte fields.
4. Validate HeaderIntegrityCrc before trusting the payload length.
5. Require the total image length to equal `112 + PayloadLength + 48`.
6. Validate the trailer and payload integrity.

`HeaderIntegrityCrc` uses CRC32/ISO-HDLC over the full 112-byte header with bytes 104..107 treated as zero. Parameters are reflected polynomial `0xEDB88320`, initial value `0xFFFF_FFFF`, final xor `0xFFFF_FFFF`, and a zero result normalized to `1`.

`PayloadCrc64` is the V0 compatibility field name. Its V1 algorithm is FNV-1a-64 over the payload bytes with offset basis `0xcbf29ce484222325`, prime `0x00000100000001b3`, and a zero result normalized to `1`. A future replacement by CRC64 requires a new format version or an explicit compatibility rule.

`PageHash` is SHA-256 over the payload bytes. An all-zero digest is invalid.

`TornWriteGuard` is FNV-1a-64 over these little-endian values in order: PageMagic, FormatVersion, PageId, ObjectId, AllocationId, PageLsn, PageEpoch, PayloadCrc64, PayloadOffset, PayloadLength. If the computed value is zero or equals PageId, xor it with `0xA9D378B54C2F6101`.

### PageLSN and recovery rules

PageLSN is the persisted page replay boundary:

- A dirty page must not be flushed unless the durable WAL LSN is greater than or equal to PageLsn.
- A flushed page whose PageLsn is ahead of the durable WAL LSN is corruption and requires StorageError plus RecoveryReport evidence.
- Redo must skip a page mutation record when `record_lsn <= PageLsn`.
- Redo must apply a page mutation record when `record_lsn > PageLsn` and all target identity and hash checks match.
- Free pages still carry non-zero PageLsn and PageEpoch so recovery can reject stale or torn reuse.

### Slot directory bounds

Slot directory bytes are part of the payload contract. Implementations must reject slot offsets that point outside payload, overlap header or trailer, overlap another live slot, are not consistent with `SlotCount`, or require `usize` truncation to interpret.

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

Recovery validates PageMagic, FormatVersion, HeaderIntegrityCrc, trailer integrity, PageLsn, PageId, ObjectId, and AllocationId before redo. Corrupt page bytes do not become truth; recovery either rebuilds the page from a valid snapshot plus WAL or records StorageError and enters the RecoveryReport mode required by the owning recovery spec.

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
- redo skip/apply tests for `record_lsn <= PageLsn` and `record_lsn > PageLsn`.
- reserved header pad rejection tests.
- slot offset overflow, out-of-payload, and overlap rejection tests.
- malformed corpus coverage under `tests/fuzzing/corpus/page_codec_v1_decode`.

## Rejection criteria

- Reject `repr(Rust) persistence`.
- Reject `bad page magic`.
- Reject `unsupported page format version`.
- Reject `usize in persisted format`.
- Reject `page without PageLsn`.
- Reject `page flush when durable WAL is behind PageLsn`.
- Reject `page image length mismatch`.
- Reject `nonzero reserved page bytes`.
- Reject `payload CRC mismatch`.
- Reject `payload hash mismatch`.
- Reject `torn write guard mismatch`.
- Reject `unchecked slot offset`.

## Acceptance summary

Owner: Personne 09 owns PageHeader/PageTrailer V0 with the `andromeda-storage-page`, `andromeda-buffer-pool`, `andromeda-disk-page-store`, `andromeda-wal`, and `andromeda-recovery` owner crates.

Evidence: acceptance requires page codec roundtrip and corruption tests, golden vectors for the 112-byte header and 48-byte trailer, PageLsn WAL fence tests, redo skip/apply tests, slot-directory bounds tests, and malformed corpus coverage under `tests/fuzzing/corpus/page_codec_v1_decode`.

Reject: reviewers must reject implementations that persist native Rust layout, skip PageMagic or FormatVersion validation, flush a page whose PageLsn is ahead of durable WAL, accept bad header CRC, payload CRC, payload hash, or torn-write guard evidence, accept nonzero reserved bytes, or trust unchecked slot offsets.
