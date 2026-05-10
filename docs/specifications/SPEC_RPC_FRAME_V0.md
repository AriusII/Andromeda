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
| `SurfaceScope` | Application, Administration, HA/DR, or internal surface scope bound to admission and authorization. |
| `StreamRole` | Transport stream role that permits only the matching frame family. |
| `FrameFamily` | Session control, contract control, RPC command, RPC result stream, diagnostic, or telemetry. |
| `ProtobufEnvelope` | Custom typed Protobuf payload envelope; never gRPC service framing, REST, or JSON-native payload contract. |
| `RpcFrameRejectionCode` | Stable typed rejection code for malformed frame, incompatible version, bounds failure, scope mismatch, or prohibited payload form. |

## Invariants

- Frames are typed and bounded.
- ProtocolMagic and ProtocolVersion are validated before payload decode.
- PayloadLength is validated before allocation.
- PayloadLength must not exceed `16 MiB` and must also fit the admitted request resource budget before allocation.
- Frame length arithmetic must be checked for `usize` overflow before slicing, copying, or allocating payload buffers.
- Metadata precedes payload.
- Application Surface cannot invoke admin operations.
- RPC semantics are custom typed Protobuf over QUIC; gRPC is not the native application surface.
- JSON-native payloads are not a V0 protocol format.
- QUIC owns transport liveness only; Procedure identity, contract binding, admission, transaction scope, WAL durability, and ResultStream semantics are owned by Andromeda RPC and engine contracts.
- `SurfaceScope` is explicit in the Protobuf command envelope and must be authorized before transaction creation; it is never inferred from a port, socket, certificate, or stream role alone.


## Serialization

- Persisted and network-visible formats use canonical encoding.
- Headers use explicit fixed-width integer fields in network big-endian order for this wire protocol.
- Variable payloads declare length before payload.
- Critical persisted structures use version fields.
- Rust native struct layout must not be persisted or sent over the wire.
- CRC and bounds checks are performed on the encoded header before payload decode.
- Custom Protobuf messages are payload contracts inside Andromeda frames. Protobuf `service`/`rpc` definitions, generated gRPC services, tonic service surfaces, HTTP/JSON fallback, REST request handlers, and JSON Schema are not native protocol contracts.

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

### Wire constants

| Constant | Value | Rejection rule |
|---|---:|---|
| Header length | `52` bytes | Reject any other header length before reading payload fields. |
| ProtocolVersion | `1` | Reject unsupported versions before frame type or payload decode. |
| Header byte order | Network big-endian | Reject codecs that reinterpret the header through native Rust layout. |
| HeaderCrc algorithm | CRC32/ISO-HDLC | Reject CRC mismatch; compute with bytes `48..52` set to zero. |
| Maximum PayloadLength | `16 MiB` | Reject larger declared payloads before allocation even if the caller budget is larger. |
| Maximum scanned frames | `4,096` | Return `ResourceError` when one buffer contains more frames. |
| Reserved header bytes | Zero | Reject any nonzero reserved byte. |
| Reserved flags | None assigned in V0 | Reject every nonzero flag unless this spec is versioned. |

### FrameType v0 registry

| Code | FrameType | FrameFamily | Required StreamRole | Payload rule |
|---:|---|---|---|---|
| `1` | `Hello` | `SessionControl` | `SessionControl` | Protobuf session-control envelope. |
| `2` | `Auth` | `SessionControl` | `SessionControl` | Protobuf authentication envelope. |
| `3` | `ContractRequest` | `ContractControl` | `CommandBidirectional` | Protobuf contract request envelope. |
| `4` | `ContractResponse` | `ContractControl` | `CommandBidirectional` | Protobuf contract response envelope. |
| `5` | `RpcExecuteRequest` | `RpcCommand` | `CommandBidirectional` | Non-empty `ProtobufEnvelope` carrying Procedure invocation, `ContractHash`, `CatalogVersion`, `PolicyVersion`, resource budget, and `SurfaceScope`. |
| `6` | `RpcMetadata` | `RpcResultStream` | `ResultUnidirectional` | ResultStream metadata; must precede `RpcBatch` and `RpcCompletion`. |
| `7` | `RpcBatch` | `RpcResultStream` | `ResultUnidirectional` | Non-empty ResultStream payload batch; requires prior metadata. |
| `8` | `RpcCompletion` | `RpcResultStream` | `ResultUnidirectional` | Typed terminal completion, rollback, cancellation, rejection, or error summary. |
| `9` | `Error` | `Diagnostic` | `Diagnostic` | Non-empty typed error envelope. |
| `100` | `TelemetrySoftSignal` | `Telemetry` | `TelemetryDatagram` | Soft telemetry only; never Procedure execution or durable truth. |

Codes not listed above are reserved and must be rejected as `ProtocolError`. `TelemetrySoftSignal` is intentionally outside the Protobuf payload code lockstep and cannot carry command or result semantics.

### ProtobufEnvelope v0 contract

Every non-empty RPC payload is a custom Andromeda Protobuf message selected by `FrameType`. The envelope must bind:

| Field | Requirement |
|---|---|
| `RequestId` and `SessionId` | Nonzero where the frame participates in an invocation or session context. |
| `TransactionId` | Present only when the transaction marker is `1`; absent when the marker is `0`. |
| `ContractHash` | Required for Procedure invocation and ResultStream frames tied to a Procedure contract. |
| `CatalogVersion` | Required when catalog-bound Procedure or result metadata is visible. |
| `PolicyVersion` | Required for admitted Procedure invocation and authorization-sensitive results. |
| `SurfaceScope` | Required on command envelopes and must match the admitted route context. |
| `PayloadKind` | Must match the enclosing `FrameType`; payload-kind spoofing is rejected. |

