# Specification: RPC Frame v0

> **Status:** Normative V0 specification  
> **Audience:** Andromeda maintainers, engine developers, architects, reviewers  
> **Language:** American English  
> **Baseline:** Rust 1.95.0, Rust 2024 Edition, x64 and ARM64 first

## In this article


- Define the purpose and scope of `RPC Frame v0`.
- State the required structures.
- State invariants, errors, security, recovery, tests, and rejection criteria.

## Purpose

Define typed RPC frame boundaries.

## Scope

This specification applies to V0 documentation and implementation planning. It defines the minimum stable contract needed for code, tests, and review.

## Non-goals

- It does not define a final production implementation.
- It does not weaken Andromeda's procedure-only surface.
- It does not authorize hidden dynamic behavior.

## Data structures

| Structure | Required role |
|---|---|
| `FrameHeader` | Must be represented as an explicit typed structure or canonical descriptor. |
| `FrameType` | Must be represented as an explicit typed structure or canonical descriptor. |
| `RequestId` | Must be represented as an explicit typed structure or canonical descriptor. |
| `SessionId` | Must be represented as an explicit typed structure or canonical descriptor. |
| `PayloadLength` | Must be represented as an explicit typed structure or canonical descriptor. |
| `Flags` | Must be represented as an explicit typed structure or canonical descriptor. |
| `HeaderCrc` | Must be represented as an explicit typed structure or canonical descriptor. |
| `RpcError` | Must be represented as an explicit typed structure or canonical descriptor. |
| `ProtocolMagic` | Fixed magic value for custom Andromeda RPC frames. |
| `ProtocolVersion` | Explicit wire version for compatibility and rejection. |
| `SurfaceScope` | Application, Administration, HA/DR, or internal surface scope. |
| `ProtobufEnvelope` | Custom typed Protobuf payload envelope; never gRPC service framing. |

## Invariants

- Frames are typed and bounded.
- ProtocolMagic and ProtocolVersion are validated before payload decode.
- PayloadLength is validated before allocation.
- PayloadLength must not exceed the admitted resource budget.
- Metadata precedes payload.
- Application Surface cannot invoke admin operations.
- RPC semantics are custom typed Protobuf over QUIC; gRPC is not the native application surface.
- JSON-native payloads are not a V0 protocol format.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.

### FrameHeader v0 wire layout

Frame headers are 52 bytes, network big-endian, followed by exactly `PayloadLength` bytes.

| Offset | Size | Field | Validation |
|---:|---:|---|---|
| 0 | 2 | Header length | Must be `52`. |
| 2 | 2 | ProtocolVersion | Must be `1`. |
| 4 | 4 | FrameType | Must be a known `FrameType` code. |
| 8 | 8 | RequestId | Must be non-zero when required by the frame type. |
| 16 | 8 | SessionId | Must be non-zero when required by the frame type. |
| 24 | 8 | TransactionId value | Must be zero when TransactionId marker is `0`. |
| 32 | 1 | TransactionId marker | Must be `0` or `1`. |
| 33 | 3 | Reserved | Must be all zero. |
| 36 | 8 | PayloadLength | Must fit `usize`, not exceed the admitted frame budget, and match available bytes. |
| 44 | 4 | Flags | Must contain only known bits for the frame type. |
| 48 | 4 | HeaderCrc | CRC32/ISO-HDLC over the 52-byte header with this field zeroed. |

The bounded batch scanner must reject more than 4,096 frames in one buffer.

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

- invalid frame tests.
- bad magic and unsupported version tests.
- oversized payload tests.
- metadata-before-payload tests.
- surface scope tests.
- gRPC and JSON-native protocol rejection tests.
- 52-byte header golden vector tests.
- CRC field-zeroing tests.
- bounded frame batch tests.

## Rejection criteria

- Reject `dynamic string command frame`.
- Reject `bad RPC protocol magic`.
- Reject `unsupported RPC protocol version`.
- Reject `allocation before length check`.
- Reject `admin frame on application surface`.
- Reject `gRPC service frame on native application surface`.
- Reject `JSON-native RPC payload`.

## Acceptance summary

This specification is acceptable when implementation, tests, and documentation can prove the listed invariants without hidden defaults.
