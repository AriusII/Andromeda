# andromeda-result-stream

## Purpose

`andromeda-result-stream` is the future R3 owner for execution result sequencing.

ResultStream behavior must emit metadata before payload, enforce bounded payload batches, validate completion, and carry terminal evidence without becoming transport, storage, or protocol byte ownership.

This scaffold is not a promoted Cargo workspace member until a later packet adds a manifest, root workspace wiring, topology tests, and compatibility evidence.

## Scope

This future crate owns:

- Execution-facing ResultStream metadata policy.
- Metadata-before-payload sequencing.
- Bounded batch emission, backpressure state, row-count policy, and completion validation.
- Terminal completion evidence that can be correlated with transaction and WAL durability evidence.
- Typed result-stream errors for missing metadata, duplicate metadata, payload-after-completion, row-count mismatch, and missing terminal evidence.

## Non-goals

This future crate does not own:

- QUIC streams, sockets, TLS, concrete transport runtime, or frame byte codecs.
- Protobuf schema generation, RPC frame type codes, or wire header layouts.
- Procedure contract definitions, Procedure execution, transaction authority, WAL records, storage truth, or recovery replay.
- Application-facing SQL, raw command text, dynamic predicates, or shape-shifting returns.
- Business hardcoding or application-specific result policies.

## Prerequisites

Before adding behavior here, confirm:

- Result metadata is derived from a typed Procedure contract and admitted route context.
- The sequence starts with metadata before any payload batch.
- Completion carries terminal transaction state and durable evidence when the result is committed or rolled back.
- Backpressure is bounded and observable.
- Payload validation does not treat transient memory or trace output as database truth.

## Procedure

1. Receive result metadata from execution orchestration before any payload data.
2. Validate stream identity, Procedure contract context, cardinality, row-count policy, and bounds.
3. Accept zero or more bounded payload batches only after metadata.
4. Reject payloads after completion and duplicate metadata.
5. Validate terminal completion evidence and row-count consistency.
6. Expose typed evidence to protocol and trace layers without owning their byte formats.

## Validation

For the scaffold, validate that only README and `src/lib.rs` files were added under this directory.

Before promoting this crate into the workspace, add and run:

```powershell
cargo test -p andromeda-result-stream --tests
cargo test -p andromeda-cli --test workspace_dependency_topology -- --nocapture
cargo test -p andromeda-cli --test orphan_source_invariants -- --nocapture
```

Runtime promotion must include tests for metadata-before-payload, duplicate metadata rejection, payload-before-metadata rejection, payload-after-completion rejection, bounded backpressure, exact row-count policy, missing terminal evidence rejection, no application-facing SQL, and no business hardcoding.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| Payload appears before metadata | Reject the sequence and require metadata as the first result event. |
| Completion lacks durable evidence | Keep the stream incomplete until terminal transaction evidence is available. |
| Backpressure grows without bounds | Enforce capacity and byte-budget limits before accepting payload batches. |
| Result shape changes per row | Bind the stream to the Procedure contract shape and reject drift. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/README.md`
- `crates/andromeda-exec/README.md`
- `crates/andromeda-rpc-protocol/README.md`
- `docs/adr/ADR-0018-engine-crate-mapping-policy.md`
- `documentations/implementation/target-crate-gap-ledger-2026-05-08.md`
