# Specification: WalRecord v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `WalRecord v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define WAL record identity, payload, checksums, chain hash, and replay behavior.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `WalRecord` | Must be represented as an explicit typed structure or canonical descriptor. |
| `WalRecordType` | Must be represented as an explicit typed structure or canonical descriptor. |
| `Lsn` | Must be represented as an explicit typed structure or canonical descriptor. |
| `PrevLsn` | Previous record pointer for chain validation within the transaction or stream. |
| `TxId` | Must be represented as an explicit typed structure or canonical descriptor. |
| `RecordLength` | Canonical record byte count. In V0 it is the `TotalLength` header field and must equal `HeaderLength + PayloadLength`. |
| `RecordLengthInv` | Derived inverse-length guard equal to `u64::MAX ^ RecordLength`. V0 does not allocate a separate persisted offset for it; tests and decoders must still compute it before payload allocation. |
| `Crc64` | V0 record/header integrity field. The current V0 algorithm is the non-zero FNV-1a 64-bit checksum defined below; replacing it with CRC64-ECMA is a versioned breaking format change. |
| `ChainHash` | Derived scan hash over the valid durable record prefix. V0 computes it from the prior chain hash, record identity, checksums, and payload bytes; it is not a separate 72-byte frame field. |
| `TxBegin` | Transaction begin record kind; tag `1`, requires `TransactionId`, zero-length payload for the generic transaction boundary. |
| `RowInsert` | Row insert redo payload record kind; tag `6`, requires `TransactionId`, payload is owner-codec bytes and must be replayed only for durable committed transactions. |
| `TxCommit` | Commit record kind; tag `2`, requires `TransactionId`, zero-length payload for the generic transaction boundary, and becomes visible only after `flush_through(commit_lsn)` succeeds. |

## Invariants

- LSN is non-zero and strictly contiguous in scan order.
- PrevLsn validates chain and must equal the previous valid record LSN, or the segment base previous LSN for the first record.
- Record checksum and header checksum mismatches are detected before replay.
- Truncated tail stops at last valid record and never contributes visible state.
- WAL records are canonical little-endian.
- `TxCommit` is not visible until the WAL is durable through its LSN.
- `RecordLength` and `RecordLengthInv` must be checked before payload allocation.
- `ChainHash` binds the durable prefix and is trace/recovery evidence; a chain hash mismatch is forensic evidence, not a recoverable tail.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### WalRecordFrameV0 byte layout

WAL record headers are 72 bytes, little-endian, followed by exactly `payload_length` bytes.

V0 frames do not carry a byte-order marker inside each record. The canonical byte order is little-endian; FileWal carries the byte-order marker at the segment boundary. A decoder must reject any frame whose magic, checksum, or fixed fields cannot be interpreted as this little-endian layout.

| Offset | Size | Field | Validation |
|---:|---:|---|---|
| 0 | 8 | Magic | Must be `0x414E44524F57414C` (`ANDROWAL`). |
| 8 | 2 | FormatVersion | Must be `1`. |
| 10 | 2 | HeaderLength | Must be `72`. |
| 12 | 8 | TotalLength | Must be at least `HeaderLength` and equal header plus payload. |
| 20 | 2 | KindTag | Must be a known record kind such as `TxBegin`, `RowInsert`, or `TxCommit`. |
| 22 | 2 | Flags | Only `HAS_PREVIOUS_LSN` and `HAS_TRANSACTION_ID` are valid. |
| 24 | 8 | Lsn | Must be non-zero and monotonic in scan order. |
| 32 | 8 | PrevLsn | Must be zero unless the previous-LSN flag is set. |
| 40 | 8 | TransactionId | Must be zero unless the transaction-id flag is set. |
| 48 | 8 | PayloadLength | Must equal `TotalLength - HeaderLength`. |
| 56 | 8 | RecordChecksum | Must validate the payload and record identity. |
| 64 | 8 | HeaderChecksum | Must validate the header with this field zeroed. |

Recovery stops at the last valid record for truncated tails. LSN gaps, reordered LSNs, and previous-LSN mismatches are forensic chain breaks.

