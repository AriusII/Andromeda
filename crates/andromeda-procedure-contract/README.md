# andromeda-procedure-contract

## Purpose

`andromeda-procedure-contract` owns typed Procedure contract descriptors, canonical Procedure contract hashes, policy versions, binding evidence, and compatibility diagnostics.

Application behavior in Andromeda is exposed through typed, cataloged Procedures. This crate keeps that boundary runtime-free: it describes the contract that catalog, SRPL, execution, and protocol layers bind to, without owning storage or invocation runtime behavior.

## Scope

- Owns `ProcedureContract`, `ProcedureContractCandidate`, `ProcedureContractRef`, and `ProcedureContractBinding`.
- Owns canonical Procedure contract hash and `PolicyVersion` materialization.
- Owns result stream contracts, transaction policy, compatibility policy, result metadata policy, error policy, and multi-result policy descriptors.
- Owns source-generator-ready Procedure manifests, manifest binding evidence, required permission descriptors, and result-stream manifest descriptors.
- Owns runtime-free RPC completion summaries and transaction outcome contracts used by protocol projections.
- Owns minimal catalog identity primitives required by Procedure contracts: `QualifiedName`, `CatalogObjectRef`, and `ObjectKind`.
- Provides compatibility diagnostics for Procedure contract evolution.

## Non-goals

- Own transport details, wire formats, or QUIC runtime integrations.
- Own storage, WAL, recovery, or catalog persistence logic.
- Introduce or execute ad hoc application SQL or dynamic query text.
- Perform database execution, transaction scheduling, or IAM enforcement.

## Procedure

1. Model Procedure contract changes as typed descriptors or newtypes.
2. Keep canonical materialization deterministic and little-endian.
3. Include any compatibility-relevant field in the contract hash or document why it is excluded.
4. Validate `ProcedureContractBinding` before persisting or accepting binding evidence.
5. Keep catalog storage, WAL, RPC, and execution behavior in their owner crates.

## Validation

- For Procedure contract changes, run:
  - `cargo fmt --all --check`
  - `cargo check -p andromeda-procedure-contract --all-targets`

## Troubleshooting

- If a contract hash changes unexpectedly, compare every canonicalized field in `src/hash.rs`.
- If compatibility diagnostics reject an additive change, check whether the changed field is intentionally immutable under `CompatibilityPolicy::AdditiveOnly`.
- If a downstream crate needs the historical `andromeda_contract::*` or `andromeda_proto::*` surface, use the compatibility reexports from those crates.

## References

- `AGENTS.md`
- `crates/AGENTS.md`
- `crates/andromeda-contract`
