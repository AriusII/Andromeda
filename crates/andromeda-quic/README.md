# andromeda-quic

## Purpose

`andromeda-quic` owns Andromeda's QUIC transport boundary, including runtime-free transport contracts, stream role validation, lifecycle gating, backpressure, typed envelope validation, Procedure gateway admission, reconnect policy, HA/DR stream allocation, and the optional Quinn-backed runtime adapter.

This crate translates QUIC transport events into typed Andromeda RPC frames. It must not redefine Procedure semantics, authorization policy, storage truth, WAL durability, recovery behavior, or catalog ownership.

## Scope

This crate is responsible for:

| Area | Responsibility |
| --- | --- |
| Transport boundary | Runtime-free transport traits, endpoint metadata, message shapes, shutdown state, cancellation status, and backpressure status. |
| Optional Quinn runtime | `runtime-quinn` feature for concrete Quinn, Rustls, and Tokio adapters; disabled by default. |
| Surface gating | Session lifecycle and listener contracts for Application, Administration, HighAvailability, and Monitoring planes. |
| Procedure gateway | Pre-dispatch admission for Application-surface Procedure invocation frames. |
| Typed envelopes | Validation of Protobuf `FrameEnvelope` payloads against frame context, contract hash, catalog version, and ResultStream bounds. |
| Catalog manifest resolution | Administration-surface manifest resolution frames and typed manifest projection. |
| Reconnect policy | Runtime-free reconnect, retry admission, certificate continuity, and pool policy contracts. |
| HA/DR streams | Reserved stream ranges and multiplexing rules for heartbeat, vote, WAL shipping, and reserved traffic. |

`andromeda-quic` reexports selected `andromeda-rpc-protocol` frame and stream contracts as a compatibility facade during migration. The canonical runtime-free frame owner remains `andromeda-rpc-protocol`.

## Non-goals

- Do not introduce gRPC, generated service definitions, HTTP/2 RPC compatibility, `tonic`, `grpcio`, or `prost-grpc`.
- Do not introduce application-facing ad hoc SQL, generic command text, dynamic table names, or dynamic predicates.
- Do not bypass typed, cataloged Procedure contracts. Application dispatch must use typed `RpcExecuteRequest` payloads bound to resolved Procedure manifests.
- Do not expose Administration, HA/DR, recovery, security management, or cluster capabilities through the Application Surface.
- Do not own Procedure execution semantics, transaction creation, durable WAL, visible commit rules, storage truth, recovery replay, IAM policy stores, or audit ledgers.
- Do not make the optional Quinn runtime a default dependency or a requirement for runtime-free protocol tests.

## Prerequisites

Before changing this crate, understand:

- Application traffic is restricted to typed Procedure invocation and result streams.
- Administration traffic is separate and is used for control-plane operations such as catalog manifest resolution.
- HA/DR streams use reserved ranges and must not enter Application dispatch.
- `andromeda-rpc-protocol` owns frame bytes and stream-role contracts; `andromeda-proto` owns generated message schemas.
- Procedure gateway admission must happen before executor dispatch and before transaction creation.
- mTLS identity, surface plane, manifest contract hash, catalog version, stats version, and required execute permission must match before routing a Procedure invocation.

## Procedure

1. Keep runtime-free contracts available without enabling `runtime-quinn`.
2. Put concrete Quinn, Rustls, Tokio, socket, and TLS behavior behind the `runtime-quinn` feature.
3. Validate connection lifecycle before dispatch. Handshake, active, drain, and close states must gate transport behavior.
4. Validate surface plane and certificate scope before Procedure gateway construction.
5. For Application Procedure routes, require an Application stream ID, `RpcExecuteRequest` frame type, no client-supplied transaction id, matching frame envelope, matching manifest, matching contract hash, matching catalog version, matching stats version, and `andromeda.execute_procedure`.
6. Reject Administration, HA/DR, monitoring, reserved, and future stream IDs from Application Procedure dispatch.
7. Keep ResultStream emission bound to the admitted Procedure route context.

## Validation

For documentation-only changes, validate this README against `AGENTS.md`, `docs/adr/ADR-0011-workspace-crate-boundaries.md`, and `docs/adr/ADR-0012-quic-rpc-no-grpc.md`.

For code changes in this crate, choose the narrowest applicable gate:

```bash
cargo fmt --all --check
cargo test -p andromeda-quic --tests
cargo test -p andromeda-quic --test procedure_gateway_route
cargo test -p andromeda-quic --test protocol_stability_contract
cargo test -p andromeda-quic --test transport_contract
cargo check -p andromeda-quic --all-targets
```

For Quinn-backed behavior, also run the feature-gated tests:

```bash
cargo test -p andromeda-quic --features runtime-quinn --test real_quinn_network
cargo test -p andromeda-quic --features runtime-quinn --test reconnect_quinn_admission_contract
```

Security, RPC, HA/DR, or recovery-adjacent behavior changes require targeted admission, surface-separation, and protocol-stability evidence in addition to compilation.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| Application dispatch accepts an Administration or HA/DR stream. | Reject before Procedure dispatch and keep the stream range reserved for its owning surface. |
| A Procedure route accepts raw SQL text. | Reject it; Application dispatch must carry a typed `RpcExecuteRequest` bound to a cataloged Procedure manifest. |
| A manifest protocol layout mentions gRPC. | Reject the manifest as protocol drift and use Andromeda RPC over QUIC with Protobuf message payloads. |
| Quinn types leak into default builds. | Move them behind `runtime-quinn` and keep runtime-free tests passing without the feature. |
| A route starts transaction work before admission completes. | Move transaction work behind successful surface, identity, frame, manifest, contract, and permission validation. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `docs/adr/ADR-0012-quic-rpc-no-grpc.md`
- `crates/andromeda-quic/src/lib.rs`
- `crates/andromeda-quic/src/procedure_gateway/route.rs`
- `crates/andromeda-quic/src/procedure_gateway/validation.rs`
- `crates/andromeda-quic/src/catalog_manifest_resolution/validation.rs`
- `crates/andromeda-quic/tests/procedure_gateway_route.rs`
- `crates/andromeda-quic/tests/protocol_stability_contract.rs`
