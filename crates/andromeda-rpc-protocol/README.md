# andromeda-rpc-protocol

## Purpose

`andromeda-rpc-protocol` owns Andromeda's runtime-free RPC frame contract: frame type codes, fixed header layout, explicit frame codecs, stream roles, frame-family routing, backpressure signaling, and ResultStream sequence validation.

This crate defines how typed protocol payloads are framed and sequenced. It does not own QUIC sockets, Quinn, TLS, listener lifecycle, Procedure execution, IAM policy authority, WAL, storage, or recovery behavior.

## Scope

This crate is responsible for:

| Area | Responsibility |
| --- | --- |
| Frame codes | Locked frame type codes for hello, auth, contract request and response, RPC execute request, metadata, batch, completion, error, and telemetry soft signal frames. |
| Frame header | A fixed 52-byte header contract with explicit fields, bounded payload length, reserved flag rejection, transaction-id presence marker, and header CRC. |
| Frame codec | Explicit encode, decode, scan-one, and scan-all behavior using the current network byte order frame contract. |
| Stream roles | Runtime-free `StreamRole` and `FrameFamily` definitions that separate command, result, control, telemetry, and reserved stream use. |
| ResultStream sequencing | Validation that result streams are metadata, zero or more batches, then completion. |
| Backpressure | Typed, bounded backpressure signals with constrained transport routing. |
| Protocol drift checks | Invariant validators for frame layout, payload-kind lockstep, and protocol version lock. |

The crate provides the contract layer consumed by `andromeda-quic` and tested against `andromeda-proto` payload kinds.

## Non-goals

- Do not introduce gRPC, generated services, HTTP/2 RPC compatibility, `tonic`, `grpcio`, or `prost-grpc`.
- Do not introduce application-facing ad hoc SQL, generic command text, dynamic table names, or dynamic predicates.
- Do not bypass typed, cataloged Procedure contracts. RPC execute frames carry typed Procedure invocation payloads; they are not a command-text tunnel.
- Do not own concrete QUIC transport, Quinn runtime adapters, TLS, mTLS certificate extraction, reconnect sockets, or listener lifecycle.
- Do not own Procedure semantics, executor dispatch, transaction creation, durable WAL, storage truth, recovery, IAM policy stores, or audit sinks.
- Do not serialize Rust native structs directly to disk or network. The frame codec is an explicit byte contract, not a native layout export.

## Prerequisites

Before changing this crate, understand:

- Frame type codes are locked and must stay in lockstep with protocol payload-kind discriminators.
- The frame header layout is a byte contract. Keep field offsets, header length, CRC offset, and reserved fields explicit.
- The current RPC frame header uses network byte order. Do not generalize this to persistent storage formats.
- ResultStream frames must be ordered as metadata, batch frames, and completion.
- Application, Administration, HA/DR, monitoring, and telemetry surfaces must remain separable at the stream and frame-family level.

## Procedure

1. Add new frame behavior only when the frame family, stream role, payload kind, and validation rule are all explicit.
2. Update `FrameType` wire codes and lockstep invariants together. Never reuse or reorder existing codes.
3. Keep decoder behavior bounded. Reject truncated headers, corrupt CRCs, oversized payload declarations, unknown frame types, reserved flags, and unbounded frame batches.
4. Validate frame sequences before dispatch. Command streams, result streams, control streams, telemetry paths, and reserved paths must accept only their allowed frame families.
5. Keep protocol errors typed and deterministic.
6. Keep this crate runtime-free. If a change needs sockets, TLS, Quinn, timers, or executor integration, put that behavior in `andromeda-quic` or another runtime owner.

## Validation

For documentation-only changes, validate this README against `AGENTS.md`, `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`, and `docs/adr/ADR-0007-QUIC_RPC_BOUNDARY_NO_GRPC.md`.

For frame or protocol code changes in this crate, prefer:

```bash
cargo fmt --all --check
cargo test -p andromeda-rpc-protocol --tests
cargo test -p andromeda-rpc-protocol --test frame_wire_contract
cargo test -p andromeda-rpc-protocol --test forbidden_surface_drift
cargo check -p andromeda-rpc-protocol --all-targets
```

If frame bytes, codes, or sequence rules change, include compatibility evidence from `andromeda-proto` and `andromeda-quic` lockstep tests.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A new frame type overlaps an existing code. | Allocate a new locked code and update protocol invariant tests. |
| A decoder accepts reserved flags or corrupt CRCs. | Reject the frame as a protocol error before payload handling. |
| A command stream carries result frames. | Reject the sequence through stream role and frame family validation. |
| A design proposes gRPC. | Use Andromeda's custom RPC frame contract and Protobuf message payloads instead. |
| A frame carries raw SQL text. | Replace it with a typed RPC execute payload bound to a cataloged Procedure contract. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `docs/adr/ADR-0002-WORKSPACE_AND_CRATE_BOUNDARIES.md`
- `docs/adr/ADR-0007-QUIC_RPC_BOUNDARY_NO_GRPC.md`
- `crates/andromeda-rpc-protocol/src/lib.rs`
- `crates/andromeda-rpc-protocol/src/frame_codec.rs`
- `crates/andromeda-rpc-protocol/src/frame_sequence.rs`
- `crates/andromeda-rpc-protocol/src/protocol_invariants.rs`
- `crates/andromeda-rpc-protocol/tests/frame_wire_contract.rs`
- `crates/andromeda-rpc-protocol/tests/forbidden_surface_drift.rs`
