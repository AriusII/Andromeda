# andromeda-rpc

## Purpose

`andromeda-rpc` owns runtime-free RPC orchestration glue between frame protocol validation and typed payload envelope validation.

## Scope

- Defines frame dispatch policy and transport surface validation.
- Reexports the runtime-free dispatch interfaces used by clients and runtime integration layers.
- Separates API-facing types from transport and wire codecs.

## Non-goals

- Own low-level frame codecs, wire encoding, or byte-level protocol evolution.
- Own transport sockets, QUIC listeners, TLS policy, or backpressure scheduling.
- Own Procedure execution, catalog writes, WAL durability, or recovery behavior.

## Prerequisites

- Frame contracts come from `andromeda-rpc-protocol`.
- Typed envelope and payload validation comes from `andromeda-rpc-codec`.
- Transport runtime integration remains outside this crate.

## Procedure

1. Keep dispatch policy runtime-free and deterministic.
2. Route typed envelope validation to `andromeda-rpc-codec`.
3. Route frame bytes, frame type codes, and stream roles to `andromeda-rpc-protocol`.
4. Keep transport runtime integration in `andromeda-quic` or a runtime-specific adapter.

## Validation

- Inspect `src/lib.rs` and `src/dispatch.rs` for runtime-free dispatch scope.
- When validating by command, use `cargo check -p andromeda-rpc --all-targets`.

## Troubleshooting

- If API and protocol boundaries overlap, push transport concerns to `andromeda-rpc-protocol` and related codec crates.
- If typed Procedure payloads are unclear, align with `andromeda-procedure-contract` and `andromeda-contract`.

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/andromeda-rpc-protocol`
- `crates/andromeda-procedure-runtime`
- `crates/andromeda-procedure-contract`

