# andromeda-contract

## Purpose

`andromeda-contract` is the compatibility surface and catalog-object descriptor crate for Andromeda contract surfaces.

Use this crate when code needs the historical unified contract surface: catalog object descriptors, structural dependencies, Procedure contract reexports from `andromeda-procedure-contract`, and StructuredObject metadata reexports from `andromeda-structured-object`.

Application behavior in Andromeda is exposed through typed, cataloged Procedures. This crate helps preserve that boundary by making contract identity, shape, dependencies, and compatibility explicit before execution, transport dispatch, or catalog publication.

## Scope

This crate is responsible for:

| Area | Responsibility |
| --- | --- |
| Procedure contracts | Reexports from `andromeda-procedure-contract` for compatibility with existing `andromeda_contract::*` imports. |
| Contract identity | Reexports Procedure `ContractHash` and policy-version materialization from `andromeda-procedure-contract`; owns catalog object shape hashes. |
| Catalog objects | Runtime-free descriptors for tables, Procedures, StructuredObjects, enums, and versioned object references. |
| Names and dependencies | Reexports qualified names and owns structural dependency edges derived from contract-safe definitions. |
| Compatibility | Reexports Procedure contract compatibility diagnostics across catalog versions. |

The crate is an R1 contract crate. It may describe what a Procedure contract means, but it does not execute a Procedure, resolve an invocation, open a transaction, write WAL, or publish catalog changes.

## Non-goals

- Do not introduce application-facing ad hoc SQL, generic command text, dynamic table names, or dynamic predicates.
- Do not bypass typed, cataloged Procedure contracts.
- Do not own SRPL parsing, binding, lowering, optimization, or execution.
- Do not own catalog storage, DefinitionBatch application, catalog publication, WAL, MVCC, recovery, or storage truth.
- Do not own RPC frames, QUIC transport, Protobuf schemas, generated wire messages, TLS, IAM runtime state, or audit sinks.
- Do not serialize Rust native structs directly to disk or network. Persistent and network formats must use explicit codecs or generated protocol contracts owned by the appropriate crate.

## Prerequisites

Before changing this crate, understand:

- Andromeda application access is RPC-only through typed, cataloged Procedures.
- Contract hashes must be stable, deterministic, and reviewable.
- Catalog object descriptors are contract evidence, not catalog storage.
- Result stream metadata and row-count policy must stay explicit.
- This crate must stay free of execution engines, durable-kernel implementations, QUIC runtime dependencies, `prost`, SQL crates, benchmark crates, analytics crates, and GPU execution crates.

## Procedure

1. Model new public concepts as typed descriptors or newtypes instead of primitive aliases.
2. Keep `lib.rs` limited to module declarations and intentional reexports.
3. Keep Procedure contract materialization in `andromeda-procedure-contract`; keep catalog object shape hashing here.
4. Derive dependencies from contract-safe definitions. Do not reach into catalog stores or execution state to infer dependencies.
5. Keep compatibility checks explicit. A changed Procedure shape should produce a diagnostic rather than an implicit accept or reject.
6. Keep Application and Administration concerns separate. Application contracts describe typed Procedure invocation; administrative capabilities belong behind Administration, security, recovery, or cluster surfaces.

## Validation

For documentation-only changes, validate the README against the doctrine in `AGENTS.md`, `crates/AGENTS.md`, and `docs/adr/ADR-0011-workspace-crate-boundaries.md`.

For code changes in this crate, prefer:

```bash
cargo fmt --all --check
cargo test -p andromeda-contract --tests
cargo test -p andromeda-contract --test contract_hash_golden
cargo check -p andromeda-procedure-contract --all-targets
cargo check -p andromeda-contract --all-targets
```

If a change affects contract hash materialization, add or update golden vectors and explain the compatibility impact.

## Troubleshooting

| Symptom | Corrective action |
| --- | --- |
| A caller wants to pass raw SQL text through a Procedure contract. | Replace the raw text with a typed Procedure name, contract hash, catalog version, and structured arguments. |
| A descriptor needs runtime catalog lookup to validate. | Move the runtime lookup to catalog or execution code; keep only the runtime-free descriptor and compatibility rule here. |
| A field affects invocation compatibility but is absent from hashing. | Add it to canonical materialization or document why it is non-contractual. |
| A change requires RPC frame details or Protobuf field tags. | Put the wire concern in `andromeda-rpc-protocol` or `andromeda-proto`; reference only stable contract identities here. |
| A new permission or surface rule appears in a Procedure descriptor. | Use `andromeda-security-contract` vocabulary instead of defining security policy in this crate. |

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `docs/adr/ADR-0011-workspace-crate-boundaries.md`
- `crates/andromeda-contract/src/lib.rs`
- `crates/andromeda-procedure-contract/src/lib.rs`
- `crates/andromeda-contract/src/objects.rs`
- `crates/andromeda-contract/tests/contract_hash_golden.rs`
