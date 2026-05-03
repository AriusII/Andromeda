# DEC-011 Rust Workspace Topology

## Status

Accepted.

## Context

Andromeda needs a Rust foundation that matches the master architecture before runtime behavior is
implemented. The current repository had one root binary package and no engine crate boundaries.

The master architecture requires explicit boundaries between core policy, Protobuf wire contracts,
QUIC framing, catalog metadata, SRPL compilation, transactions, storage, execution, and observability.
It also requires that Protobuf remains a boundary contract format, QUIC remains separate from catalog
mutation logic, and visible commit remains tied to durable WAL coverage.

## Decision

Use a virtual Cargo workspace with lowercase package names under `crates/`.

The initial workspace contains:

- `andromeda-core`
- `andromeda-proto`
- `andromeda-quic`
- `andromeda-catalog`
- `andromeda-srpl`
- `andromeda-tx`
- `andromeda-storage`
- `andromeda-exec`
- `andromeda-observe`
- `andromeda-cli`

The first pass uses only the Rust standard library. It does not introduce Tokio, Quinn, prost, tracing,
gRPC, runtime JSON, or application-facing ad hoc SQL.

## Consequences

- Each crate can compile and test independently while preserving dependency direction.
- Generated Protobuf code can be added later without leaking generated structs into catalog,
  optimizer, transaction, or storage logic.
- QUIC implementation selection remains open until protocol contracts and benchmark criteria are ready.
- The root package is no longer a binary. The diagnostic binary lives in `andromeda-cli`.

## Validation

- `cargo check --workspace`
- `cargo test --workspace`
- Verify that root `src/main.rs` is removed.
- Verify that the new Rust crates do not introduce gRPC, ad hoc SQL surfaces, or runtime JSON as a
  normative protocol path.