### Record kind tags

| Tag | Kind | V0 replay class |
|---:|---|---|
| 1 | `TxBegin` | Transaction boundary; requires `TransactionId`. |
| 2 | `TxCommit` | Terminal commit boundary; requires `TransactionId` and durable flush before visibility. |
| 3 | `TxRollback` | Terminal rollback boundary; requires `TransactionId`. |
| 4 | `PageAllocate` | Redo-relevant page operation. |
| 5 | `PageFormat` | Redo-relevant page operation. |
| 6 | `RowInsert` | Redo-relevant row operation; requires `TransactionId`. |
| 7 | `RowUpdate` | Redo-relevant row operation; requires `TransactionId`. |
| 8 | `RowDelete` | Redo-relevant row operation; requires `TransactionId`. |
| 9 | `IndexInsert` | Access-path mutation; replay is gated by rebuild/quarantine policy. |
| 10 | `IndexDelete` | Access-path mutation; replay is gated by rebuild/quarantine policy. |
| 11 | `MvccVersionCreate` | Redo-relevant MVCC operation; requires `TransactionId`. |
| 12 | `MvccVersionClose` | Redo-relevant MVCC operation; requires `TransactionId`. |
| 13 | `MapDeltaAppend` | Redo-relevant map delta operation. |
| 14 | `CheckpointBegin` | Checkpoint boundary. |
| 15 | `CheckpointEnd` | Checkpoint boundary. |
| 16 | `SnapshotBegin` | Snapshot boundary. |
| 17 | `SnapshotEnd` | Snapshot boundary. |
| 18 | `ManifestSwitch` | Redo-relevant manifest switch boundary. |
| 19 | `CatalogChangeBegin` | Catalog transaction boundary; requires `TransactionId`. |
| 20 | `CatalogChangeApply` | Redo-relevant catalog operation; requires `TransactionId`. |
| 21 | `CatalogChangeCommit` | Redo-relevant catalog commit operation; requires `TransactionId`. |
| 22 | `SecurityAuditAppend` | Redo-relevant durable audit operation. |
| 23 | `BTreeInsert` | Access-path mutation; fail-stop until promoted by explicit rebuild evidence. |
| 24 | `BTreeDelete` | Access-path mutation; fail-stop until promoted by explicit rebuild evidence. |
| 25 | `BTreeSplit` | Access-path mutation; fail-stop until promoted by explicit rebuild evidence. |
| 26 | `BTreeMerge` | Access-path mutation; fail-stop until promoted by explicit rebuild evidence. |

Unknown tags are rejected. A transaction-scoped tag without `TransactionId` is rejected.

### Integrity algorithms

All integer inputs are folded as little-endian bytes.

`RecordChecksumV0`:

```text
state = 0xcbf29ce484222325
for value in [KindTag as u64, Lsn, PrevLsn or 0, TransactionId or 0, PayloadLength]:
  fold each little-endian byte with FNV-1a 64-bit prime 0x00000100000001b3
for byte in Payload:
  fold byte with FNV-1a 64-bit prime 0x00000100000001b3
if state == 0, store 1, else store state
```

`HeaderChecksumV0` uses the same non-zero FNV-1a 64-bit algorithm over header fields from offset `0` through `63`, in field order, excluding the `HeaderChecksum` field itself. `PrevLsn` and `TransactionId` are encoded as zero when their flag is absent.

`RecordLengthInvV0` is a derived validation value:

```text
RecordLengthV0 = TotalLength
RecordLengthInvV0 = u64::MAX ^ RecordLengthV0
PayloadLength == RecordLengthV0 - HeaderLength
```

Decoders must validate the length relationship and derived inverse before allocating payload memory. Malformed-vector tests must include inverse-length mismatch cases, even though V0 does not persist a separate inverse field.

`ChainHashV0` is derived during scan:

```text
ChainHash0 = 0
ChainHashN = FNV64_NONZERO(
  ChainHashN-1 ||
  FormatVersion ||
  HeaderLength ||
  KindTag ||
  Flags ||
  Lsn ||
  PrevLsn or 0 ||
  TransactionId or 0 ||
  TotalLength ||
  PayloadLength ||
  RecordChecksum ||
  HeaderChecksum ||
  Payload
)
```

