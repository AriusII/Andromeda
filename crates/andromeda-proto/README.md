# andromeda-proto

## Purpose

`andromeda-proto` owns Andromeda's generated Protobuf schema boundary: crate-local `.proto` sources, prost output, descriptor bytes, schema hashes, and governance checks.

This crate provides message schemas and generated module access. Runtime validation and wire behavior are owned by `andromeda-proto-wire`, `andromeda-rpc-protocol`, and `andromeda-rpc-codec`. It does not define a gRPC service, a JSON runtime, or an application command surface. Andromeda RPC remains a custom typed protocol over explicit frames and QUIC transport.

## Scope

This crate is responsible for:

| Area | Responsibility |
| --- | --- |
| Protobuf schemas | Message-only `.proto` files under `proto/andromeda/...`, compiled through `prost-build`. |
| Generated boundary | Generated message exports, descriptor set bytes, descriptor set hashing, and compatibility reexports while callers migrate to owner crates. |
| Procedure manifests | Schema declarations for Procedure manifest descriptors, required permissions, protocol layout, policy version, result stream descriptors, and manifest hashing. |
| Completion and errors | Schema declarations for structured completion status, transaction outcome, row-count summaries, error envelopes, retry disposition, and backpressure metadata. |
| Structured payloads | Schema declarations for StructuredObject headers and payload bounds. |

The crate sits at the protocol schema boundary. It can say which bytes are valid generated messages, but it does not open sockets, route QUIC streams, authorize principals, execute Procedures, write WAL, or decide storage truth.

## Non-goals

- Do not introduce gRPC, `tonic`, generated service definitions, HTTP/2 RPC compatibility, or `prost-grpc`.
- Do not introduce runtime JSON defaults, `serde` wire contracts, or stringly typed payload fallbacks.
- Do not introduce application-facing ad hoc SQL, generic command text, dynamic table names, or dynamic predicates.
- Do not bypass typed, cataloged Procedure contracts. Invocation payloads must bind to contract hash, catalog version, stats version, and structured arguments.
- Do not treat generated messages as Rust native layout for disk or network. The schema is the contract; explicit encoders and validators own boundary behavior.
- Do not own QUIC transport, frame byte layout, listener lifecycle, IAM runtime state, policy stores, WAL, storage, recovery, or audit sinks.

## Prerequisites

Before changing this crate, understand:

- Protobuf schemas are message-only. `.proto` files must not declare `service` or `rpc`.
- The build script rejects forbidden boundary identifiers including gRPC, tonic, JSON, serde, SQL, service, and rpc in active `.proto` source.
- `FrameEnvelope` and generated protocol messages must preserve deterministic projection and compatibility rules.
- Result stream ordering is metadata, zero or more batches, then completion.
- Metadata and row-count policy must be available before payload inspection.
- Application and Administration traffic must remain distinguishable through typed manifests and surface scope, not through dynamic command text.

## Procedure

1. Add or update message schemas under the crate-local `proto/` tree.
2. Keep new schemas message-only. Do not add service definitions or RPC method declarations.
3. Keep generated-message validation in the owning protocol crates. `andromeda-proto-wire` owns envelope and payload validation; `andromeda-rpc-protocol` owns frame and stream invariants; `andromeda-rpc-codec` owns typed frame/envelope codec helpers.
4. Keep Procedure invocation payloads bound to `procedure_name`, expected contract hash, expected catalog version, expected stats version, surface scope, and structured arguments.
5. Keep manifest resolution explicit. A Procedure manifest must carry protocol layout, required permissions, result stream descriptors, policy version, contract hash, catalog version, and stats version.
6. When adding a payload kind or frame envelope projection, update lockstep tests with `andromeda-rpc-protocol` and `andromeda-quic`.

## Validation

For documentation-only changes, validate this README against `AGENTS.md`, `docs/adr/ADR-0011-workspace-crate-boundaries.md`, and `docs/adr/ADR-0012-quic-rpc-no-grpc.md`.

For schema or code changes in this crate, prefer:

```bash
cargo fmt --all --check
cargo test -p andromeda-proto --tests
cargo test -p andromeda-proto --test protobuf_determinism_tests
cargo test -p andromeda-proto --test protocol_contract
cargo check -p andromeda-proto --all-targets
```

If schemas change, include compatibility evidence for generated descriptors and deterministic serialization. Malformed input handling and frame/result-stream projection evidence belongs with the protocol owner crates.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A `.proto` addition needs `service` or `rpc`. | Keep the schema message-only and route it through Andromeda RPC frames instead. |
| A payload wants raw SQL text. | Replace it with a typed Procedure selector and structured arguments bound to a contract hash and catalog version. |
| A caller wants JSON as the runtime protocol. | Keep JSON out of the runtime boundary; use generated Protobuf messages and explicit validators. |
| A manifest omits required permissions or protocol layout. | Reject it as an invalid contract boundary before dispatch. |
| A result stream emits batches before metadata. | Reject the sequence; ResultStream metadata must precede payload frames. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `crates/andromeda-proto/build.rs`
- `crates/andromeda-proto/src/lib.rs`
- `crates/andromeda-proto/proto/andromeda/protocol/v1/`
- `crates/andromeda-proto/proto/andromeda/contract/v1/`
- `crates/andromeda-proto/tests/protobuf_determinism_tests.rs`
- `crates/andromeda-proto/tests/protocol_contract.rs`
