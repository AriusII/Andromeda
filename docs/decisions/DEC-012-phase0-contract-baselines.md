# DEC-012 Phase 0 Contract Baselines

## Status

Accepted.

## Context

The Rust workspace now needs executable Phase 0 baselines for the master architecture. These baselines
must stay strict and limited: they define boundaries, validation rules, and test vectors without implying
that the database engine can execute user workloads yet.

The affected surfaces are type descriptors, catalog objects, Procedure contracts, Protobuf boundary
envelopes, QUIC frame policy, SRPL core semantics, transaction state, WAL records, page metadata,
manifests, execution admission, and observability traces.

## Decision

Implement Phase 0 as std-only Rust contracts inside the existing crates:

- `andromeda-core` owns common errors, identifiers, timestamps, hardware policy, and type descriptors.
- `andromeda-catalog` owns `QualifiedName`, object references, Procedure contracts, and
  `DefinitionBatch::dry_run`.
- `andromeda-proto` owns boundary envelopes, payload families, manifest descriptors, result stream
  descriptors, error envelopes, and completion status.
- `andromeda-quic` owns frame taxonomy, stream role policy, and backpressure reasons.
- `andromeda-srpl` owns strict cardinality, Procedure signature shapes, diagnostics, and forbidden
  core construct detection.
- `andromeda-tx` owns the transaction state machine and MVCC row visibility helper.
- `andromeda-storage` owns WAL record taxonomy, page header/trailer contracts, manifest validation,
  and recovery plan creation.
- `andromeda-exec` owns pre-transaction invocation validation and result metadata checks.
- `andromeda-observe` owns stable trace event shapes and explanation checks.

No runtime dependency is introduced. Protobuf remains a boundary model, QUIC remains a frame/session
boundary, and generated Protobuf types are still future work.

## Invariants Preserved

- No application-facing ad hoc SQL surface.
- No gRPC.
- No runtime JSON as the normative protocol path.
- No external network or filesystem access from SRPL core semantics.
- Commit visibility requires a durable WAL boundary.
- Result batch payloads require metadata and exact row count where cardinality requires it.
- Storage recovery remains snapshot plus WAL.

## Validation

- Unit tests cover Phase 0 test vectors in the owning crates.
- `cargo check --workspace` must pass.
- `cargo test --workspace` must pass.
- Doctrine scans must find no gRPC, ad hoc SQL surface, runtime JSON dependency, Tokio, Quinn, prost,
  or tracing dependency in the Rust workspace.
