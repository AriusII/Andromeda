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
| `RecordLength` | Must be represented as an explicit typed structure or canonical descriptor. |
| `RecordLengthInv` | Must be represented as an explicit typed structure or canonical descriptor. |
| `Crc64` | Must be represented as an explicit typed structure or canonical descriptor. |
| `ChainHash` | Must be represented as an explicit typed structure or canonical descriptor. |
| `TxBegin` | Transaction begin record kind. |
| `RowInsert` | Row insert redo payload record kind. |
| `TxCommit` | Commit record kind that becomes visible only after durable flush. |

## Invariants

- LSN is monotonic.
- PrevLsn validates chain.
- CRC mismatch is detected.
- Truncated tail stops at last valid record.
- WAL records are canonical little-endian.
- `TxCommit` is not visible until the WAL is durable through its LSN.
- `RecordLength` and `RecordLengthInv` must be checked before payload allocation.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### WalRecordFrameV0 byte layout

WAL record headers are 72 bytes, little-endian, followed by exactly `payload_length` bytes.

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

- roundtrip property tests.
- truncated tail tests.
- CRC mismatch tests.
- chain mismatch tests.
- TxBegin/RowInsert/TxCommit replay-order tests.
- RecordLengthInv malformed-input tests.
- fuzz wal_record.
- 72-byte header golden vector tests.
- forensic chain-break scan tests.

## Rejection criteria

- Reject `record without CRC`.
- Reject `record without LSN`.
- Reject `commit visible before flush_through commit LSN`.
- Reject `native Rust struct serialization`.
- Reject `unchecked payload length`.
- Reject `RecordLengthInv mismatch`.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove the listed invariants without hidden defaults.