The native protocol rejects gRPC service frames, HTTP/JSON fallback bodies, JSON-native RPC payloads, string command frames, and native Rust struct payloads. JSON may be used only for out-of-band tooling or documentation artifacts that are not accepted by the native RPC decoder.

### SurfaceScope admission

`SurfaceScope` values are `Application`, `Administration`, `HADR`, and `Internal`. `Application` admits only Procedure execution authorized by Procedure contracts. `Administration` and `HADR` require their own permissions, policy evidence, and audit evidence. `Internal` is not externally routable.

A frame is rejected before execution when:

| Condition | Error family | Evidence |
|---|---|---|
| Frame family is not permitted by `StreamRole` | `ProtocolError` | Frame type, stream role, request/session context when available. |
| `SurfaceScope` is missing from a command envelope | `ContractError` | Procedure name when decoded, contract hash when available. |
| Command envelope surface differs from admitted route surface | `PermissionError` | Admitted surface, requested surface, principal when resolved, policy version when available. |
| Application surface requests Administration or HA/DR permission | `PermissionError` | Required permission, surface, principal, policy version, denial reason. |
| Internal surface arrives from an external route | `PermissionError` | Route, certificate identity, and rejection reason. |

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

### Typed frame rejection mapping

| Rejection | Error family |
|---|---|
| Bad header length, unsupported version, unknown frame type, invalid transaction marker, nonzero reserved bytes, CRC mismatch, payload-kind spoofing, native Rust layout, gRPC service frame, or JSON-native payload | `ProtocolError` |
| Payload length over `16 MiB`, admitted budget exceeded, frame length overflow, scan batch over `4,096` frames, or backpressure admission failure | `ResourceError` |
| Missing `ContractHash`, missing `CatalogVersion`, missing `PolicyVersion`, missing `SurfaceScope`, or Protobuf envelope shape mismatch | `ContractError` |
| Surface mismatch, Application surface invoking admin/HA/DR operation, or unauthorised principal | `PermissionError` |
| Durable mutation without transaction/WAL evidence | `TransactionError` or `StorageError` according to the owning durable spec |

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

### Trace evidence requirements

Pre-auth protocol rejection evidence must not invent a principal. It records `TraceId`, remote route identity when available, `FrameType` when decoded, `RpcFrameRejectionCode`, `ErrorKind`, and bounded details without payload dumps.

Post-auth command rejection evidence must additionally record `RequestId`, `SessionId`, `InvocationId` when assigned, `SurfaceScope`, `PrincipalId`, `PolicyVersion`, `ContractHash` when decoded, and the typed permission or contract reason. Payload bytes are never logged as native evidence.

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
- FrameType registry and StreamRole separation tests.
- payload-kind spoofing tests.
- admitted budget before allocation tests.
- typed rejection evidence tests.

## Rejection criteria

- Reject `dynamic string command frame`.
- Reject `bad RPC protocol magic`.
- Reject `unsupported RPC protocol version`.
- Reject `unexpected RPC frame header length`.
- Reject `nonzero reserved RPC frame bytes`.
- Reject `nonzero reserved RPC frame flags`.
- Reject `invalid RPC transaction marker`.
- Reject `RPC frame header CRC mismatch`.
- Reject `unknown RPC frame type code`.
- Reject `RPC frame length overflow`.
- Reject `truncated RPC frame payload`.
- Reject `trailing bytes after single RPC frame decode`.
- Reject `more than 4,096 RPC frames in one decode buffer`.
- Reject `allocation before length check`.
- Reject `RPC payload over 16 MiB`.
- Reject `RPC payload over admitted budget`.
- Reject `admin frame on application surface`.
- Reject `internal surface frame from external route`.
- Reject `surface inferred from transport only`.
- Reject `protobuf payload kind that does not match FrameType`.
- Reject `gRPC service frame on native application surface`.
- Reject `JSON-native RPC payload`.
- Reject `HTTP/JSON fallback as native RPC`.
- Reject `native Rust layout RPC payload`.

## Acceptance summary

Owner: Person 12 owns the RPC frame contract with implementation evidence from `crates/andromeda-rpc-protocol`, Protobuf envelope evidence from `crates/andromeda-proto-wire`, and boundary doctrine from `docs/adr/ADR-0007-QUIC_RPC_BOUNDARY_NO_GRPC.md`.

Evidence: acceptance requires retained golden-vector and rejection tests proving the 52-byte network-big-endian `FrameHeader`, CRC32/ISO-HDLC field-zeroing, `ProtocolVersion = 1`, exact `FrameType` registry, `PayloadLength <= 16 MiB`, budget-before-allocation checks, `4,096` frame scan bound, `StreamRole` separation, `SurfaceScope` admission binding, custom Protobuf envelope context, and no gRPC/JSON-native protocol path.

Reject: acceptance is denied for any implementation that accepts unsupported versions, native Rust wire layout, unknown frame codes, nonzero reserved bytes or flags, CRC mismatch, truncated or trailing payload bytes, oversized or unbudgeted payloads, surface inferred from transport only, Application surface admin/HA/DR execution, gRPC service framing, HTTP/JSON fallback, JSON-native RPC payloads, or string command frames.
