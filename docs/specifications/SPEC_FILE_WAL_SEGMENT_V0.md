# Specification: FileWal Segment v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `FileWal Segment v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define WAL segment file format and rotation behavior.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `WalSegmentHeader` | Fixed 80-byte `FileWalHeaderV0` stored at file offset `0`. |
| `WalSegmentTrailer` | Not persisted in mono-segment V0; segment integrity is represented by header checksum plus durable-prefix scan evidence. A future persisted trailer requires a new format version. |
| `TimelineId` | Not persisted in mono-segment V0; the implicit V0 timeline is the single local WAL file. Multi-timeline storage requires a versioned format. |
| `StartLsn` | First valid record LSN for the segment. Mono-segment V0 requires `StartLsn = 1`. |
| `PreviousSegmentHash` | Not persisted for mono-segment V0 and must be treated as zero/absent. Multi-segment V0+ must bind the previous durable segment hash before replay. |
| `SegmentCrc` | Header checksum plus durable-prefix record checksums and derived chain hash; V0 uses non-zero FNV-1a 64-bit for header checksum. |
| `DurablePrefix` | Highest contiguous LSN range proven readable and checksummed. |
| `FlushThroughLsn` | Explicit fence proving bytes are durable through a target LSN. |

## Invariants

- Segments are append-only.
- PreviousSegmentHash links segments.
- Rotation preserves replay order.
- Segment header is verified before replay.
- Recovery only trusts the durable prefix.
- `flush_through` evidence is required before a visible commit can be acknowledged.
- Mono-segment V0 requires `SegmentId = 1`, `StartLsn = 1`, `BasePreviousLsn = 0`, and `Reserved = 0`.
- File length beyond the durable prefix is untrusted physical tail and must be truncated or ignored before replay.
- LSN chain breaks inside the durable prefix are forensic failures, not recoverable tails.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### FileWalHeaderV0 byte layout

The FileWal segment header is exactly 80 bytes at file offset `0`. All integer fields are little-endian.

| Offset | Size | Field | Required value or validation |
|---:|---:|---|---|
| 0 | 8 | Magic | Must be `0x314C415752444E41`; on disk bytes are `41 4E 44 52 57 41 4C 31` (`ANDRWAL1`). |
| 8 | 2 | FormatVersion | Must be `1`. |
| 10 | 2 | ByteOrder | Must be `0x0102`, the V0 little-endian marker. |
| 12 | 4 | HeaderLength | Must be `80`. |
| 16 | 8 | SegmentId | Must be `1` for mono-segment V0. |
| 24 | 8 | FirstLsn | Must be `1`. |
| 32 | 8 | BasePreviousLsn | Must be `0` for mono-segment V0. |
| 40 | 8 | DurableLsn | Highest LSN covered by durable bytes, or `0` for an empty WAL. |
| 48 | 8 | DurableBytes | Number of record bytes after the header that are claimed durable. |
| 56 | 8 | DurableRecordCount | Number of complete records in the durable prefix. |
| 64 | 8 | HeaderChecksum | Non-zero FNV-1a 64-bit over offsets `0..64` and `72..80`, with this field excluded. |
| 72 | 8 | Reserved | Must be `0`. |

Record bytes begin at offset `80`. Each record is a `WalRecordFrameV0` from `SPEC_WAL_RECORD_V0.md`; FileWal must not define a second record format.

### Header checksum

`HeaderChecksumV0` is non-zero FNV-1a 64-bit with offset basis `0xcbf29ce484222325` and prime `0x00000100000001b3`. The checksum input is:

```text
Magic ||
FormatVersion ||
ByteOrder ||
HeaderLength ||
SegmentId ||
FirstLsn ||
BasePreviousLsn ||
DurableLsn ||
DurableBytes ||
DurableRecordCount ||
Reserved
```

If the computed state is zero, the stored value is `1`. Any mismatch rejects the file before record scan.

### Durable prefix algorithm

`DurablePrefix` is the validated prefix produced by scanning only:

```text
scan_len = min(Header.DurableBytes, PhysicalFileLength - 80)
scan_range = bytes[80 .. 80 + scan_len]
```

The scan must use `WalRecordFrameV0` validation with `FirstLsn` and `BasePreviousLsn`. The effective durable prefix is:

```text
DurablePrefixBytes = scan.valid_bytes
DurablePrefixLsn = scan.last_valid_lsn or 0
DurablePrefixRecordCount = scan.records.len
```

Header fields are claims, not truth. If header `DurableBytes`, `DurableLsn`, or `DurableRecordCount` exceed the validated scan result, recovery must trust the smaller validated prefix and emit recovery evidence. A physical tail beyond `80 + DurablePrefixBytes` must not be replayed and must be truncated before normal append resumes.

Recoverable scan stops:

| Stop | Policy |
|---|---|
| `TruncatedHeader` at tail | Trust last valid prefix; truncate tail before append. |
| `TruncatedRecord` at tail | Trust last valid prefix; truncate tail before append. |
| `CorruptHeader` or `CorruptRecord` after a valid prefix | Trust only last valid prefix and require corruption evidence. |

