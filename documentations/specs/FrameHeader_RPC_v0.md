# FrameHeader RPC v0 Specification

## Purpose

Define the accepted documentation contract for Andromeda RPC frame headers used
by the runtime-free RPC protocol boundary.

This specification records the current explicit wire contract for `FrameHeader`
and the frame families used by the QUIC adapter. It is documentation acceptance
only. It does not create a new wire format and does not authorize any runtime
behavior beyond the code evidence already present in the workspace.

## Scope

This specification applies to the v0 RPC frame header contract owned by
`andromeda-rpc-protocol` and consumed by QUIC transport adapters.

It covers:

- fixed encoded header length and field offsets;
- frame type wire codes;
- payload length and scan bounds;
- header checksum placement;
- stream-role compatibility;
- metadata-before-payload ordering for ResultStream frames;
- no-gRPC, no-runtime-JSON, no-native-layout documentation acceptance.

## Non-goals

This specification does not:

- introduce gRPC, tonic, generated services, HTTP/2 RPC compatibility, or
  runtime JSON defaults;
- introduce ad hoc SQL, generic command text, dynamic table names, or dynamic
  predicates;
- change Protobuf schemas or descriptor hashes;
- define QUIC listener lifecycle, Quinn runtime behavior, TLS configuration, or
  socket scheduling;
- define Procedure semantics, authorization policy, WAL durability, storage
  truth, or recovery behavior;
- serialize Rust native struct layout to the network;
- change the repository rule that persistent formats use explicit codecs and
  canonical little-endian serialization unless a specific network format
  documents an explicit exception.

## Prerequisites

Reviewers must use these references before accepting changes to this spec:

- DEC-017 for QUIC runtime dependency ownership.
- DEC-021 for Protobuf message-only schema governance and metadata-before-
  payload rules.
- DEC-040 for RPC, QUIC, IAM, audit, and surface-plane separation.
- DEC-041 for security vocabulary and explicit mapping requirements.
- ADR-0011 for crate boundary ownership.
- `crates/andromeda-rpc-protocol/src/frame_codec.rs` for encoded header fields.
- `crates/andromeda-rpc-protocol/src/protocol_invariants.rs` for locked
  protocol invariants.
- `crates/andromeda-rpc-protocol/tests/frame_wire_contract.rs` for current
  frame-wire evidence.

## Procedure

### Ownership

`andromeda-rpc-protocol` owns the runtime-free frame, stream, codec, and
ResultStream sequencing contract. It must not depend on Quinn, Rustls, Tokio,
listener lifecycles, executor dispatch, IAM runtime stores, WAL, storage, or
recovery code.

`andromeda-quic` may consume this contract as a concrete transport adapter. It
must not redefine the frame header, own Procedure semantics, own authorization
policy, or become a source of storage truth.

### Encoded header layout

The encoded RPC frame header is fixed at 52 bytes. All integer fields in this
RPC frame header use network byte order, which is big-endian. This is an
explicit RPC network-wire contract and not a storage-format rule.

The Rust `FrameHeader` struct is a domain model. It is not the wire layout.
Encoding and decoding must use the explicit codec.

| Offset | Size | Field | Rule |
| --- | ---: | --- | --- |
| 0 | 2 | `header_len` | Must equal `52`. |
| 2 | 2 | `codec_version` | Must equal `1`. |
| 4 | 4 | `frame_type` | Must be a locked frame type code. |
| 8 | 8 | `request_id` | Unsigned request correlation identifier. |
| 16 | 8 | `session_id` | Unsigned session correlation identifier. |
| 24 | 8 | `tx_id` | Transaction identifier value when present, zero otherwise. |
| 32 | 1 | `tx_id_present` | Must be `0` or `1`. |
| 33 | 3 | `reserved` | Must be zero. |
| 36 | 8 | `payload_length` | Must be less than or equal to 16 MiB. |
| 44 | 4 | `flags` | All bits are reserved in v0 and must be zero. |
| 48 | 4 | `header_crc` | CRC-32 over the encoded header with this field zeroed. |

### Frame type codes

Frame type codes are locked for v0:

| Code | Frame type | Stream role |
| ---: | --- | --- |
| 1 | `HELLO` | Session control |
| 2 | `AUTH` | Session control |
| 3 | `CONTRACT_REQUEST` | Command bidirectional |
| 4 | `CONTRACT_RESPONSE` | Command bidirectional |
| 5 | `RPC_EXECUTE_REQUEST` | Command bidirectional |
| 6 | `RPC_METADATA` | Result unidirectional |
| 7 | `RPC_BATCH` | Result unidirectional |
| 8 | `RPC_COMPLETION` | Result unidirectional |
| 9 | `ERROR` | Diagnostic |
| 100 | `TELEMETRY_SOFT_SIGNAL` | Telemetry datagram only |

Payload kind discriminators remain in lockstep with frame type codes 1 through
9. Telemetry soft signals are outside the contract-bound RPC payload range and
must not carry Procedure payloads.

### Validation order

Frame validation must fail closed in this order:

1. Check the 52-byte minimum header length.
2. Check `header_len` and `codec_version`.
3. Check reserved bytes and the `tx_id_present` marker.
4. Recompute and validate `header_crc` with the CRC field zeroed.
5. Decode `frame_type` and reject unknown codes.
6. Reject any reserved flag bit.
7. Reject `payload_length` above 16 MiB.
8. Reject truncated payloads or trailing bytes for single-frame decode.
9. Check stream-role compatibility.
10. Reject required payloads that are empty.

Bulk frame scans must remain bounded. The current decoder limit is 4,096 frames
per scan. Changing this limit requires an explicit compatibility and
backpressure review.

### ResultStream ordering

ResultStream frames must preserve metadata-before-payload ordering:

```text
RPC_METADATA -> RPC_BATCH* -> RPC_COMPLETION
```

`RPC_BATCH` must not appear before `RPC_METADATA`. `RPC_COMPLETION` is terminal.
Zero-row completion requires an explicit metadata policy that allows completion
without payload batches.

### Surface and security relationship

This frame header does not authorize work. Frame validity is only the protocol
precondition for later admission. `SecurityAdmission v0` must still validate
surface, contract, principal/policy evidence, resource budget, and audit
evidence before transaction creation.

## Validation

Documentation acceptance checks:

- The spec names DEC-040 and DEC-041.
- The spec says no gRPC, no runtime JSON default, no ad hoc SQL, and no native
  Rust struct layout serialization.
- The spec identifies `andromeda-rpc-protocol` as runtime-free and
  `andromeda-quic` as a concrete transport boundary.
- The spec does not claim QUIC listener runtime completion.

Existing code evidence to use when code validation is allowed:

```powershell
cargo test -p andromeda-rpc-protocol --test frame_wire_contract
cargo test -p andromeda-quic --test protocol_stability_contract
```

These commands are not required for WR5-DOC documentation-only acceptance.

## Troubleshooting

| Symptom | Likely cause | Corrective action |
| --- | --- | --- |
| Header CRC mismatch | Bytes changed after encoding or CRC computed with a nonzero CRC field. | Recompute CRC over the fixed header with `header_crc` zeroed. |
| Unknown frame type | A caller used an unregistered frame code. | Reject as a protocol error and require a decision record for new codes. |
| Payload length mismatch | Declared length does not match available bytes. | Reject before payload interpretation. |
| Result batch before metadata | ResultStream ordering is invalid. | Reject as a protocol error. |
| RPC payload over datagram | Contract-bound RPC payload is being routed through telemetry. | Reject and use reliable stream roles. |
| Documentation says gRPC | Boundary wording drifted from DEC-040. | Replace with custom Andromeda RPC over QUIC. |

## References

- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `documentations/governance/decisions/DEC-017-quic-runtime-dependency.md`
- `documentations/governance/decisions/DEC-021-protobuf-schema-contract.md`
- `documentations/governance/decisions/DEC-040-rpc-quic-security-boundary.md`
- `documentations/governance/decisions/DEC-041-security-contract-boundary.md`
- `crates/andromeda-rpc-protocol/src/frame_codec.rs`
- `crates/andromeda-rpc-protocol/src/frame_struct.rs`
- `crates/andromeda-rpc-protocol/src/protocol_invariants.rs`
- `crates/andromeda-rpc-protocol/tests/frame_wire_contract.rs`
