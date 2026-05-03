# DEC-014 Rust Crate Module Structure

## Status

Accepted.

## Context

The initial Rust foundation created correct Cargo crate boundaries, but several crates still kept
their domain contracts in single `lib.rs` files. That was acceptable for bootstrap validation, but it
does not scale for Andromeda's storage, transaction, protocol, catalog, and execution workstreams.

The codebase needs an internal topology that supports focused implementation, review, tests, and
future subagent ownership without changing public crate boundaries.

## Decision

Keep each crate public API stable through `src/lib.rs`, but make `lib.rs` only declare modules and
re-export the intentional public surface. Domain code must live in focused modules:

- `andromeda-catalog`: `batch`, `contracts`, `fixtures`, `names`, `objects`.
- `andromeda-proto`: `completion`, `envelope`, `errors`, `manifest`, `payload`, `structured`,
  `version`.
- `andromeda-quic`: `backpressure`, `frame`, `stream`.
- `andromeda-srpl`: `cardinality`, `diagnostics`, `signature`, `source`.
- `andromeda-tx`: `mvcc`, `state`, `trace`.
- `andromeda-storage`: `lsn`, `manifest`, `page`, `recovery`, `wal`.
- `andromeda-exec`: `invocation`, `local`, `result`, `wal`.
- `andromeda-observe`: `events`, `trace_id`.

Unit tests stay close to the module they validate. Public types remain re-exported at crate root so
existing consumers do not couple to private module paths.

## Invariants Preserved

- No external runtime dependencies are introduced.
- No gRPC, ad hoc SQL, or normative runtime JSON is introduced.
- `andromeda-core` remains independent from all other Andromeda crates.
- WAL-before-visible-commit and contract-before-transaction semantics remain unchanged.
- Existing public crate imports keep working through crate-root re-exports.

## Validation

- `cargo fmt --all`
- `cargo check --workspace`
- `cargo test --workspace`
- `cargo run -p andromeda-cli -- vertical`