Forensic scan stops:

| Stop | Policy |
|---|---|
| `LsnGap` | Reject normal open; require forensic startup decision. |
| `DuplicateOrReorderedLsn` | Reject normal open; require forensic startup decision. |
| `PreviousLsnMismatch` | Reject normal open; require forensic startup decision. |

### flush_through contract

`flush_through(target_lsn)` succeeds only when all of the following are true:

1. `target_lsn` is zero or already durable, in which case the existing durable LSN is returned without weakening the durable prefix.
2. `target_lsn` has been appended and is not beyond the last appended record.
3. The implementation knows the exact end byte of the target record.
4. Record bytes through that end byte are flushed with file data sync.
5. A new `FileWalHeaderV0` is written with `DurableLsn = target_lsn`, `DurableBytes = target_end_bytes`, and `DurableRecordCount = target_record_index + 1`.
6. The header update is flushed with full file sync.

Only after step 6 may `FlushThroughLsn = target_lsn` be returned as evidence. A visible commit may be acknowledged only when `FlushThroughLsn >= commit_lsn`. Short flush, missing sync evidence, or a target LSN that is not an appended boundary is rejection evidence.

### Golden vectors

The empty mono-segment V0 header must roundtrip with:

```text
Magic=0x314C415752444E41
FormatVersion=1
ByteOrder=0x0102
HeaderLength=80
SegmentId=1
FirstLsn=1
BasePreviousLsn=0
DurableLsn=0
DurableBytes=0
DurableRecordCount=0
Reserved=0
```

Golden tests must also cover a flushed `TxBegin` -> `RowInsert` prefix, a fully flushed `TxBegin` -> `RowInsert` -> `TxCommit` chain, a physical tail beyond the durable prefix, a corrupted header checksum, a header claiming more bytes than the valid prefix, and forensic chain breaks inside claimed durable bytes.

## State transitions

State transitions must be explicit. Invalid transitions return typed errors and emit trace evidence when they affect execution, storage, security, or recovery.

```text
EmptyFile -> HeaderInitializedAndSynced -> AppendOpen
AppendOpen -> RecordsAppended -> FlushDataSynced -> HeaderUpdated -> HeaderSynced -> DurablePrefixAdvanced
AppendOpen -> RecoverableTailDetected -> TailTruncated -> HeaderSynced -> AppendOpen
AppendOpen -> ForensicChainBreakDetected -> NormalOpenRejected
```

There is no transition that makes appended but unflushed records durable, and no transition that acknowledges a commit before the header-synced durable prefix covers the commit LSN.

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

Recovery validates `FileWalHeaderV0`, scans only the claimed durable byte range bounded by physical file length, and replays only the validated durable prefix. Recoverable physical tails are truncated before append resumes. Forensic LSN chain breaks reject normal open and must produce a `RecoveryReportV0` or equivalent startup evidence before any operator decision. A `TxCommit` outside the durable prefix is ignored for visibility.

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

- `FileWalHeaderV0` roundtrip and golden vector tests.
- header magic, version, byte-order marker, checksum, reserved-field, and mono-segment rejection tests.
- segment scan tests for physical tail, truncated header, truncated record, corrupt record, LSN gap, duplicate/reordered LSN, and `PrevLsn` mismatch.
- previous hash tests documented as V0 multi-segment exclusions: mono-segment V0 must reject non-zero `BasePreviousLsn` and non-`1` segment id.
- partial segment tests proving recovery trusts only the durable prefix.
- durable prefix truncation tests proving unflushed physical bytes are removed or ignored before append.
- `flush_through` visibility fence tests proving no visible commit before header-synced durable prefix covers `TxCommit`.

## Rejection criteria

- Reject `segment rewrite`.
- Reject `missing timeline`.
- Reject `unverified segment header`.
- Reject `visible commit beyond durable prefix`.
- Reject `flush_through without fsync evidence`.
- Reject `non-mono SegmentId in FileWalHeaderV0`.
- Reject `non-zero BasePreviousLsn in mono-segment V0`.
- Reject `header DurableLsn without durable records`.
- Reject `header DurableBytes that cannot be validated by WalRecordFrameV0 scan`.
- Reject `normal open after LSN gap, duplicate/reordered LSN, or PrevLsn mismatch`.

## Acceptance summary

Owner: Personne 08 / FileWal owner crates. Evidence: `crates/andromeda-wal/tests/file_wal_contract.rs`, WAL codec golden tests, recovery corruption tests, crash recovery evidence mapped through `SPEC_CRASH_RECOVERY_TEST_PLAN_V0.md`, and retained durable-prefix vectors. Reject acceptance if `flush_through` can report success without data-sync plus header-sync evidence, if a visible commit can exceed the durable prefix, if a header claim is trusted over a validated scan, or if normal open proceeds after a forensic LSN/`PrevLsn` chain break.
