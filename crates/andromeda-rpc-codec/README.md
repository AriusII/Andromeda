# andromeda-rpc-codec

## Purpose

`andromeda-rpc-codec` owns runtime-free RPC payload encoding, decoding, and typed envelope validation helpers below RPC orchestration.

## Scope

- Own serialization and deserialization helpers for RPC payloads used by runtime adapters.
- Own typed `FrameEnvelope` validation against frame context, contract hash, catalog version, and ResultStream bounds.
- Define deterministic codec boundaries for protocol-specific payload formats.
- Keep codec primitives separate from higher-level orchestration and transport.

## Non-goals

- Do not own transport adapters, sockets, retries, admission, retry policy execution, database semantics, Procedure execution, catalog mutation, WAL, storage, or recovery.
- Do not replace frame byte contracts owned by `andromeda-rpc-protocol` or wire contracts owned by `andromeda-proto-wire`.

## Prerequisites

- Callers provide typed frame context and bounds before envelope validation.
- Transport runtime state remains outside this crate.
- Wire contract changes are coordinated with the owning protocol crates.

## Procedure

1. Keep this crate codec-only with explicit, deterministic APIs.
2. Keep conversions boundary-limited and free of ambient runtime state.
3. Leave protocol orchestration to `andromeda-rpc` and transport to `andromeda-quic`.

## Validation

- Inspect `src/lib.rs`, `src/protobuf.rs`, `src/result_metadata.rs`, and `src/typed_envelope.rs` for codec-only behavior.
- When validating by command, use `cargo check -p andromeda-rpc-codec --all-targets`.

## Troubleshooting

- If payload framing changes, synchronize with `andromeda-rpc-protocol` and protocol integration tests.
- If codec behavior is unstable, pin conversion points and keep temporary adapters local to this crate.

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/andromeda-rpc-protocol`
- `crates/andromeda-proto-wire`