Recovery evidence and forensic scans must report the final chain hash for the durable prefix when the caller requests forensic detail.

### Validation order

1. Read exactly 72 header bytes; if fewer bytes remain, report `TruncatedHeader`.
2. Decode fixed fields as little-endian and validate magic, version, header length, flags, length relationship, derived `RecordLengthInv`, and header checksum before payload allocation.
3. Reject unknown `KindTag`, zero `Lsn`, unflagged non-zero `PrevLsn`, unflagged non-zero `TransactionId`, or missing transaction id for transaction-scoped kinds.
4. Ensure `TotalLength` fits the host allocation bound and the available scan buffer; if not, report `TruncatedRecord`.
5. Validate `RecordChecksumV0` over the payload and record identity.
6. Validate strict next-LSN order and `PrevLsn` chain continuity before admitting the record to the durable prefix.
7. Update `ChainHashV0` only after all prior checks pass.

Only truncation at the physical tail is recoverable. Corrupt headers, corrupt records, LSN gaps, duplicate or reordered LSNs, and `PrevLsn` mismatches stop replay at the last valid prefix and require forensic evidence.

### Golden vectors

The canonical zero-payload `TxBegin` vector is:

```text
KindTag=1, Lsn=1, PrevLsn=0, TransactionId=7, PayloadLength=0
4c 41 57 4f 52 44 4e 41 01 00 48 00 48 00 00 00
00 00 00 00 01 00 02 00 01 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 07 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 62 b9 da e7 d0 06 99 ba
e8 9e 23 a5 d9 40 8e be
```

Golden tests must also cover a valid `TxBegin` -> `RowInsert` -> `TxCommit` chain, an unknown kind tag, `RecordLengthInv` mismatch, checksum mismatch, truncated header, truncated payload, LSN gap, duplicate/reordered LSN, and `PrevLsn` mismatch.

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

Recovery scans the durable prefix only. A record is replay-eligible only after the frame validates, the scan chain remains contiguous, and the owning transaction has a durable `TxCommit` whose LSN is covered by `flush_through`. Uncommitted `TxBegin`/`RowInsert` prefixes are ignored for replay and reported as ignored transactions. A `TxCommit` beyond the durable prefix is not visible and must not be used to publish state.

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

- roundtrip property tests for `WalRecordFrameV0`.
- truncated header and truncated payload tail tests.
- record checksum and header checksum mismatch tests.
- derived `ChainHashV0` chain mismatch tests.
- `TxBegin`/`RowInsert`/`TxCommit` replay-order tests.
- `RecordLengthInv` malformed-input tests.
- fuzz `wal_record`.
- 72-byte header golden vector tests.
- forensic chain-break scan tests for LSN gap, duplicate/reordered LSN, and `PrevLsn` mismatch.
- visible-commit fence tests proving `TxCommit` is ignored until `flush_through(commit_lsn)` succeeds.

## Rejection criteria

- Reject `record without CRC`.
- Reject `record without LSN`.
- Reject `commit visible before flush_through commit LSN`.
- Reject `native Rust struct serialization`.
- Reject `unchecked payload length`.
- Reject `RecordLengthInv mismatch`.
- Reject `unknown WalRecordKind tag`.
- Reject `transaction-scoped record without TransactionId`.
- Reject `LSN gap, duplicate LSN, reordered LSN, or PrevLsn mismatch without forensic classification`.

## Acceptance summary

Owner: Personne 08 / WAL owner crates. Evidence: `crates/andromeda-wal-codec/tests/wal_codec_contract.rs`, `crates/andromeda-wal/tests/wal_codec_contract.rs`, corruption/recovery tests, property tests, and retained golden vectors. Reject acceptance if any implementation can make a visible commit before `flush_through(commit_lsn)`, decode a native Rust layout, allocate from unchecked length fields, or replay a chain that fails checksum, LSN, `PrevLsn`, `RecordLengthInv`, or `ChainHashV0` evidence.
