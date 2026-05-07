# DEC-013 Local Vertical Prototype

## Status

Accepted.

## Context

Phase 0 established Rust contract baselines. The next implementation step is a local vertical prototype
that proves the ordering of contract validation, transaction creation, WAL coverage, visible commit,
result metadata, and trace production.

This prototype must not claim production persistence. It uses an in-memory WAL to exercise the
semantics before disk formats, QUIC runtime, Protobuf code generation, and the SRPL compiler are ready.

## Decision

Implement a std-only local vertical path:

1. `andromeda-catalog` provides a validated `Inventory.ReserveStock` Procedure contract test vector.
2. `andromeda-proto` provides helpers for validated RPC execute, completion, and error envelopes.
3. `andromeda-quic` validates frame payload length and stream role policy.
4. `andromeda-storage` provides `InMemoryWal` with monotonic LSN, simulated durable flush, and durable
   replay filtering.
5. `andromeda-exec` provides `LocalVerticalRuntime`, which validates the Procedure contract before
   creating a transaction, appends WAL records, flushes through commit LSN, and only then marks the
   transaction committed.
6. `andromeda-cli vertical` runs the local prototype and prints the committed status, rows affected,
   durable WAL LSN, durable record count, and contract validation trace.

## Invariants Preserved

- Contract mismatch is rejected before transaction creation.
- Commit visibility requires a durable WAL LSN.
- Result metadata is validated before payload.
- The WAL replay view includes only durable records.
- This remains local and in-memory; no disk durability, network transport, gRPC, or runtime JSON is
  introduced.

## Validation

- `cargo run -p andromeda-cli -- vertical`
- `cargo test -p andromeda-storage -p andromeda-tx`
- `cargo test -p andromeda-catalog -p andromeda-proto -p andromeda-quic`
- `cargo test -p andromeda-exec`
- `cargo check --workspace`
- `cargo test --workspace`
